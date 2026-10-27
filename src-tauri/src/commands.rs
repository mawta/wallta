use crate::{models::*, providers, rotation, storage::{self, Store}, wallpaper};
use chrono::{Datelike, Timelike};
use std::{collections::HashSet, fs, io::Read, path::PathBuf, sync::{Arc, Mutex}, time::{Duration, Instant}};
use tauri::{Emitter, Manager};

pub struct Engine { pub inner: Mutex<Runtime>, pub store: Store, pub client: reqwest::blocking::Client }
pub struct Runtime { pub state: AppState, pub next_due: Instant, pub wallpaper_path: Option<PathBuf> }
impl Engine {
    pub fn new(store: Store) -> Result<Self, String> {
        let state = match store.load() { Ok(s) => s, Err(e) => AppState { error: Some(e), ..Default::default() } };
        Ok(Self { inner: Mutex::new(Runtime { state, next_due: Instant::now(), wallpaper_path: None }), store, client: providers::client()? })
    }
    pub fn snapshot(&self) -> Result<AppState, String> { let mut runtime = self.inner.lock().map_err(|_| "Wallpaper service lock failed.".to_string())?; Ok(storage::public_state(&mut runtime.state)) }
    fn list(&self, state: &AppState) -> (Vec<MediaItem>, Vec<String>) {
        let mut images = vec![]; let mut errors = vec![];
        let wifi = !state.settings.wifi_only || wallpaper::wifi_connected();
        for source in state.sources.iter().filter(|s| s.enabled) {
            if source.kind != "local" && !wifi { errors.push(format!("{}: Waiting for Wi-Fi.", source.name)); continue; }
            match providers::list_source(&self.client, source) { Ok(mut items) => images.append(&mut items), Err(e) => errors.push(format!("{}: {e}", source.name)) }
        }
        let mut ids = HashSet::new(); images.retain(|i| ids.insert(i.id.clone())); (images, errors)
    }
    pub fn list_media(&self) -> Result<Vec<MediaItem>, String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        let (images, errors) = self.list(&runtime.state); runtime.state.error = (!errors.is_empty()).then(|| errors.join("\n")); Ok(images)
    }
    pub fn rotate(&self, automatic: bool) -> Result<AppState, String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        if automatic {
            let now = chrono::Local::now();
            if !runtime.state.running || Instant::now() < runtime.next_due || !rotation::schedule_allows(&runtime.state.settings, now.weekday().num_days_from_sunday(), now.hour() * 60 + now.minute()) { return Ok(storage::public_state(&mut runtime.state)); }
        }
        let (mut items, mut errors) = self.list(&runtime.state);
        let previous = runtime.state.current.as_ref().and_then(|c| items.iter().position(|i| i.id == c.id));
        let index = rotation::choose_index(items.len(), previous, runtime.state.settings.order == "shuffle");
        if let Some(index) = index { items.rotate_left(index); }
        // Try another image if the first item is corrupt. Never replace current on failure.
        let mut applied = false;
        for media in &items {
            if items.len() > 1 && runtime.state.current.as_ref().is_some_and(|c| c.id == media.id) { continue; }
            let source = runtime.state.sources.iter().find(|s| s.id == media.source_id).unwrap();
            match self.prepare(media, source, &runtime.state.settings).and_then(|bytes| wallpaper::apply(&bytes, &runtime.state.settings, &self.store.cache)) {
                Ok(path) => { runtime.wallpaper_path = Some(path); runtime.state.current = Some(media.clone()); runtime.state.last_changed = Some(chrono::Utc::now().to_rfc3339()); applied = true; break; }
                Err(e) => errors.push(format!("{}: {e}", media.name)),
            }
        }
        if items.is_empty() && errors.is_empty() { errors.push("No images found. Enable a source and add a supported image.".into()); }
        runtime.state.error = (!errors.is_empty()).then(|| errors.join("\n"));
        runtime.next_due = Instant::now() + Duration::from_secs(if applied { runtime.state.settings.interval_seconds } else { 30 });
        self.trim_cache(runtime.state.settings.cache_limit_mb, runtime.wallpaper_path.as_deref())?;
        self.store.save(&runtime.state)?;
        Ok(storage::public_state(&mut runtime.state))
    }
    fn prepare(&self, media: &MediaItem, source: &Source, settings: &Settings) -> Result<Vec<u8>, String> {
        if source.kind == "local" {
            let mut bytes = Vec::new(); fs::File::open(&media.url).map_err(|_| "Local image is no longer accessible.".to_string())?.take(providers::MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if bytes.len() as u64 > providers::MAX_IMAGE_BYTES { return Err("Local image exceeds 32 MB.".into()); } return Ok(bytes);
        }
        let path = self.store.cache.join(format!("{}.img", media.id));
        if path.exists() { return fs::read(path).map_err(|e| e.to_string()); }
        let bytes = providers::download(&self.client, media, source)?;
        // Decode before caching: HTML/error pages and broken images never become usable cache entries.
        wallpaper::render(&bytes, settings, (1,1))?;
        storage::atomic_write(&path, &bytes)?; Ok(bytes)
    }
    fn trim_cache(&self, limit_mb: u64, keep: Option<&std::path::Path>) -> Result<(), String> {
        let mut files: Vec<_> = fs::read_dir(&self.store.cache).map_err(|e| e.to_string())?.filter_map(Result::ok).filter_map(|e| e.metadata().ok().filter(|m| m.is_file()).map(|m| (e.path(), m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)))).collect();
        let mut total: u64 = files.iter().map(|(_,n,_)| n).sum(); files.sort_by_key(|(_,_,time)| *time);
        for (path, size, _) in files { if total <= limit_mb * 1024 * 1024 { break; } if keep == Some(path.as_path()) { continue; } if fs::remove_file(path).is_ok() { total = total.saturating_sub(size); } }
        Ok(())
    }
    pub fn set_running(&self, running: bool) -> Result<AppState, String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        let mut next = runtime.state.clone(); next.running = running; next.error = None; self.store.save(&next)?;
        runtime.state = next; runtime.next_due = Instant::now(); Ok(storage::public_state(&mut runtime.state))
    }
}

