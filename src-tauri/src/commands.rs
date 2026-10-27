use crate::{
    models::*,
    providers, rotation,
    storage::{self, Catalog, RuntimeState, Store},
    wallpaper,
};
use chrono::{Datelike, Timelike};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::Emitter;

pub struct Engine {
    pub inner: Mutex<Runtime>,
    pub store: Store,
    pub client: reqwest::blocking::Client,
    pub revision: AtomicU64,
}
pub struct Runtime {
    pub state: AppState,
    pub next_due: Instant,
    pub wallpaper_path: Option<PathBuf>,
    pub media: Vec<MediaItem>,
    pub persistent: RuntimeState,
}
impl Engine {
    pub fn new(store: Store) -> Result<Self, String> {
        let mut state = match store.load() {
            Ok(s) => s,
            Err(e) => AppState {
                error: Some(e),
                ..Default::default()
            },
        };
        let mut persistent = match store.load_runtime() {
            Ok(r) => r,
            Err(e) => {
                state.error = Some(e);
                RuntimeState::default()
            }
        };
        storage::reconcile_catalogs(&mut persistent, &state.sources, &store.cache);
        let wallpaper_path = fs::read_dir(store.root.join("wallpaper"))
            .ok()
            .and_then(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|e| e.path().extension().is_some_and(|x| x == "bmp"))
                    .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
                    .map(|e| e.path())
            });
        Ok(Self {
            inner: Mutex::new(Runtime {
                state,
                next_due: Instant::now(),
                wallpaper_path,
                media: Vec::new(),
                persistent,
            }),
            store,
            client: providers::client()?,
            revision: AtomicU64::new(0),
        })
    }
    pub fn snapshot(&self) -> Result<AppState, String> {
        let mut runtime = self
            .inner
            .lock()
            .map_err(|_| "Wallpaper service lock failed.".to_string())?;
        Ok(storage::public_state(&mut runtime.state))
    }
    fn list(
        &self,
        state: &AppState,
        persistent: &mut RuntimeState,
    ) -> (Vec<MediaItem>, Vec<String>) {
        let revision = self.revision.load(Ordering::SeqCst);
        let mut images = vec![];
        let mut errors = vec![];
        let wifi = !state.settings.wifi_only || wallpaper::wifi_connected();
        storage::reconcile_catalogs(persistent, &state.sources, &self.store.cache);
        for source in state.sources.iter().filter(|s| s.enabled) {
            if self.revision.load(Ordering::SeqCst) != revision {
                break;
            }
            if source.kind != "local" && !wifi {
                errors.push(format!("{}: Waiting for Wi-Fi.", source.name));
                images.extend(storage::offline_items(
                    source,
                    persistent.catalogs.get(&source.id),
                    &self.store.cache,
                ));
                continue;
            }
            match providers::list_source(&self.client, source) {
                Ok(mut items) => {
                    if source.kind != "photos" {
                        persistent.catalogs.insert(
                            source.id.clone(),
                            Catalog {
                                fingerprint: storage::source_fingerprint(source),
                                items: items.clone(),
                            },
                        );
                    }
                    images.append(&mut items)
                }
                Err(e) => {
                    errors.push(format!("{}: {e}", source.name));
                    images.extend(storage::offline_items(
                        source,
                        persistent.catalogs.get(&source.id),
                        &self.store.cache,
                    ));
                }
            }
        }
        let mut ids = HashSet::new();
        images.retain(|i| ids.insert(i.id.clone()));
        (images, errors)
    }
    pub fn list_media(&self) -> Result<Vec<MediaItem>, String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        let state = runtime.state.clone();
        let (images, errors) = self.list(&state, &mut runtime.persistent);
        runtime.state.error = (!errors.is_empty()).then(|| errors.join("\n"));
        runtime.media = images.clone();
        self.store.save_runtime(&runtime.persistent)?;
        Ok(images)
    }
    pub fn rotate(&self, automatic: bool) -> Result<AppState, String> {
        let revision = self.revision.load(Ordering::SeqCst);
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        if automatic {
            let now = chrono::Local::now();
            if !runtime.state.running
                || Instant::now() < runtime.next_due
                || !rotation::schedule_allows(
                    &runtime.state.settings,
                    now.weekday().num_days_from_sunday(),
                    now.hour() * 60 + now.minute(),
                )
            {
                return Ok(storage::public_state(&mut runtime.state));
            }
        }
        let state = runtime.state.clone();
        let (mut items, mut errors) = self.list(&state, &mut runtime.persistent);
        if self.revision.load(Ordering::SeqCst) != revision {
            return Ok(storage::public_state(&mut runtime.state));
        }
        runtime.media = items.clone();
        let previous = runtime
            .state
            .current
            .as_ref()
            .and_then(|c| items.iter().position(|i| i.id == c.id));
        let shuffle = runtime.state.settings.order == "shuffle";
        let ids: Vec<String> = items.iter().map(|m| m.id.clone()).collect();
        if !shuffle {
            if let Some(index) = rotation::choose_index(items.len(), previous, false) {
                items.rotate_left(index);
            }
        }
        // Try another image if the first item is corrupt. Never replace current on failure.
        let mut applied = false;
        for attempt in 0..items.len().min(20) {
            let media = if shuffle {
                let previous = runtime.state.current.as_ref().map(|m| m.id.clone());
                let Some(id) = runtime.persistent.shuffle.next(&ids, previous.as_deref()) else {
                    break;
                };
                let Some(media) = items.iter().find(|m| m.id == id) else {
                    continue;
                };
                media
            } else {
                &items[attempt]
            };
            if self.revision.load(Ordering::SeqCst) != revision {
                return Ok(storage::public_state(&mut runtime.state));
            }
            if items.len() > 1
                && runtime
                    .state
                    .current
                    .as_ref()
                    .is_some_and(|c| c.id == media.id)
            {
                continue;
            }
            let source = runtime
                .state
                .sources
                .iter()
                .find(|s| s.id == media.source_id)
                .unwrap();
            match self
                .prepare(media, source, &runtime.state.settings)
                .and_then(|bytes| {
                    if self.revision.load(Ordering::SeqCst) != revision {
                        return Err("Wallpaper change cancelled because settings changed.".into());
                    }
                    wallpaper::apply(
                        &bytes,
                        &runtime.state.settings,
                        &self.store.root.join("wallpaper"),
                    )
                }) {
                Ok(path) => {
                    runtime.wallpaper_path = Some(path);
                    runtime.state.current = Some(media.clone());
                    runtime.state.last_changed = Some(chrono::Utc::now().to_rfc3339());
                    applied = true;
                    break;
                }
                Err(e) => errors.push(format!("{}: {e}", media.name)),
            }
        }
        if items.is_empty() && errors.is_empty() {
            errors.push("No images found. Enable a source and add a supported image.".into());
        }
        runtime.state.error = (!errors.is_empty()).then(|| errors.join("\n"));
        runtime.next_due = Instant::now()
            + Duration::from_secs(if applied {
                runtime.state.settings.interval_seconds
            } else {
                30
            });
        self.trim_cache(
            runtime.state.settings.cache_limit_mb,
            runtime.wallpaper_path.as_deref(),
        )?;
        self.store.save(&runtime.state)?;
        self.store.save_runtime(&runtime.persistent)?;
        Ok(storage::public_state(&mut runtime.state))
    }
    fn prepare(
        &self,
        media: &MediaItem,
        source: &Source,
        settings: &Settings,
    ) -> Result<Vec<u8>, String> {
        if source.kind == "local" {
            let mut bytes = Vec::new();
            fs::File::open(&media.url)
                .map_err(|_| "Local image is no longer accessible.".to_string())?
                .take(providers::MAX_IMAGE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > providers::MAX_IMAGE_BYTES {
                return Err("Local image exceeds 32 MB.".into());
            }
            return Ok(bytes);
        }
        let path = self.store.cache.join(format!("{}.img", media.id));
        if path.exists() {
            return fs::read(path).map_err(|e| e.to_string());
        }
        let bytes = providers::download(&self.client, media, source)?;
        // Decode before caching: HTML/error pages and broken images never become usable cache entries.
        wallpaper::render(&bytes, settings, (1, 1))?;
        storage::atomic_write(&path, &bytes)?;
        self.trim_cache(settings.cache_limit_mb, None)?;
        Ok(bytes)
    }
    fn trim_cache(&self, limit_mb: u64, keep: Option<&std::path::Path>) -> Result<(), String> {
        let mut files: Vec<_> = fs::read_dir(&self.store.cache)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter_map(|e| {
                e.metadata().ok().filter(|m| m.is_file()).map(|m| {
                    (
                        e.path(),
                        m.len(),
                        m.modified().unwrap_or(std::time::UNIX_EPOCH),
                    )
                })
            })
            .collect();
        let mut total: u64 = files.iter().map(|(_, n, _)| n).sum();
        files.sort_by_key(|(_, _, time)| *time);
        for (path, size, _) in files {
            if total <= limit_mb * 1024 * 1024 {
                break;
            }
            if keep == Some(path.as_path()) {
                continue;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }
    pub fn set_running(&self, running: bool) -> Result<AppState, String> {
        self.revision.fetch_add(1, Ordering::SeqCst);
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        let mut next = runtime.state.clone();
        next.running = running;
        next.error = None;
        self.store.save(&next)?;
        runtime.state = next;
        runtime.next_due = Instant::now();
        Ok(storage::public_state(&mut runtime.state))
    }
    pub fn preview(&self, id: &str) -> Result<String, String> {
        let runtime = self.inner.lock().map_err(|e| e.to_string())?;
        let media = runtime
            .media
            .iter()
            .find(|m| m.id == id)
            .or_else(|| runtime.state.current.as_ref().filter(|m| m.id == id))
            .ok_or("Image is no longer in the library. Refresh the library first.")?;
        let source = runtime
            .state
            .sources
            .iter()
            .find(|s| s.id == media.source_id && s.enabled)
            .ok_or("The image source has been removed or disabled.")?;
        let path = self.store.cache.join(format!("{}-preview.png", media.id));
        if !path.exists() {
            if source.kind != "local"
                && runtime.state.settings.wifi_only
                && !wallpaper::wifi_connected()
                && !self.store.cache.join(format!("{}.img", media.id)).is_file()
            {
                return Err("Waiting for Wi-Fi before downloading preview.".into());
            }
            let bytes = self.prepare(media, source, &runtime.state.settings)?;
            let preview = wallpaper::thumbnail(&bytes)?;
            storage::atomic_write(&path, &preview)?;
            self.trim_cache(
                runtime.state.settings.cache_limit_mb,
                runtime.wallpaper_path.as_deref(),
            )?;
        }
        if !path.exists() {
            return Err("The cache is too small to retain this preview.".into());
        }
        Ok(path.to_string_lossy().into_owned())
    }
}

pub fn start_worker(app: tauri::AppHandle, engine: Arc<Engine>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let before = engine.snapshot().ok().map(|s| (s.last_changed, s.error));
        match engine.rotate(true) {
            Ok(state) => {
                if Some((state.last_changed.clone(), state.error.clone())) != before {
                    let _ = app.emit("wallta:state", state);
                }
            }
            Err(error) => {
                if let Ok(mut runtime) = engine.inner.lock() {
                    runtime.state.error = Some(error);
                    runtime.next_due = Instant::now() + Duration::from_secs(30);
                }
            }
        }
    });
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_state(engine: tauri::State<'_, Arc<Engine>>) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.snapshot()).await
}
#[tauri::command]
pub async fn list_media(engine: tauri::State<'_, Arc<Engine>>) -> Result<Vec<MediaItem>, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.list_media()).await
}
#[tauri::command]
pub async fn get_preview(
    id: String,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<String, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.preview(&id)).await
}
#[tauri::command]
pub async fn next_wallpaper(engine: tauri::State<'_, Arc<Engine>>) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.rotate(false)).await
}
#[tauri::command]
pub async fn set_running(
    running: bool,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.set_running(running)).await
}
#[tauri::command]
pub async fn pick_folder() -> Result<Option<String>, String> {
    blocking(|| {
        Ok(rfd::FileDialog::new()
            .set_title("Choose wallpaper folder")
            .pick_folder()
            .map(|p| p.to_string_lossy().into_owned()))
    })
    .await
}
#[tauri::command]
pub async fn save_state(
    mut state: AppState,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    engine.revision.fetch_add(1, Ordering::SeqCst);
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
        state.current = runtime.state.current.clone().filter(|media| {
            let old = runtime.state.sources.iter().find(|s| s.id == media.source_id);
            let new = state.sources.iter().find(|s| s.id == media.source_id && s.enabled);
            matches!((old, new), (Some(old), Some(new)) if storage::source_fingerprint(old) == storage::source_fingerprint(new))
        }); state.last_changed = runtime.state.last_changed.clone(); state.error = None;
        if state.settings.autostart != runtime.state.settings.autostart { wallpaper::set_autostart(state.settings.autostart)?; }
        engine.store.save(&state)?;
        for old in &runtime.state.sources { if !state.sources.iter().any(|s| s.id == old.id) { engine.store.forget(old)?; } }
        storage::reconcile_catalogs(&mut runtime.persistent, &state.sources, &engine.store.cache);
        runtime.media.clear();
        engine.store.save_runtime(&runtime.persistent)?;
        runtime.state = state; runtime.next_due = Instant::now();
        engine.trim_cache(runtime.state.settings.cache_limit_mb, runtime.wallpaper_path.as_deref())?;
        Ok(storage::public_state(&mut runtime.state))
    }).await
}
#[tauri::command]
pub async fn clear_cache(engine: tauri::State<'_, Arc<Engine>>) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || {
        let runtime = engine.inner.lock().map_err(|e| e.to_string())?;
        for entry in fs::read_dir(&engine.store.cache).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if runtime.wallpaper_path.as_deref() != Some(entry.path().as_path())
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })
    .await
}
#[tauri::command]
pub fn get_capabilities() -> Capabilities {
    Capabilities { platform: if cfg!(windows) { "windows" } else { "browser" }, wallpaper: cfg!(windows), screensaver: false, min_interval_seconds: 10, notes: vec!["Rotation runs in the native process after closing the window. Quit from the tray stops rotation.", "Blurred background applies to Fit mode. Desktop clock overlay is not supported.", "Google sources require your own API credentials. Expired tokens must be reconnected."] }
}
#[tauri::command]
pub async fn open_external(url: String) -> Result<(), String> {
    let parsed = providers::network_url(&url)?;
    if parsed.scheme() != "https" {
        return Err("External links must use HTTPS.".into());
    }
    blocking(move || open::that(parsed.as_str()).map_err(|e| e.to_string())).await
}
