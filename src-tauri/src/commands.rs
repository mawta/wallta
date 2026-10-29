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
    rotation: Mutex<()>,
    mutation_gate: crate::update_policy::MutationGate,
}
#[derive(Clone)]
pub struct Runtime {
    pub state: AppState,
    pub next_due: Instant,
    pub wallpaper_path: Option<PathBuf>,
    pub media: Vec<MediaItem>,
    pub persistent: RuntimeState,
}
impl Engine {
    /// Acquire only while holding the native state lock to await all prior saves.
    pub(crate) fn begin_update_install(
        &self,
    ) -> Result<crate::update_policy::InstallBarrier<'_>, String> {
        let _runtime = self.inner.lock().map_err(|e| e.to_string())?;
        let barrier = self.mutation_gate.install()?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        Ok(barrier)
    }
    /// Resolve IDs through enabled local providers only; never treat caller values as paths.
    pub(crate) fn lan_exports(
        &self,
        ids: &[String],
    ) -> Result<Vec<crate::lan::ExportInput>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        if ids.len() > 50 || ids.iter().collect::<HashSet<_>>().len() != ids.len() {
            return Err("Select at most 50 unique local images.".into());
        }
        let runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        let mut resolved = Vec::new();
        for source in runtime
            .state
            .sources
            .iter()
            .filter(|s| s.enabled && s.kind == "local")
        {
            for media in providers::list_source(&self.client, source)? {
                if ids.contains(&media.id) {
                    resolved.push(crate::lan::ExportInput {
                        id: media.id,
                        name: media.name,
                        path: PathBuf::from(media.url),
                    });
                }
            }
        }
        if resolved.len() != ids.len() {
            return Err(
                "Selected images must belong to an enabled local source. Refresh the library."
                    .into(),
            );
        }
        Ok(resolved)
    }
    /// Append to the fresh native state while holding its lock. Do not restart or apply rotation.
    pub(crate) fn accept_lan_source(&self, path: &std::path::Path) -> Result<AppState, String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        if runtime.state.sources.len() >= 100 {
            return Err(
                "At most 100 sources are supported. Remove a source before adding received images."
                    .into(),
            );
        }
        let mut next = runtime.state.clone();
        next.sources.push(Source {
            id: format!(
                "lan-{}",
                path.file_name()
                    .ok_or("Invalid received folder.")?
                    .to_string_lossy()
            ),
            name: format!("Wi-Fi · {}", chrono::Local::now().format("%Y-%m-%d %H:%M")),
            kind: "local".into(),
            location: path.to_string_lossy().into_owned(),
            folders: vec![],
            recursive: false,
            enabled: true,
            username: None,
            password: None,
            api_key: None,
            access_token: None,
            google: None,
        });
        self.store.save(&next)?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        runtime.state = next;
        runtime.media.clear();
        Ok(storage::public_state(&mut runtime.state))
    }
    /// Resolve original content through the current native catalog; callers supply IDs only.
    pub(crate) fn upscale_input(
        &self,
        id: &str,
    ) -> Result<(MediaItem, Source, Vec<u8>, u64), String> {
        let (snapshot, revision) = self.operation_snapshot()?;
        let media = snapshot
            .media
            .iter()
            .find(|media| media.id == id)
            .or_else(|| {
                snapshot
                    .state
                    .current
                    .as_ref()
                    .filter(|media| media.id == id)
            })
            .ok_or("Image is no longer in the library. Refresh the library first.")?;
        let source = snapshot
            .state
            .sources
            .iter()
            .find(|source| source.id == media.source_id && source.enabled)
            .ok_or("The image source has been removed or disabled.")?;
        if source.kind != "local"
            && snapshot.state.settings.wifi_only
            && !wallpaper::wifi_connected()
            && !self.store.cache.join(format!("{}.img", media.id)).is_file()
        {
            return Err("Waiting for Wi-Fi before downloading the original image.".into());
        }
        let bytes = self.prepare(media, source, &snapshot.state.settings, revision)?;
        let runtime = self.inner.lock().map_err(|error| error.to_string())?;
        if !self.source_is_current(&runtime, source, revision) {
            return Err("Image request cancelled because settings changed.".into());
        }
        Ok((media.clone(), source.clone(), bytes, revision))
    }
    /// Publish a completed managed job against fresh state, atomically with the revision check.
    pub(crate) fn accept_upscale_source(
        &self,
        path: &std::path::Path,
        expected_revision: u64,
    ) -> Result<Source, String> {
        let mut runtime = self.inner.lock().map_err(|error| error.to_string())?;
        self.mutation_gate.ensure_open()?;
        let root = self
            .store
            .root
            .canonicalize()
            .map_err(|_| "Managed image storage is unavailable.")?;
        let parent = self
            .store
            .root
            .join("upscaled")
            .canonicalize()
            .map_err(|_| "Managed upscale storage is unavailable.")?;
        let canonical = path
            .canonicalize()
            .map_err(|_| "The completed upscale folder is unavailable.")?;
        if parent.parent() != Some(root.as_path())
            || canonical.parent() != Some(parent.as_path())
            || !canonical.is_dir()
            || !canonical
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    !name.is_empty()
                        && name.len() <= 128
                        && name
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                })
        {
            return Err("Upscale results must be a direct managed job folder.".into());
        }
        if let Some(existing) = runtime.state.sources.iter().find(|source| {
            source.kind == "local"
                && source.effective_folders().iter().any(|location| {
                    std::path::Path::new(location)
                        .canonicalize()
                        .is_ok_and(|path| path == canonical)
                })
        }) {
            let mut clean = existing.clone();
            clean.password = None;
            clean.api_key = None;
            clean.access_token = None;
            return Ok(clean);
        }
        if self.revision.load(Ordering::SeqCst) != expected_revision {
            return Err("Upscale results cannot be saved because settings changed. Process the selected images again.".into());
        }
        if runtime.state.sources.len() >= 100 {
            return Err("At most 100 sources are supported. Remove a source before saving processed images.".into());
        }
        let source = Source {
            id: format!("upscale-{:032x}", rand::random::<u128>()),
            name: "Ảnh 4K".into(),
            kind: "local".into(),
            location: canonical.to_string_lossy().into_owned(),
            folders: vec![],
            recursive: false,
            enabled: true,
            username: None,
            password: None,
            api_key: None,
            access_token: None,
            google: None,
        };
        let mut next = runtime.state.clone();
        next.sources.push(source.clone());
        self.store.save(&next)?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        runtime.state = next;
        runtime.media.clear();
        Ok(source)
    }
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
        store.save_runtime(&persistent)?;
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
            rotation: Mutex::new(()),
            mutation_gate: crate::update_policy::MutationGate::default(),
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
        revision: u64,
    ) -> (Vec<MediaItem>, Vec<String>) {
        let mut images = vec![];
        let mut errors = vec![];
        let wifi = !state.settings.wifi_only || wallpaper::wifi_connected();
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
        let (snapshot, revision) = self.operation_snapshot()?;
        let mut persistent = snapshot.persistent;
        let (images, errors) = self.list(&snapshot.state, &mut persistent, revision);
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        if self.revision.load(Ordering::SeqCst) != revision {
            return Ok(runtime.media.clone());
        }
        runtime.persistent.catalogs = persistent.catalogs;
        runtime.state.error = (!errors.is_empty()).then(|| errors.join("\n"));
        runtime.media = images.clone();
        self.store.save_runtime(&runtime.persistent)?;
        Ok(images)
    }
    fn operation_snapshot(&self) -> Result<(Runtime, u64), String> {
        let runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        Ok((runtime.clone(), self.revision.load(Ordering::SeqCst)))
    }
    fn source_is_current(&self, runtime: &Runtime, source: &Source, revision: u64) -> bool {
        self.mutation_gate.ensure_open().is_ok()
            && self.revision.load(Ordering::SeqCst) == revision
            && runtime.state.sources.iter().any(|current| {
                current.enabled
                    && current.id == source.id
                    && storage::source_fingerprint(current) == storage::source_fingerprint(source)
            })
    }
    fn persist_cache(&self, runtime: &mut Runtime) -> Result<(), String> {
        self.trim_cache(runtime.state.settings.cache_limit_mb, None)?;
        storage::prune_cache_owners(&mut runtime.persistent, &self.store.cache);
        self.store.save_runtime(&runtime.persistent)
    }
    fn cache_bytes(
        &self,
        media: &MediaItem,
        source: &Source,
        revision: u64,
        suffix: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        if !self.source_is_current(&runtime, source, revision) {
            return Err("Image request cancelled because settings changed.".into());
        }
        storage::own_cache(&mut runtime.persistent, media, source);
        // Write ownership first: every published file remains attributable after a crash.
        self.store.save_runtime(&runtime.persistent)?;
        storage::atomic_write(
            &self.store.cache.join(format!("{}{suffix}", media.id)),
            bytes,
        )?;
        self.persist_cache(&mut runtime)
    }
    pub fn rotate(&self, automatic: bool) -> Result<AppState, String> {
        self.rotate_selection(automatic, None, false)
    }
    pub fn rotate_selection(
        &self,
        automatic: bool,
        source_id: Option<&str>,
        start_running: bool,
    ) -> Result<AppState, String> {
        // Serialize rotations without blocking state, settings, or pause controls.
        let _rotation = self.rotation.lock().map_err(|e| e.to_string())?;
        let (mut snapshot, revision) = self.operation_snapshot()?;
        if automatic {
            let now = chrono::Local::now();
            if !snapshot.state.running
                || Instant::now() < snapshot.next_due
                || !rotation::schedule_allows(
                    &snapshot.state.settings,
                    now.weekday().num_days_from_sunday(),
                    now.hour() * 60 + now.minute(),
                )
            {
                return Ok(storage::public_state(&mut snapshot.state));
            }
        }
        let mut selection = snapshot.state.clone();
        if let Some(id) = source_id {
            selection
                .sources
                .retain(|source| source.id == id && source.enabled && !source.location.is_empty());
            if selection.sources.is_empty() {
                return Err("Chọn và bật một thư mục ảnh trước khi áp dụng.".into());
            }
        }
        let (mut items, mut errors) = self.list(&selection, &mut snapshot.persistent, revision);
        let mut media_list = if let Some(id) = source_id {
            snapshot
                .media
                .iter()
                .filter(|media| media.source_id != id)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        media_list.extend(items.clone());
        let mut selection_shuffle = snapshot.persistent.shuffle.clone();
        let previous = snapshot
            .state
            .current
            .as_ref()
            .and_then(|c| items.iter().position(|i| i.id == c.id));
        let shuffle = snapshot.state.settings.order == "shuffle";
        let ids: Vec<String> = items.iter().map(|m| m.id.clone()).collect();
        if !shuffle {
            if let Some(index) = rotation::choose_index(items.len(), previous, false) {
                items.rotate_left(index);
            }
        }
        let mut applied = false;
        for attempt in 0..items.len().min(20) {
            if self.revision.load(Ordering::SeqCst) != revision {
                return self.snapshot();
            }
            let media = if shuffle {
                let previous = snapshot.state.current.as_ref().map(|m| m.id.clone());
                let Some(id) = selection_shuffle.next(&ids, previous.as_deref()) else {
                    break;
                };
                let Some(media) = items.iter().find(|m| m.id == id) else {
                    continue;
                };
                media
            } else {
                &items[attempt]
            };
            if items.len() > 1
                && snapshot
                    .state
                    .current
                    .as_ref()
                    .is_some_and(|c| c.id == media.id)
            {
                continue;
            }
            let Some(source) = snapshot
                .state
                .sources
                .iter()
                .find(|s| s.id == media.source_id)
            else {
                continue;
            };
            let result = self
                .prepare(media, source, &snapshot.state.settings, revision)
                .and_then(|bytes| {
                    wallpaper::prepare(
                        &bytes,
                        &snapshot.state.settings,
                        &self.store.root.join("wallpaper"),
                    )
                });
            let path = match result {
                Ok(path) => path,
                Err(error) => {
                    errors.push(format!("{}: {error}", media.name));
                    continue;
                }
            };
            // Decode/render has finished. Only the short OS commit is serialized with controls.
            let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
            if !self.source_is_current(&runtime, source, revision) {
                let _ = fs::remove_file(path);
                return Ok(storage::public_state(&mut runtime.state));
            }
            match wallpaper::apply_prepared(&path, &self.store.root.join("wallpaper")) {
                Ok(()) => {
                    runtime.wallpaper_path = Some(path);
                    runtime.state.current = Some(media.clone());
                    runtime.state.last_changed = Some(chrono::Utc::now().to_rfc3339());
                    if start_running {
                        runtime.state.running = true;
                    }
                    applied = true;
                    break;
                }
                Err(error) => {
                    let _ = fs::remove_file(path);
                    errors.push(format!("{}: {error}", media.name));
                }
            }
        }
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        if self.revision.load(Ordering::SeqCst) != revision {
            return Ok(storage::public_state(&mut runtime.state));
        }
        runtime.persistent.catalogs = snapshot.persistent.catalogs;
        if source_id.is_none() {
            runtime.persistent.shuffle = selection_shuffle;
        }
        runtime.media = media_list;
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
        self.store.save(&runtime.state)?;
        self.persist_cache(&mut runtime)?;
        if source_id.is_some() && !applied {
            return Err(runtime
                .state
                .error
                .clone()
                .unwrap_or_else(|| "Không thể áp dụng ảnh từ thư mục đã chọn.".into()));
        }
        Ok(storage::public_state(&mut runtime.state))
    }
    fn prepare(
        &self,
        media: &MediaItem,
        source: &Source,
        settings: &Settings,
        revision: u64,
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
            let mut bytes = Vec::new();
            fs::File::open(path)
                .map_err(|error| error.to_string())?
                .take(providers::MAX_IMAGE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() as u64 > providers::MAX_IMAGE_BYTES {
                return Err("Cached original image exceeds 32 MB.".into());
            }
            return Ok(bytes);
        }
        let bytes = providers::download(&self.client, media, source)?;
        wallpaper::render(&bytes, settings, (1, 1))?;
        self.cache_bytes(media, source, revision, ".img", &bytes)?;
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
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        let mut next = runtime.state.clone();
        next.running = running;
        next.error = None;
        self.store.save(&next)?;
        runtime.state = next;
        runtime.next_due = Instant::now();
        Ok(storage::public_state(&mut runtime.state))
    }
    pub fn preview(&self, id: &str) -> Result<String, String> {
        let (snapshot, revision) = self.operation_snapshot()?;
        let media = snapshot
            .media
            .iter()
            .find(|m| m.id == id)
            .or_else(|| snapshot.state.current.as_ref().filter(|m| m.id == id))
            .ok_or("Image is no longer in the library. Refresh the library first.")?;
        let source = snapshot
            .state
            .sources
            .iter()
            .find(|s| s.id == media.source_id && s.enabled)
            .ok_or("The image source has been removed or disabled.")?;
        let path = self.store.cache.join(format!("{}-preview.png", media.id));
        if !path.exists() {
            if source.kind != "local"
                && snapshot.state.settings.wifi_only
                && !wallpaper::wifi_connected()
                && !self.store.cache.join(format!("{}.img", media.id)).is_file()
            {
                return Err("Waiting for Wi-Fi before downloading preview.".into());
            }
            let bytes = self.prepare(media, source, &snapshot.state.settings, revision)?;
            let preview = wallpaper::thumbnail(&bytes)?;
            self.cache_bytes(media, source, revision, "-preview.png", &preview)?;
        }
        let runtime = self.inner.lock().map_err(|e| e.to_string())?;
        if !self.source_is_current(&runtime, source, revision) {
            return Err("Image request cancelled because settings changed.".into());
        }
        if !path.exists() {
            return Err("The cache is too small to retain this preview.".into());
        }
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn save_state(&self, mut state: AppState) -> Result<AppState, String> {
        if state.version != 1 {
            return Err("Unsupported state version.".into());
        }
        state.settings.validate()?;
        if state.sources.len() > 100 {
            return Err("At most 100 sources are supported.".into());
        }
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        let mut ids = HashSet::new();
        for source in &mut state.sources {
            if source.id.is_empty()
                || source.id.len() > 128
                || !source
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                || !ids.insert(source.id.clone())
            {
                return Err("Source IDs must be unique and contain letters, digits, hyphens or underscores.".into());
            }
            if !["local", "url", "manifest", "webdav", "drive", "photos"]
                .contains(&source.kind.as_str())
            {
                return Err("Unknown source type.".into());
            }
            source.validate_folders()?;
            crate::google::validate_source_location(source)?;
            if ["url", "manifest", "webdav"].contains(&source.kind.as_str()) {
                providers::network_url(&source.location)?;
            }
            storage::merge_credentials(
                source,
                runtime.state.sources.iter().find(|s| s.id == source.id),
            );
        }
        // Native runtime owns wallpaper status. A stale UI cannot overwrite a successful background change.
        state.current = runtime.state.current.clone().filter(|media| {
            let old = runtime.state.sources.iter().find(|s| s.id == media.source_id);
            let new = state.sources.iter().find(|s| s.id == media.source_id && s.enabled);
            matches!((old, new), (Some(old), Some(new)) if storage::source_fingerprint(old) == storage::source_fingerprint(new))
        });
        state.last_changed = runtime.state.last_changed.clone();
        state.error = None;
        if state.settings.autostart != runtime.state.settings.autostart {
            wallpaper::set_autostart(state.settings.autostart)?;
        }
        self.store.save(&state)?;
        for source in &state.sources {
            crate::google::commit_connection(&source.id)?;
        }
        for old in &runtime.state.sources {
            if !state.sources.iter().any(|s| s.id == old.id) {
                self.store.forget(old)?;
            }
        }
        storage::reconcile_catalogs(&mut runtime.persistent, &state.sources, &self.store.cache);
        runtime.media.clear();
        self.store.save_runtime(&runtime.persistent)?;
        let previous_sources = runtime.state.sources.clone();
        runtime.state = state;
        runtime.next_due = Instant::now();
        self.persist_cache(&mut runtime)?;
        crate::google::cleanup_obsolete_imports(
            &self.store.root,
            &previous_sources,
            &runtime.state.sources,
        );
        Ok(storage::public_state(&mut runtime.state))
    }
}

#[cfg(test)]
mod update_install_tests {
    use super::*;

    #[test]
    fn canceled_or_rejected_photos_edit_does_not_clean_imports() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("google-import").join("old");
        let new = root.path().join("google-import").join("new");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        let engine =
            Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap())
                .unwrap();
        let mut state = AppState::default();
        state.sources.push(
            serde_json::from_value(
                serde_json::json!({"id":"photos", "name":"Photos", "kind":"local", "location":old}),
            )
            .unwrap(),
        );
        engine.save_state(state).unwrap();
        let mut canceled = engine.snapshot().unwrap();
        canceled.sources[0].location = new.to_string_lossy().into_owned();
        drop(canceled);
        assert!(old.is_dir());
        let mut rejected = engine.snapshot().unwrap();
        rejected.sources[0].location = new.to_string_lossy().into_owned();
        rejected.sources[0].name.clear();
        assert!(engine.save_state(rejected).is_err());
        assert!(old.is_dir());
        assert!(new.is_dir());
        let mut accepted = engine.snapshot().unwrap();
        accepted.sources[0].location = new.to_string_lossy().into_owned();
        engine.save_state(accepted).unwrap();
        assert!(!old.exists());
        assert!(new.is_dir());
    }

    #[test]
    fn disabled_connected_drive_can_be_saved_before_folder_selection_and_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let make_engine = || {
            Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap()).unwrap()
        };
        let engine = make_engine();
        let mut state = AppState::default();
        state.sources.push(
            serde_json::from_value(serde_json::json!({
                "id":"drive", "name":"Drive", "kind":"drive", "location":"", "enabled":false,
                "google":{"provider":"drive", "connected":true}
            }))
            .unwrap(),
        );
        assert!(engine.save_state(state).is_ok());
        let reopened = make_engine();
        assert!(reopened.save_state(reopened.snapshot().unwrap()).is_ok());
        let mut enabled = reopened.snapshot().unwrap();
        enabled.sources[0].enabled = true;
        assert!(reopened.save_state(enabled).is_err());
        let mut disconnected = reopened.snapshot().unwrap();
        disconnected.sources[0].google.as_mut().unwrap().connected = false;
        assert!(reopened.save_state(disconnected).is_err());
    }

    #[test]
    fn installing_preserves_saved_state_and_rejects_mutations_until_failure() {
        let root = tempfile::tempdir().unwrap();
        let engine =
            Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap())
                .unwrap();
        engine.set_running(false).unwrap();
        let before = fs::read(root.path().join("state.json")).unwrap();
        let state = engine.snapshot().unwrap();
        let barrier = engine.begin_update_install().unwrap();
        assert!(engine.set_running(true).is_err());
        assert!(engine.save_state(state).is_err());
        assert!(engine.rotate(false).is_err());
        assert_eq!(fs::read(root.path().join("state.json")).unwrap(), before);
        drop(barrier);
        assert!(engine.set_running(true).is_ok());
    }
}