pub fn allow_local_assets(app: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    for source in state.sources.iter().filter(|s| s.kind == "local" && s.enabled) { if let Ok(path) = fs::canonicalize(&source.location) { app.asset_protocol_scope().allow_directory(path, source.recursive).map_err(|e| e.to_string())?; } }
    Ok(())
}
pub fn start_worker(app: tauri::AppHandle, engine: Arc<Engine>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let before = engine.snapshot().ok().and_then(|s| s.last_changed);
        match engine.rotate(true) {
            Ok(state) => { if state.last_changed != before { let _ = app.emit("wallta:state", state); } }
            Err(error) => { if let Ok(mut runtime) = engine.inner.lock() { runtime.state.error = Some(error); runtime.next_due = Instant::now() + Duration::from_secs(30); } }
        }
    });
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T,String> + Send + 'static) -> Result<T,String> { tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())? }
#[tauri::command] pub async fn get_state(engine: tauri::State<'_, Arc<Engine>>) -> Result<AppState,String> { let engine = engine.inner().clone(); blocking(move || engine.snapshot()).await }
#[tauri::command] pub async fn list_media(engine: tauri::State<'_, Arc<Engine>>) -> Result<Vec<MediaItem>,String> { let engine = engine.inner().clone(); blocking(move || engine.list_media()).await }
#[tauri::command] pub async fn next_wallpaper(engine: tauri::State<'_, Arc<Engine>>) -> Result<AppState,String> { let engine = engine.inner().clone(); blocking(move || engine.rotate(false)).await }
#[tauri::command] pub async fn set_running(running: bool, engine: tauri::State<'_, Arc<Engine>>) -> Result<AppState,String> { let engine = engine.inner().clone(); blocking(move || engine.set_running(running)).await }
#[tauri::command] pub async fn pick_folder() -> Result<Option<String>, String> { blocking(|| Ok(rfd::FileDialog::new().set_title("Choose wallpaper folder").pick_folder().map(|p| p.to_string_lossy().into_owned()))).await }
#[tauri::command] pub async fn save_state(mut state: AppState, app: tauri::AppHandle, engine: tauri::State<'_, Arc<Engine>>) -> Result<AppState,String> {
    let engine = engine.inner().clone();
    blocking(move || {
        if state.version != 1 { return Err("Unsupported state version.".into()); } state.settings.validate()?;
        if state.sources.len() > 100 { return Err("At most 100 sources are supported.".into()); }
        let mut runtime = engine.inner.lock().map_err(|e| e.to_string())?;
        let mut ids = HashSet::new();
        for source in &mut state.sources {
            if source.id.is_empty() || source.id.len() > 128 || !source.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || !ids.insert(source.id.clone()) { return Err("Source IDs must be unique and contain letters, digits, hyphens or underscores.".into()); }
            if !["local","url","manifest","webdav","drive","photos"].contains(&source.kind.as_str()) { return Err("Unknown source type.".into()); }
            if source.name.trim().is_empty() || source.location.trim().is_empty() { return Err("Every source needs a name and location.".into()); }
            if ["url","manifest","webdav"].contains(&source.kind.as_str()) { providers::network_url(&source.location)?; }
            storage::merge_credentials(source, runtime.state.sources.iter().find(|s| s.id == source.id));
        }
        // Native runtime owns wallpaper status. A stale UI cannot overwrite a successful background change.
        state.current = runtime.state.current.clone(); state.last_changed = runtime.state.last_changed.clone(); state.error = None;
        if state.settings.autostart != runtime.state.settings.autostart { wallpaper::set_autostart(state.settings.autostart)?; }
        allow_local_assets(&app, &state)?;
        engine.store.save(&state)?;
        for old in &runtime.state.sources { if !state.sources.iter().any(|s| s.id == old.id) { engine.store.forget(old)?; } }
        runtime.state = state; runtime.next_due = Instant::now();
        engine.trim_cache(runtime.state.settings.cache_limit_mb, runtime.wallpaper_path.as_deref())?;
        Ok(storage::public_state(&mut runtime.state))
    }).await
}
#[tauri::command] pub async fn clear_cache(engine: tauri::State<'_, Arc<Engine>>) -> Result<(),String> {
    let engine = engine.inner().clone(); blocking(move || { let runtime = engine.inner.lock().map_err(|e| e.to_string())?; for entry in fs::read_dir(&engine.store.cache).map_err(|e| e.to_string())? { let entry = entry.map_err(|e| e.to_string())?; if runtime.wallpaper_path.as_deref() != Some(entry.path().as_path()) && entry.file_type().map_err(|e| e.to_string())?.is_file() { fs::remove_file(entry.path()).map_err(|e| e.to_string())?; } } Ok(()) }).await
}
#[tauri::command] pub fn get_capabilities() -> Capabilities { Capabilities { platform: if cfg!(windows) { "windows" } else { "browser" }, wallpaper: cfg!(windows), screensaver: false, min_interval_seconds: 10, notes: vec!["Rotation runs in the native process after closing the window. Quit from the tray stops rotation.", "Blurred background applies to Fit mode. Desktop clock overlay is not supported.", "Google sources require your own API credentials. Expired tokens must be reconnected."] } }
#[tauri::command] pub async fn open_external(url: String) -> Result<(),String> { let parsed = providers::network_url(&url)?; if parsed.scheme() != "https" { return Err("External links must use HTTPS.".into()); } blocking(move || open::that(parsed.as_str()).map_err(|e| e.to_string())).await }
