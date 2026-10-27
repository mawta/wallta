use crate::{
    models::{AppState, MediaItem, Source},
    rotation::ShuffleBag,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn public_state(state: &mut AppState) -> AppState {
    let mut clean = state.clone();
    for source in &mut clean.sources {
        source.password = None;
        source.api_key = None;
        source.access_token = None;
    }
    clean
}

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
    pub cache: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub fingerprint: String,
    pub items: Vec<MediaItem>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuntimeState {
    pub shuffle: ShuffleBag,
    pub catalogs: HashMap<String, Catalog>,
    // IDs and hashed source identities only: never persist Photos' temporary URLs.
    pub cache_owners: HashMap<String, CacheOwner>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheOwner {
    pub source_id: String,
    pub fingerprint: String,
}

pub fn own_cache(runtime: &mut RuntimeState, media: &MediaItem, source: &Source) {
    runtime.cache_owners.insert(
        media.id.clone(),
        CacheOwner {
            source_id: source.id.clone(),
            fingerprint: source_fingerprint(source),
        },
    );
}
fn cache_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit())
}
pub fn prune_cache_owners(runtime: &mut RuntimeState, cache: &Path) {
    runtime.cache_owners.retain(|id, _| {
        cache_id(id)
            && (cache.join(format!("{id}.img")).is_file()
                || cache.join(format!("{id}-preview.png")).is_file())
    });
}