pub fn start_worker(app: tauri::AppHandle, engine: Arc<Engine>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if engine.mutation_gate.ensure_open().is_err() {
            continue;
        }
        let before = engine.snapshot().ok().map(|s| (s.last_changed, s.error));
        match engine.rotate(true) {
            Ok(state) => {
                if Some((state.last_changed.clone(), state.error.clone())) != before {
                    let _ = app.emit("wallta:state", state);
                }
            }
            Err(error) => {
                if let Ok(mut runtime) = engine.inner.lock() {
                    if engine.mutation_gate.ensure_open().is_err() {
                        continue;
                    }
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
pub async fn next_wallpaper(
    source_id: Option<String>,
    start_running: Option<bool>,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    blocking(move || {
        engine.rotate_selection(false, source_id.as_deref(), start_running.unwrap_or(false))
    })
    .await
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
    state: AppState,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.save_state(state)).await
}

#[tauri::command]
pub async fn clear_cache(engine: tauri::State<'_, Arc<Engine>>) -> Result<(), String> {
    let engine = engine.inner().clone();
    blocking(move || {
        let mut runtime = engine.inner.lock().map_err(|e| e.to_string())?;
        engine.mutation_gate.ensure_open()?;
        engine.revision.fetch_add(1, Ordering::SeqCst);
        for entry in fs::read_dir(&engine.store.cache).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if runtime.wallpaper_path.as_deref() != Some(entry.path().as_path())
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        runtime.persistent.cache_owners.clear();
        engine.store.save_runtime(&runtime.persistent)?;
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

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };

    // The server signals only after the request reaches the network, then waits
    // for the test to finish checking control responsiveness before replying.
    fn delayed_server(
        body: Vec<u8>,
    ) -> (
        String,
        mpsc::Receiver<()>,
        mpsc::Sender<()>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/image.png", listener.local_addr().unwrap());
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request).unwrap();
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        });
        (url, started_rx, release_tx, server)
    }

    fn engine_for(url: String, kind: &str) -> (tempfile::TempDir, Arc<Engine>) {
        let dir = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::new(Store::new(dir.path().join("state"), dir.path().join("cache")).unwrap())
                .unwrap(),
        );
        let source: Source = serde_json::from_value(serde_json::json!({"id":"remote", "name":"Remote", "kind":kind, "location":url, "enabled":true})).unwrap();
        engine.inner.lock().unwrap().state.sources = vec![source];
        (dir, engine)
    }

    #[test]
    fn state_controls_remain_responsive_during_remote_listing() {
        let (url, started, release, server) = delayed_server(br#"["late.png"]"#.to_vec());
        let (_dir, engine) = engine_for(url, "manifest");
        let worker_engine = engine.clone();
        let listing = thread::spawn(move || worker_engine.list_media());
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let controls = engine.clone();
        let (tx, rx) = mpsc::channel();
        let control = thread::spawn(move || {
            tx.send(
                controls
                    .snapshot()
                    .and_then(|_| controls.set_running(false))
                    .and_then(|mut state| {
                        state.sources.clear();
                        controls.save_state(state)
                    }),
            )
            .unwrap();
        });
        let result = rx.recv_timeout(Duration::from_millis(500));
        release.send(()).unwrap();
        server.join().unwrap();
        let images = listing.join().unwrap().unwrap();
        control.join().unwrap();
        assert!(
            result.is_ok(),
            "State/pause/save was blocked by network I/O"
        );
        result.unwrap().unwrap();
        assert!(images.is_empty(), "Removed source listing was published");
        assert!(engine.inner.lock().unwrap().persistent.catalogs.is_empty());
    }

    fn assert_cancelled_preview(disconnect: bool) {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let (url, started, release, server) = delayed_server(png.into_inner());
        let (_dir, engine) = engine_for(url, "url");
        let media = {
            let mut runtime = engine.inner.lock().unwrap();
            let source = &runtime.state.sources[0];
            let media = providers::item(source.location.clone(), None, &source.id);
            runtime.media = vec![media.clone()];
            media
        };
        let worker_engine = engine.clone();
        let preview = thread::spawn(move || worker_engine.preview(&media.id));
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let controls = engine.clone();
        let (tx, rx) = mpsc::channel();
        let control = thread::spawn(move || {
            let outcome = if disconnect {
                let mut state = controls.snapshot().unwrap();
                state.sources.clear();
                controls.save_state(state)
            } else {
                controls.set_running(false)
            };
            tx.send(outcome).unwrap();
        });
        let result = rx.recv_timeout(Duration::from_millis(500));
        release.send(()).unwrap();
        server.join().unwrap();
        let outcome = preview.join().unwrap();
        control.join().unwrap();
        assert!(result.is_ok(), "Control was blocked by image download");
        result.unwrap().unwrap();
        assert!(outcome.is_err(), "Cancelled download published a preview");
        assert_eq!(fs::read_dir(&engine.store.cache).unwrap().count(), 0);
    }
    #[test]
    fn cancelled_preview_never_publishes_downloads_after_pause() {
        assert_cancelled_preview(false);
    }
    #[test]
    fn removed_source_cannot_publish_a_late_download() {
        assert_cancelled_preview(true);
    }
    #[test]
    fn rotation_does_not_block_pause_while_download_is_pending() {
        let (url, started, release, server) = delayed_server(b"corrupt image".to_vec());
        let (_dir, engine) = engine_for(url, "url");
        let worker = engine.clone();
        let rotation = thread::spawn(move || worker.rotate(false));
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let control_engine = engine.clone();
        let (tx, rx) = mpsc::channel();
        let control = thread::spawn(move || tx.send(control_engine.set_running(false)).unwrap());
        let result = rx.recv_timeout(Duration::from_millis(500));
        release.send(()).unwrap();
        server.join().unwrap();
        rotation.join().unwrap().unwrap();
        control.join().unwrap();
        assert!(result.is_ok(), "Pause was blocked by rotation download");
        result.unwrap().unwrap();
        let runtime = engine.inner.lock().unwrap();
        assert!(!runtime.state.running);
        assert!(
            runtime.state.error.is_none(),
            "Cancelled operation overwrote paused state"
        );
        assert!(runtime.persistent.catalogs.is_empty());
    }
}

#[cfg(test)]
#[path = "lan_engine_tests.rs"]
mod lan_engine_tests;

#[cfg(test)]
mod upscale_engine_tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Engine, MediaItem) {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("originals");
        fs::create_dir(&library).unwrap();
        let image = library.join("photo.png");
        fs::write(&image, b"original-image-bytes").unwrap();
        let engine =
            Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap())
                .unwrap();
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"original", "name":"Original", "kind":"local", "location":library
        }))
        .unwrap();
        let media = providers::item(image.to_string_lossy().into(), None, &source.id);
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.state.sources.push(source);
            runtime.media.push(media.clone());
        }
        (root, engine, media)
    }

    fn result_folder(root: &std::path::Path) -> PathBuf {
        let result = root.join("upscaled").join("test-job");
        fs::create_dir_all(&result).unwrap();
        fs::write(result.join("001.jpg"), b"processed-image-bytes").unwrap();
        result
    }

    #[test]
    fn upscale_resolves_enabled_media_ids_and_reads_original_bytes() {
        let (_root, engine, media) = fixture();
        let (resolved, source, bytes, revision) = engine.upscale_input(&media.id).unwrap();
        assert_eq!(resolved.id, media.id);
        assert_eq!(source.id, "original");
        assert_eq!(bytes, b"original-image-bytes");
        assert_eq!(revision, engine.revision.load(Ordering::SeqCst));
        assert!(engine.upscale_input(&media.url).is_err());
        assert!(engine.upscale_input("unknown").is_err());
        engine.inner.lock().unwrap().state.sources[0].enabled = false;
        assert!(engine.upscale_input(&media.id).is_err());
    }

    #[test]
    fn upscale_can_resolve_current_image_but_respects_installer_gate() {
        let (_root, engine, media) = fixture();
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.media.clear();
            runtime.state.current = Some(media.clone());
        }
        assert!(engine.upscale_input(&media.id).is_ok());
        let barrier = engine.begin_update_install().unwrap();
        assert!(engine.upscale_input(&media.id).is_err());
        drop(barrier);
    }

    #[test]
    fn upscale_reads_cached_original_with_the_same_32_mb_limit() {
        let (_root, engine, media) = fixture();
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.state.sources[0].kind = "manifest".into();
            runtime.state.sources[0].location = "https://images.test/list.json".into();
        }
        let cached = engine.store.cache.join(format!("{}.img", media.id));
        fs::write(&cached, b"cached-original").unwrap();
        assert_eq!(
            engine.upscale_input(&media.id).unwrap().2,
            b"cached-original"
        );
        fs::File::create(&cached)
            .unwrap()
            .set_len(providers::MAX_IMAGE_BYTES + 1)
            .unwrap();
        assert!(engine
            .upscale_input(&media.id)
            .unwrap_err()
            .contains("32 MB"));
    }

    #[test]
    fn upscale_save_preserves_fresh_state_and_is_idempotent_after_revision_changes() {
        let (root, engine, _media) = fixture();
        let path = result_folder(root.path());
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.state.settings.interval_seconds = 4321;
            runtime.state.running = false;
        }
        let revision = engine.revision.load(Ordering::SeqCst);
        let saved = engine.accept_upscale_source(&path, revision).unwrap();
        assert_eq!(saved.name, "Ảnh 4K");
        assert_eq!(saved.kind, "local");
        assert!(saved.enabled);
        assert!(saved.folders.is_empty());
        assert_eq!(PathBuf::from(&saved.location), path.canonicalize().unwrap());
        assert_eq!(engine.snapshot().unwrap().settings.interval_seconds, 4321);
        assert!(!engine.snapshot().unwrap().running);
        assert!(engine.inner.lock().unwrap().media.is_empty());
        assert_eq!(engine.store.load().unwrap().sources.len(), 2);
        engine.set_running(true).unwrap();
        let again = engine.accept_upscale_source(&path, revision).unwrap();
        assert_eq!(saved.id, again.id);
        assert_eq!(engine.snapshot().unwrap().sources.len(), 2);
    }

    #[test]
    fn upscale_save_rejects_stale_revision_and_paths_outside_managed_job() {
        let (root, engine, _media) = fixture();
        let job = result_folder(root.path());
        let revision = engine.revision.load(Ordering::SeqCst);
        engine.set_running(false).unwrap();
        let before = engine.snapshot().unwrap().sources.len();
        assert!(engine
            .accept_upscale_source(&job, revision)
            .unwrap_err()
            .contains("changed"));
        let current = engine.revision.load(Ordering::SeqCst);
        for invalid in [
            root.path().to_path_buf(),
            root.path().join("upscaled"),
            root.path().join("originals"),
            job.join("001.jpg"),
        ] {
            assert!(engine.accept_upscale_source(&invalid, current).is_err());
        }
        assert_eq!(engine.snapshot().unwrap().sources.len(), before);
        let barrier = engine.begin_update_install().unwrap();
        assert!(engine.accept_upscale_source(&job, current).is_err());
        drop(barrier);
    }

    #[test]
    fn upscale_save_failed_persistence_does_not_publish_source() {
        let (root, engine, _media) = fixture();
        let job = result_folder(root.path());
        fs::create_dir(root.path().join("state.json")).unwrap();
        let revision = engine.revision.load(Ordering::SeqCst);
        assert!(engine.accept_upscale_source(&job, revision).is_err());
        assert_eq!(engine.snapshot().unwrap().sources.len(), 1);
        assert_eq!(engine.revision.load(Ordering::SeqCst), revision);
    }

    #[test]
    fn upscale_save_enforces_source_limit_but_existing_result_remains_idempotent() {
        let (root, engine, _media) = fixture();
        let job = result_folder(root.path());
        let saved = engine.accept_upscale_source(&job, 0).unwrap();
        {
            let mut runtime = engine.inner.lock().unwrap();
            let template = runtime.state.sources[0].clone();
            for index in 2..100 {
                let mut extra = template.clone();
                extra.id = format!("source-{index}");
                runtime.state.sources.push(extra);
            }
        }
        assert_eq!(engine.accept_upscale_source(&job, 0).unwrap().id, saved.id);
        let another = root.path().join("upscaled").join("another-job");
        fs::create_dir(&another).unwrap();
        let revision = engine.revision.load(Ordering::SeqCst);
        assert!(engine
            .accept_upscale_source(&another, revision)
            .unwrap_err()
            .contains("100"));
    }

    #[test]
    fn save_state_normalizes_authoritative_folder_array_before_location_validation() {
        let (_root, engine, _media) = fixture();
        let mut state = engine.snapshot().unwrap();
        state.sources[0].location.clear();
        state.sources[0].folders = vec![SourceFolder {
            location: "selected".into(),
            name: None,
        }];
        let saved = engine.save_state(state).unwrap();
        assert_eq!(saved.sources[0].location, "selected");
    }
}