pub fn source_fingerprint(source: &Source) -> String {
    // Only the hash is persisted. Credential values remain in Credential Manager.
    let identity = serde_json::json!([
        source.id,
        source.kind,
        source.location,
        source.recursive,
        source.enabled,
        source.username,
        source.password,
        source.api_key,
        source.access_token
    ]);
    format!("{:x}", Sha256::digest(identity.to_string().as_bytes()))
}
pub fn offline_items(source: &Source, catalog: Option<&Catalog>, cache: &Path) -> Vec<MediaItem> {
    if !source.enabled || source.kind == "photos" || source.kind == "local" {
        return vec![];
    }
    catalog
        .filter(|c| c.fingerprint == source_fingerprint(source))
        .map(|c| {
            c.items
                .iter()
                .filter(|m| cache.join(format!("{}.img", m.id)).is_file())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}
pub fn reconcile_catalogs(runtime: &mut RuntimeState, sources: &[Source], cache: &Path) {
    // Migrate ownership from older installations before discarding stale catalogs.
    // This index then outlives catalog refreshes, including items no longer listed.
    for (source_id, catalog) in &runtime.catalogs {
        for media in &catalog.items {
            if cache_id(&media.id) {
                runtime
                    .cache_owners
                    .entry(media.id.clone())
                    .or_insert(CacheOwner {
                        source_id: source_id.clone(),
                        fingerprint: catalog.fingerprint.clone(),
                    });
            }
        }
    }
    let enabled: HashMap<_, _> = sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| (source.id.as_str(), source_fingerprint(source)))
        .collect();
    runtime.cache_owners.retain(|id, owner| {
        cache_id(id) && enabled.get(owner.source_id.as_str()) == Some(&owner.fingerprint)
    });
    runtime.catalogs.retain(|id, catalog| {
        enabled.get(id.as_str()) == Some(&catalog.fingerprint)
            && sources
                .iter()
                .any(|source| source.id == *id && source.kind != "photos")
    });
    // Older Photos downloads and stale originals have no recoverable owner.
    // Purging unknown files also handles a crash between writing an index and a file.
    if let Ok(entries) = fs::read_dir(cache) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let id = name
                .strip_suffix(".img")
                .or_else(|| name.strip_suffix("-preview.png"));
            if !id.is_some_and(|id| runtime.cache_owners.contains_key(id))
                && entry.file_type().is_ok_and(|kind| kind.is_file())
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    prune_cache_owners(runtime, cache);
}

impl Store {
    pub fn new(root: PathBuf, cache: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        Ok(Self { root, cache })
    }
    pub fn load(&self) -> Result<AppState, String> {
        let path = self.root.join("state.json");
        if !path.exists() {
            return Ok(AppState::default());
        }
        let mut state: AppState =
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Saved configuration is invalid: {e}"))?;
        if state.version != 1 {
            return Err("Unsupported saved state version.".into());
        }
        for source in &mut state.sources {
            load_secrets(source)?;
        }
        state.settings.validate()?;
        Ok(state)
    }
    pub fn save(&self, state: &AppState) -> Result<(), String> {
        for source in &state.sources {
            save_secrets(source)?;
        }
        let clean = public_state(&mut state.clone());
        let bytes = serde_json::to_vec_pretty(&clean).map_err(|e| e.to_string())?;
        atomic_write(&self.root.join("state.json"), &bytes)
    }
    pub fn forget(&self, source: &Source) -> Result<(), String> {
        crate::google::disconnect(&source.id)?;
        for field in ["password", "apiKey", "accessToken"] {
            put_secret(&source.id, field, None)?;
        }
        Ok(())
    }
    pub fn load_runtime(&self) -> Result<RuntimeState, String> {
        let path = self.root.join("runtime.json");
        if !path.exists() {
            return Ok(RuntimeState::default());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Saved catalog exceeds 32 MB.".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "Saved catalog is invalid.".into())
    }
    pub fn save_runtime(&self, runtime: &RuntimeState) -> Result<(), String> {
        let bytes = serde_json::to_vec(runtime).map_err(|e| e.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Catalog exceeds 32 MB. Use fewer sources or smaller libraries.".into());
        }
        atomic_write(&self.root.join("runtime.json"), &bytes)
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    // Windows rename replaces an existing file through MoveFileExW.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(tmp, path).map_err(|e| e.to_string())
    }
}

pub fn merge_credentials(new: &mut Source, old: Option<&Source>) {
    if let Some(old) = old {
        if new.password.is_none() {
            new.password = old.password.clone();
        }
        if new.api_key.is_none() {
            new.api_key = old.api_key.clone();
        }
        if new.access_token.is_none() {
            new.access_token = old.access_token.clone();
        }
    }
    for field in [&mut new.password, &mut new.api_key, &mut new.access_token] {
        if field.as_ref().is_some_and(String::is_empty) {
            *field = None;
        }
    }
}
fn save_secrets(s: &Source) -> Result<(), String> {
    put_secret(&s.id, "password", s.password.as_deref())?;
    put_secret(&s.id, "apiKey", s.api_key.as_deref())?;
    put_secret(&s.id, "accessToken", s.access_token.as_deref())
}
fn load_secrets(s: &mut Source) -> Result<(), String> {
    s.password = get_secret(&s.id, "password")?;
    s.api_key = get_secret(&s.id, "apiKey")?;
    s.access_token = get_secret(&s.id, "accessToken")?;
    Ok(())
}
#[cfg(windows)]
fn get_secret(id: &str, field: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new("app.wallta.desktop", &format!("{id}:{field}"))
        .map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("Windows Credential Manager could not read the source credential.".into()),
    }
}
#[cfg(windows)]
fn put_secret(id: &str, field: &str, value: Option<&str>) -> Result<(), String> {
    let entry = keyring::Entry::new("app.wallta.desktop", &format!("{id}:{field}"))
        .map_err(|e| e.to_string())?;
    if let Some(value) = value {
        entry.set_password(value).map_err(|_| {
            "Windows Credential Manager could not securely save the source credential.".to_string()
        })
    } else {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Windows Credential Manager could not remove a credential.".into()),
        }
    }
}
#[cfg(not(windows))]
fn get_secret(_: &str, _: &str) -> Result<Option<String>, String> {
    Ok(None)
}
#[cfg(not(windows))]
fn put_secret(_: &str, _: &str, value: Option<&str>) -> Result<(), String> {
    if value.is_some() {
        Err("Secure credential storage is only available in the Windows build.".into())
    } else {
        Ok(())
    }
}
