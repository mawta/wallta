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
    lock_rotation: Mutex<()>,
    mutation_gate: crate::update_policy::MutationGate,
}
#[derive(Clone)]
pub struct Runtime {
    pub state: AppState,
    pub next_due: Instant,
    pub lock_next_due: Instant,
    pub wallpaper_path: Option<PathBuf>,
    pub motion_path: Option<PathBuf>,
    pub media: Vec<MediaItem>,
    pub persistent: RuntimeState,
    cache_epoch: u64,
}
impl Engine {
    /// Acquire only while holding the native state lock to await all prior saves.
    pub(crate) fn begin_update_install(
        &self,
    ) -> Result<crate::update_policy::InstallBarrier<'_>, String> {
        let _runtime = self.inner.lock().map_err(|e| e.to_string())?;
        let barrier = self.mutation_gate.install()?;
        crate::animated_wallpaper::stop();
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
        let bytes = self.upscale_original(media, source, &snapshot.state.settings, revision)?;
        if crate::motion_media::detect_format(&bytes).is_ok_and(|format| format.is_video()) {
            return Err(
                "4K processing supports still images and GIF frames. Videos cannot be upscaled."
                    .into(),
            );
        }
        let runtime = self.inner.lock().map_err(|error| error.to_string())?;
        if !self.source_is_current(&runtime, source, revision) {
            return Err("Image request cancelled because settings changed.".into());
        }
        Ok((media.clone(), source.clone(), bytes, revision))
    }
    /// Enumerate originals in the worker, independently of the UI's current page.
    pub(crate) fn upscale_all_items(
        &self,
        source_id: Option<&str>,
        expected: u64,
    ) -> Result<Vec<MediaItem>, String> {
        let (mut snapshot, revision) = self.operation_snapshot()?;
        if revision != expected {
            return Err("Sources changed during processing. Start a new job.".into());
        }
        let managed = self.store.root.join("upscaled");
        let managed = crate::upscale::canonical_with_missing_tail(&managed)?;
        snapshot.state.sources.retain(|source| {
            source.enabled
                && source_id.is_none_or(|id| source.id == id)
                && !(source.kind == "local"
                    && source.effective_folders().iter().all(|location| {
                        let path = PathBuf::from(location);
                        crate::upscale::canonical_with_missing_tail(&path)
                            .unwrap_or(path)
                            .starts_with(&managed)
                    }))
        });
        if source_id.is_some() && snapshot.state.sources.is_empty() {
            return Err("Select an enabled original source. Managed upscale collections cannot process themselves.".into());
        }
        let (mut items, errors) = self.list_excluding(
            &snapshot.state,
            &mut snapshot.persistent,
            revision,
            std::slice::from_ref(&managed),
        );
        items.retain(|media| {
            !is_video_item(media)
                && !PathBuf::from(&media.url)
                    .canonicalize()
                    .is_ok_and(|path| path.starts_with(&managed))
        });
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        if self.revision.load(Ordering::SeqCst) != expected {
            return Err("Sources changed during processing. Start a new job.".into());
        }
        Ok(items)
    }
    /// Capture the original source label against the same revision as the worker's items.
    pub(crate) fn upscale_scope_name(
        &self,
        ids: &[String],
        selected: Option<&[MediaItem]>,
        expected: u64,
    ) -> Result<String, String> {
        let (snapshot, revision) = self.operation_snapshot()?;
        if revision != expected {
            return Err("Sources changed during processing. Start a new job.".into());
        }
        let source_ids: HashSet<&str> = match selected {
            Some(items) => items.iter().map(|media| media.source_id.as_str()).collect(),
            None => snapshot
                .media
                .iter()
                .chain(snapshot.state.current.iter())
                .filter(|media| ids.contains(&media.id))
                .map(|media| media.source_id.as_str())
                .collect(),
        };
        if source_ids.len() == 1 {
            if let Some(source) = snapshot
                .state
                .sources
                .iter()
                .find(|source| source.enabled && source_ids.contains(source.id.as_str()))
            {
                return Ok(source.name.clone());
            }
        }
        Ok("Tất cả nguồn".into())
    }
    /// Read a snapshot item after revalidating its source, without depending on a refreshed UI catalog.
    pub(crate) fn upscale_snapshot_input(
        &self,
        media: &MediaItem,
        expected: u64,
    ) -> Result<Vec<u8>, String> {
        let (snapshot, revision) = self.operation_snapshot()?;
        if revision != expected {
            return Err("Sources changed during processing. Start a new job.".into());
        }
        let source = snapshot
            .state
            .sources
            .iter()
            .find(|source| source.enabled && source.id == media.source_id)
            .ok_or("The image source has been removed or disabled.")?;
        if source.kind != "local"
            && snapshot.state.settings.wifi_only
            && !wallpaper::wifi_connected()
            && !self.store.cache.join(format!("{}.img", media.id)).is_file()
        {
            return Err("Waiting for Wi-Fi before downloading the original image.".into());
        }
        let bytes = self.upscale_original(media, source, &snapshot.state.settings, revision)?;
        if crate::motion_media::detect_format(&bytes).is_ok_and(|format| format.is_video()) {
            return Err(
                "4K processing supports still images and GIF frames. Videos cannot be upscaled."
                    .into(),
            );
        }
        let runtime = self.inner.lock().map_err(|e| e.to_string())?;
        if !self.source_is_current(&runtime, source, revision) {
            return Err("Sources changed during processing. Start a new job.".into());
        }
        Ok(bytes)
    }
    /// Publish a completed managed job against fresh state, atomically with the revision check.
    pub(crate) fn accept_upscale_source(
        &self,
        path: &std::path::Path,
        expected_revision: u64,
    ) -> Result<Source, String> {
        self.publish_upscale_source(path, expected_revision, false, "Ảnh 4K")
            .map(|(source, _)| source)
    }
    pub(crate) fn accept_upscale_source_with_revision(
        &self,
        path: &std::path::Path,
        expected_revision: u64,
    ) -> Result<(Source, u64), String> {
        self.publish_upscale_source(path, expected_revision, true, "Ảnh 4K")
    }
    pub(crate) fn accept_upscale_source_named(
        &self,
        path: &std::path::Path,
        expected_revision: u64,
        name: &str,
    ) -> Result<(Source, u64), String> {
        self.publish_upscale_source(path, expected_revision, true, name)
    }
    fn publish_upscale_source(
        &self,
        path: &std::path::Path,
        expected_revision: u64,
        strict: bool,
        name: &str,
    ) -> Result<(Source, u64), String> {
        let mut runtime = self.inner.lock().map_err(|error| error.to_string())?;
        self.mutation_gate.ensure_open()?;
        if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
            return Err("Invalid managed collection name.".into());
        }
        if strict && self.revision.load(Ordering::SeqCst) != expected_revision {
            return Err("Sources changed during processing. Start a new job.".into());
        }
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
            return Ok((clean, self.revision.load(Ordering::SeqCst)));
        }
        if self.revision.load(Ordering::SeqCst) != expected_revision {
            return Err("Upscale results cannot be saved because settings changed. Process the selected images again.".into());
        }
        if runtime.state.sources.len() >= 100 {
            return Err("At most 100 sources are supported. Remove a source before saving processed images.".into());
        }
        let source = Source {
            id: format!("upscale-{:032x}", rand::random::<u128>()),
            name: name.into(),
            kind: "local".into(),
            location: canonical.to_string_lossy().into_owned(),
            folders: vec![],
            recursive: false,
            enabled: false,
            username: None,
            password: None,
            api_key: None,
            access_token: None,
            google: None,
        };
        let mut next = runtime.state.clone();
        next.sources.push(source.clone());
        self.store.save(&next)?;
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        runtime.state = next;
        Ok((source, revision))
    }
    /// Detach a managed folder against fresh state before its files are deleted.
    pub(crate) fn detach_upscale_collection(&self, path: &std::path::Path) -> Result<(), String> {
        self.mutate_upscale_collection(path, false)
    }
    pub(crate) fn delete_upscale_collection(&self, path: &std::path::Path) -> Result<(), String> {
        self.mutate_upscale_collection(path, true)
    }
    fn mutate_upscale_collection(
        &self,
        path: &std::path::Path,
        delete: bool,
    ) -> Result<(), String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        let references = |location: &str| {
            PathBuf::from(location)
                .canonicalize()
                .is_ok_and(|folder| folder.starts_with(path))
        };
        let mut next = runtime.state.clone();
        let mut removed = HashSet::new();
        next.sources.retain_mut(|source| {
            if source.kind != "local" {
                return true;
            }
            let had_folders = !source.folders.is_empty();
            source
                .folders
                .retain(|folder| !references(&folder.location));
            let keep = if had_folders {
                !source.folders.is_empty()
            } else {
                !references(&source.location)
            };
            if had_folders && keep {
                source.location = source.folders[0].location.clone();
            }
            if !keep {
                removed.insert(source.id.clone());
            }
            keep
        });
        if next
            .current
            .as_ref()
            .is_some_and(|media| removed.contains(&media.source_id) || references(&media.url))
        {
            next.current = None;
        }
        storage::normalize_lock_selection(&mut next);
        if next
            .lock_current
            .as_ref()
            .is_some_and(|media| references(&media.url))
        {
            storage::clear_lock_status(&mut next);
        }
        let mut persistent = runtime.persistent.clone();
        persistent.catalogs.retain(|id, _| !removed.contains(id));
        for catalog in persistent.catalogs.values_mut() {
            catalog.items.retain(|media| !references(&media.url));
        }
        persistent
            .cache_owners
            .retain(|_, owner| !removed.contains(&owner.source_id));
        persistent.shuffle = Default::default();
        storage::reconcile_catalogs(&mut persistent, &next.sources, &self.store.cache);
        self.store.save_runtime(&persistent)?;
        self.store.save(&next)?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        runtime.state = next;
        runtime.persistent = persistent;
        runtime
            .media
            .retain(|media| !removed.contains(&media.source_id) && !references(&media.url));
        if delete {
            let id = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Invalid collection ID.")?;
            let fresh = crate::upscale::checked_collection_path(&self.store.root, id)?;
            if fresh != path {
                return Err("Collection storage changed during deletion.".into());
            }
            // State writers and updater installation cannot adopt or mutate this folder
            // between detaching its references and deleting its owned files.
            fs::remove_dir_all(&fresh).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub(crate) fn clear_image_cache(&self) -> Result<(), String> {
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        runtime.cache_epoch = runtime.cache_epoch.wrapping_add(1);
        let motion_files = active_motion_files(&runtime);
        for entry in fs::read_dir(&self.store.cache).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if runtime.wallpaper_path.as_deref() != Some(entry.path().as_path())
                && !motion_files.contains(&entry.path())
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        runtime.persistent.cache_owners.clear();
        runtime
            .persistent
            .motion_owners
            .retain(|name, _| motion_files.contains(&self.store.cache.join(name)));
        self.store.save_runtime(&runtime.persistent)
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
        storage::normalize_lock_selection(&mut state);
        let lock_next_due = lock_due_after_restart(&state);
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
                lock_next_due,
                wallpaper_path,
                motion_path: None,
                media: Vec::new(),
                persistent,
                cache_epoch: 0,
            }),
            store,
            client: providers::client()?,
            revision: AtomicU64::new(0),
            rotation: Mutex::new(()),
            lock_rotation: Mutex::new(()),
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
        self.list_excluding(state, persistent, revision, &[])
    }
    fn list_excluding(
        &self,
        state: &AppState,
        persistent: &mut RuntimeState,
        revision: u64,
        excluded: &[PathBuf],
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
            match providers::list_source_excluding(&self.client, source, excluded) {
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
        self.trim_cache_excluding(
            runtime.state.settings.cache_limit_mb,
            &active_motion_files(runtime),
        )?;
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
    fn stage_motion(
        &self,
        bytes: &[u8],
        source: &Source,
        revision: u64,
    ) -> Result<PathBuf, String> {
        let mut runtime = self.inner.lock().map_err(|error| error.to_string())?;
        if !self.source_is_current(&runtime, source, revision) {
            return Err("Media request cancelled because settings changed.".into());
        }
        use sha2::{Digest, Sha256};
        let format = crate::motion_media::detect_format(bytes)?;
        let limit = if format.is_video() {
            crate::motion_media::MAX_VIDEO_BYTES
        } else {
            crate::motion_media::MAX_STILL_BYTES
        };
        if bytes.len() as u64 > limit {
            return Err("Media exceeds its size limit.".into());
        }
        // Bind staged content to its source grant, even when two sources share identical bytes.
        let path = self.store.cache.join(format!(
            "motion-{}-{:x}.{}",
            storage::source_fingerprint(source),
            Sha256::digest(bytes),
            format.extension()
        ));
        if !path.is_file() {
            storage::atomic_write(&path, bytes)?;
        }
        let name = path
            .file_name()
            .ok_or("Invalid cached media filename.")?
            .to_string_lossy()
            .into_owned();
        runtime.persistent.motion_owners.insert(
            name,
            storage::CacheOwner {
                source_id: source.id.clone(),
                fingerprint: storage::source_fingerprint(source),
            },
        );
        self.store.save_runtime(&runtime.persistent)?;
        Ok(path)
    }
    pub fn playback_preview(&self, id: &str) -> Result<String, String> {
        let (snapshot, revision) = self.operation_snapshot()?;
        let media = snapshot
            .media
            .iter()
            .chain(snapshot.state.current.iter())
            .chain(snapshot.state.lock_current.iter())
            .chain(
                snapshot
                    .persistent
                    .catalogs
                    .values()
                    .flat_map(|catalog| catalog.items.iter()),
            )
            .find(|media| media.id == id)
            .ok_or("Media is no longer in the library. Refresh the library first.")?;
        let source = snapshot
            .state
            .sources
            .iter()
            .find(|source| source.enabled && source.id == media.source_id)
            .ok_or("The media source has been removed or disabled.")?;
        if source.kind != "local"
            && snapshot.state.settings.wifi_only
            && !wallpaper::wifi_connected()
            && !self.store.cache.join(format!("{}.img", media.id)).is_file()
        {
            return Err("Waiting for Wi-Fi before downloading media.".into());
        }
        let bytes = self.prepare(media, source, &snapshot.state.settings, revision)?;
        if !crate::motion_media::detect_format(&bytes)?.is_motion() {
            return Err("This file is a still image.".into());
        }
        let path = self.stage_motion(&bytes, source, revision)?;
        Ok(path.to_string_lossy().into_owned())
    }
    fn restore_motion(&self) -> Result<(), String> {
        let (snapshot, revision) = self.operation_snapshot()?;
        if !snapshot.state.settings.animated_wallpaper {
            return Ok(());
        }
        let Some(media) = snapshot.state.current.as_ref() else {
            return Ok(());
        };
        let Some(source) = snapshot
            .state
            .sources
            .iter()
            .find(|source| source.enabled && source.id == media.source_id)
        else {
            return Ok(());
        };
        if source.kind != "local"
            && snapshot.state.settings.wifi_only
            && !wallpaper::wifi_connected()
            && !self.store.cache.join(format!("{}.img", media.id)).is_file()
        {
            return Ok(());
        }
        let bytes = self.prepare(media, source, &snapshot.state.settings, revision)?;
        if !crate::motion_media::detect_format(&bytes)?.is_motion() {
            return Ok(());
        }
        let path = self.stage_motion(&bytes, source, revision)?;
        let mut runtime = self.inner.lock().map_err(|error| error.to_string())?;
        if !self.source_is_current(&runtime, source, revision) {
            return Ok(());
        }
        crate::animated_wallpaper::start(&path, &snapshot.state.settings.fit)?;
        runtime.motion_path = Some(path);
        crate::animated_wallpaper::set_paused(false)?;
        self.persist_cache(&mut runtime)
    }
    pub fn rotate(&self, automatic: bool) -> Result<AppState, String> {
        self.rotate_selection(automatic, None, false)
    }
    pub fn rotate_lock(
        &self,
        automatic: bool,
        source_id: Option<&str>,
        start_running: bool,
    ) -> Result<AppState, String> {
        let _rotation = self
            .lock_rotation
            .lock()
            .map_err(|error| error.to_string())?;
        let (mut snapshot, revision) = self.operation_snapshot()?;
        if automatic {
            let now = chrono::Local::now();
            if !snapshot.state.settings.lock_running
                || Instant::now() < snapshot.lock_next_due
                || !rotation::schedule_allows(
                    &snapshot.state.settings,
                    now.weekday().num_days_from_sunday(),
                    now.hour() * 60 + now.minute(),
                )
            {
                return Ok(storage::public_state(&mut snapshot.state));
            }
        }
        let selected_id = source_id
            .unwrap_or(&snapshot.state.settings.lock_source_id)
            .to_owned();
        let source = snapshot
            .state
            .sources
            .iter()
            .find(|source| source.enabled && source.id == selected_id)
            .ok_or("Select an enabled Lock screen source first.")?
            .clone();
        let mut selection = snapshot.state.clone();
        selection
            .sources
            .retain(|candidate| candidate.id == selected_id);
        let (mut items, mut errors) = self.list(&selection, &mut snapshot.persistent, revision);
        let mut shuffle = if selected_id == snapshot.state.settings.lock_source_id {
            snapshot.persistent.lock_shuffle.clone()
        } else {
            Default::default()
        };
        let previous = snapshot
            .state
            .lock_current
            .as_ref()
            .filter(|media| media.source_id == selected_id);
        let ids = items
            .iter()
            .map(|media| media.id.clone())
            .collect::<Vec<_>>();
        let shuffled = snapshot.state.settings.order == "shuffle";
        if !shuffled {
            let previous_index =
                previous.and_then(|media| items.iter().position(|item| item.id == media.id));
            if let Some(index) = rotation::choose_index(items.len(), previous_index, false) {
                items.rotate_left(index);
            }
        }
        let directory = self.store.root.join("lock-wallpaper");
        let mut applied = false;
        for attempt in 0..items.len().min(20) {
            if self.revision.load(Ordering::SeqCst) != revision {
                return self.snapshot();
            }
            let media = if shuffled {
                let Some(id) = shuffle.next(&ids, previous.map(|media| media.id.as_str())) else {
                    break;
                };
                let Some(media) = items.iter().find(|media| media.id == id) else {
                    continue;
                };
                media
            } else {
                &items[attempt]
            };
            if items.len() > 1 && previous.is_some_and(|previous| previous.id == media.id) {
                continue;
            }
            let prepared = self
                .prepare(media, &source, &snapshot.state.settings, revision)
                .and_then(|bytes| crate::motion_media::wallpaper_bytes(&bytes, &self.store.cache))
                .and_then(|poster| {
                    wallpaper::prepare(&poster, &snapshot.state.settings, &directory)
                });
            let path = match prepared {
                Ok(path) => path,
                Err(error) => {
                    errors.push(format!("{}: {error}", media.name));
                    continue;
                }
            };
            let mut runtime = self.inner.lock().map_err(|error| error.to_string())?;
            if !self.source_is_current(&runtime, &source, revision) {
                let _ = fs::remove_file(path);
                return Ok(storage::public_state(&mut runtime.state));
            }
            match wallpaper::apply_lock_prepared(&path, &directory) {
                Ok(()) => {
                    if runtime.state.settings.lock_source_id != selected_id {
                        storage::clear_lock_status(&mut runtime.state);
                    }
                    runtime.state.settings.lock_source_id = selected_id.clone();
                    if start_running {
                        runtime.state.settings.lock_running = true;
                    }
                    runtime.state.lock_current = Some(media.clone());
                    runtime.state.lock_last_changed = Some(chrono::Utc::now().to_rfc3339());
                    applied = true;
                    break;
                }
                Err(error) => {
                    let _ = fs::remove_file(path);
                    errors.push(error);
                }
            }
        }
        let mut runtime = self.inner.lock().map_err(|error| error.to_string())?;
        if self.revision.load(Ordering::SeqCst) != revision {
            return Ok(storage::public_state(&mut runtime.state));
        }
        // Merge only the selected catalog. Home owns its media list and shuffle cursor.
        if let Some(catalog) = snapshot.persistent.catalogs.remove(&selected_id) {
            runtime.persistent.catalogs.insert(selected_id, catalog);
        }
        runtime.persistent.lock_shuffle = shuffle;
        if items.is_empty() && errors.is_empty() {
            errors.push("No supported media found in the Lock screen source.".into());
        }
        runtime.state.lock_error = (!errors.is_empty()).then(|| errors.join("\n"));
        runtime.lock_next_due = Instant::now()
            + Duration::from_secs(if applied {
                runtime.state.settings.lock_interval_seconds
            } else {
                30
            });
        self.store.save(&runtime.state)?;
        self.persist_cache(&mut runtime)?;
        Ok(storage::public_state(&mut runtime.state))
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
        let explicit_source = source_id.is_some();
        let configured_source = snapshot.state.settings.home_source_id.clone();
        let source_id = source_id
            .or_else(|| (!configured_source.is_empty()).then_some(configured_source.as_str()));
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
                    let format = crate::motion_media::detect_format(&bytes)?;
                    let motion_path =
                        if snapshot.state.settings.animated_wallpaper && format.is_motion() {
                            Some(self.stage_motion(&bytes, source, revision)?)
                        } else {
                            None
                        };
                    let bytes = if format.is_video() {
                        crate::motion_media::wallpaper_bytes(&bytes, &self.store.cache)?
                    } else {
                        bytes
                    };
                    wallpaper::prepare(
                        &bytes,
                        &snapshot.state.settings,
                        &self.store.root.join("wallpaper"),
                    )
                    .map(|path| (path, motion_path))
                });
            let (path, motion_path) = match result {
                Ok(prepared) => prepared,
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
                    crate::animated_wallpaper::stop();
                    runtime.motion_path = None;
                    if let Some(path) = motion_path {
                        match crate::animated_wallpaper::start(&path, &snapshot.state.settings.fit)
                        {
                            Ok(()) => {
                                runtime.motion_path = Some(path);
                            }
                            Err(error) => errors.push(error),
                        }
                    }
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
        if !explicit_source {
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
    /// Upscaling owns decode validation. Do not render or cache an unvalidated remote body.
    fn upscale_original(
        &self,
        media: &MediaItem,
        source: &Source,
        settings: &Settings,
        revision: u64,
    ) -> Result<Vec<u8>, String> {
        if source.kind == "local" || self.store.cache.join(format!("{}.img", media.id)).is_file() {
            self.prepare(media, source, settings, revision)
        } else {
            providers::download(&self.client, media, source)
        }
    }
    fn prepare(
        &self,
        media: &MediaItem,
        source: &Source,
        _settings: &Settings,
        revision: u64,
    ) -> Result<Vec<u8>, String> {
        if source.kind == "local" {
            return crate::motion_media::read_bounded(
                fs::File::open(&media.url)
                    .map_err(|_| "Local image is no longer accessible.".to_string())?,
            );
        }
        let path = self.store.cache.join(format!("{}.img", media.id));
        if path.exists() {
            return crate::motion_media::read_bounded(
                fs::File::open(path).map_err(|error| error.to_string())?,
            );
        }
        let bytes = providers::download(&self.client, media, source)?;
        crate::motion_media::poster_bytes(&bytes, &self.store.cache)?;
        self.cache_bytes(media, source, revision, ".img", &bytes)?;
        Ok(bytes)
    }
    fn trim_cache_excluding(&self, limit_mb: u64, keep: &[PathBuf]) -> Result<(), String> {
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
            if keep.contains(&path) {
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
    pub(crate) fn photo_preview(
        &self,
        id: &str,
        source_id: &str,
    ) -> Result<crate::photo_preview::PhotoPreview, String> {
        let (snapshot, revision) = self.operation_snapshot()?;
        let mut known = snapshot.media.clone();
        known.extend(snapshot.state.current.iter().cloned());
        for source in &snapshot.state.sources {
            if let Some(catalog) = snapshot
                .persistent
                .catalogs
                .get(&source.id)
                .filter(|catalog| catalog.fingerprint == storage::source_fingerprint(source))
            {
                known.extend(catalog.items.iter().cloned());
            }
        }
        let prepared = crate::photo_preview::prepare(
            &crate::photo_preview::Context {
                root: &self.store.root,
                client: &self.client,
                sources: &snapshot.state.sources,
                known: &known,
                network_allowed: !snapshot.state.settings.wifi_only || wallpaper::wifi_connected(),
            },
            id,
            source_id,
        )?;
        // Explicit viewing may read disabled sources without changing wallpaper eligibility.
        let mut runtime = self.inner.lock().map_err(|e| e.to_string())?;
        self.mutation_gate.ensure_open()?;
        if self.revision.load(Ordering::SeqCst) != revision
            || runtime.cache_epoch != snapshot.cache_epoch
            || prepared.images().any(|image| {
                !runtime.state.sources.iter().any(|source| {
                    source.id == image.source.id
                        && storage::source_fingerprint(source)
                            == storage::source_fingerprint(&image.source)
                })
            })
        {
            return Err("Photo request cancelled because sources changed.".into());
        }
        for image in prepared.images() {
            storage::own_cache(&mut runtime.persistent, &image.media, &image.source);
        }
        self.store.save_runtime(&runtime.persistent)?;
        let preview = prepared.publish(&self.store.cache)?;
        let mut keep = vec![PathBuf::from(&preview.image.url)];
        keep.extend(preview.before.iter().map(|image| PathBuf::from(&image.url)));
        self.trim_cache_excluding(runtime.state.settings.cache_limit_mb, &keep)?;
        storage::prune_cache_owners(&mut runtime.persistent, &self.store.cache);
        self.store.save_runtime(&runtime.persistent)?;
        Ok(preview)
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
            let preview = crate::motion_media::poster_bytes(&bytes, &self.store.cache)?;
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
        storage::preserve_lock_status(&mut state, &runtime.state);
        let home_changed = {
            let mut previous_settings =
                serde_json::to_value(&runtime.state.settings).map_err(|error| error.to_string())?;
            let mut next_settings =
                serde_json::to_value(&state.settings).map_err(|error| error.to_string())?;
            for key in ["lockSourceId", "lockRunning", "lockIntervalSeconds"] {
                previous_settings.as_object_mut().unwrap().remove(key);
                next_settings.as_object_mut().unwrap().remove(key);
            }
            previous_settings != next_settings
                || state.running != runtime.state.running
                || state
                    .sources
                    .iter()
                    .map(storage::source_fingerprint)
                    .collect::<Vec<_>>()
                    != runtime
                        .state
                        .sources
                        .iter()
                        .map(storage::source_fingerprint)
                        .collect::<Vec<_>>()
        };
        let lock_source_changed = {
            let old = runtime
                .state
                .sources
                .iter()
                .find(|source| source.id == runtime.state.settings.lock_source_id);
            let new = state
                .sources
                .iter()
                .find(|source| source.id == state.settings.lock_source_id);
            old.map(storage::source_fingerprint) != new.map(storage::source_fingerprint)
        };
        let lock_schedule_changed = state.settings.lock_source_id
            != runtime.state.settings.lock_source_id
            || lock_source_changed
            || state.settings.lock_running != runtime.state.settings.lock_running
            || state.settings.lock_interval_seconds != runtime.state.settings.lock_interval_seconds;
        if lock_source_changed
            || state.settings.lock_source_id != runtime.state.settings.lock_source_id
        {
            runtime.persistent.lock_shuffle = Default::default();
        }
        if state.settings.autostart != runtime.state.settings.autostart {
            wallpaper::set_autostart(state.settings.autostart)?;
        }
        self.store.save(&state)?;
        if !state.settings.animated_wallpaper || state.current.is_none() {
            crate::animated_wallpaper::stop();
            runtime.motion_path = None;
        }
        for source in &state.sources {
            crate::google::commit_connection(&source.id)?;
        }
        for old in &runtime.state.sources {
            if !state.sources.iter().any(|s| s.id == old.id) {
                self.store.forget(old)?;
            }
        }
        storage::reconcile_catalogs(&mut runtime.persistent, &state.sources, &self.store.cache);
        if home_changed {
            runtime.media.clear();
        }
        self.store.save_runtime(&runtime.persistent)?;
        let previous_sources = runtime.state.sources.clone();
        let restore_motion = state.settings.animated_wallpaper
            && state.current.is_some()
            && (runtime.motion_path.is_none() || state.settings.fit != runtime.state.settings.fit);
        runtime.state = state;
        if home_changed {
            runtime.next_due = Instant::now();
        }
        if lock_schedule_changed {
            runtime.lock_next_due = Instant::now();
        }
        self.persist_cache(&mut runtime)?;
        crate::google::cleanup_obsolete_imports(
            &self.store.root,
            &previous_sources,
            &runtime.state.sources,
        );
        let public = storage::public_state(&mut runtime.state);
        drop(runtime);
        if restore_motion {
            if let Err(error) = self.restore_motion() {
                let mut runtime = self.inner.lock().map_err(|error| error.to_string())?;
                runtime.state.error = Some(error);
                self.store.save(&runtime.state)?;
                return Ok(storage::public_state(&mut runtime.state));
            }
        }
        Ok(public)
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

fn lock_due_after_restart(state: &AppState) -> Instant {
    let elapsed = state
        .lock_last_changed
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| {
            chrono::Utc::now()
                .signed_duration_since(value)
                .num_seconds()
                .max(0) as u64
        });
    Instant::now()
        + Duration::from_secs(elapsed.map_or(0, |elapsed| {
            state.settings.lock_interval_seconds.saturating_sub(elapsed)
        }))
}

fn active_motion_files(runtime: &Runtime) -> Vec<PathBuf> {
    runtime
        .motion_path
        .iter()
        .flat_map(|path| [path.clone(), path.with_extension("poster.png")])
        .collect()
}

fn is_video_item(media: &MediaItem) -> bool {
    [&media.name, &media.url].iter().any(|value| {
        value
            .split(['?', '#'])
            .next()
            .and_then(|value| value.rsplit('.').next())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("mp4") || extension.eq_ignore_ascii_case("webm")
            })
    })
}

pub fn start_worker(app: tauri::AppHandle, engine: Arc<Engine>) {
    let lock_engine = engine.clone();
    let lock_app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if lock_engine.mutation_gate.ensure_open().is_err() {
            continue;
        }
        let before = lock_engine
            .snapshot()
            .ok()
            .map(|state| (state.lock_last_changed, state.lock_error));
        match lock_engine.rotate_lock(true, None, false) {
            Ok(state) => {
                if Some((state.lock_last_changed.clone(), state.lock_error.clone())) != before {
                    let _ = lock_app.emit("wallta:state", state);
                }
            }
            Err(error) => {
                if let Ok(mut runtime) = lock_engine.inner.lock() {
                    if lock_engine.mutation_gate.ensure_open().is_err() {
                        continue;
                    }
                    runtime.state.lock_error = Some(error);
                    runtime.lock_next_due = Instant::now() + Duration::from_secs(30);
                    let _ = lock_engine.store.save(&runtime.state);
                    if Some((
                        runtime.state.lock_last_changed.clone(),
                        runtime.state.lock_error.clone(),
                    )) != before
                    {
                        let _ = lock_app
                            .emit("wallta:state", storage::public_state(&mut runtime.state));
                    }
                }
            }
        }
    });
    std::thread::spawn(move || {
        if let Err(error) = engine.restore_motion() {
            if let Ok(mut runtime) = engine.inner.lock() {
                runtime.state.error = Some(error);
            }
        }
        loop {
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
            if let Ok(mut runtime) = engine.inner.lock() {
                if runtime.motion_path.is_some() && engine.mutation_gate.ensure_open().is_ok() {
                    if let Err(error) = crate::animated_wallpaper::health() {
                        crate::animated_wallpaper::stop();
                        runtime.motion_path = None;
                        runtime.state.error = Some(error);
                        let _ = engine.store.save(&runtime.state);
                        let _ = app.emit("wallta:state", storage::public_state(&mut runtime.state));
                    }
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
pub async fn get_playback_preview(
    id: String,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<String, String> {
    let engine = engine.inner().clone();
    blocking(move || engine.playback_preview(&id)).await
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
pub async fn next_lock_wallpaper(
    source_id: Option<String>,
    start_running: Option<bool>,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    blocking(move || {
        engine.rotate_lock(false, source_id.as_deref(), start_running.unwrap_or(false))
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
pub async fn clear_cache(
    upscale: tauri::State<'_, Arc<crate::upscale::UpscaleManager>>,
) -> Result<(), String> {
    let manager = upscale.inner().clone();
    blocking(move || manager.clear_cache().map(|_| ())).await
}

#[tauri::command]
pub fn get_capabilities() -> Capabilities {
    Capabilities { platform: if cfg!(windows) { "windows" } else { "browser" }, wallpaper: cfg!(windows), lock_wallpaper: cfg!(windows), screensaver: false, min_interval_seconds: 10, notes: vec!["Rotation runs in the native process after closing the window. Quit from the tray stops rotation.", "Blurred background applies to Fit mode. Desktop clock overlay is not supported.", "Google sources require your own API credentials. Expired tokens must be reconnected."] }
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
    fn clearing_cache_cancels_an_inflight_photo_without_changing_worker_revision() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let (url, started, release, server) = delayed_server(png.into_inner());
        let (_dir, engine) = engine_for(url.clone(), "url");
        let media = providers::item(url, None, "remote");
        engine.inner.lock().unwrap().media.push(media.clone());
        let revision = engine.revision.load(Ordering::SeqCst);
        let worker_engine = engine.clone();
        let preview =
            thread::spawn(move || worker_engine.photo_preview(&media.id, &media.source_id));
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        engine.clear_image_cache().unwrap();
        assert_eq!(engine.revision.load(Ordering::SeqCst), revision);
        release.send(()).unwrap();
        server.join().unwrap();
        assert!(
            preview.join().unwrap().is_err(),
            "An in-flight photo repopulated cleared cache"
        );
        assert_eq!(fs::read_dir(&engine.store.cache).unwrap().count(), 0);
        assert!(engine
            .inner
            .lock()
            .unwrap()
            .persistent
            .cache_owners
            .is_empty());
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

    #[test]
    fn collection_name_uses_original_snapshot_and_remains_idempotent() {
        let (root, engine, media) = fixture();
        let scope = engine
            .upscale_scope_name(
                std::slice::from_ref(&media.id),
                Some(std::slice::from_ref(&media)),
                0,
            )
            .unwrap();
        assert_eq!(scope, "Original");
        let mut second = media.clone();
        second.source_id = "another-source".into();
        assert_eq!(
            engine
                .upscale_scope_name(&[], Some(&[media.clone(), second]), 0)
                .unwrap(),
            "Tất cả nguồn"
        );
        let path = result_folder(root.path());
        let label = "Ảnh 4K · Original · Photo · 2026-10-30 12:34:56";
        let (saved, own_revision) = engine.accept_upscale_source_named(&path, 0, label).unwrap();
        assert_eq!(saved.name, label);
        assert_eq!(engine.accept_upscale_source(&path, 0).unwrap().name, label);
        let again = engine
            .accept_upscale_source_named(&path, own_revision, "Another label")
            .unwrap()
            .0;
        assert_eq!(again.id, saved.id);
        assert_eq!(again.name, label);
        assert!(
            engine.upscale_scope_name(&[media.id], None, 0).is_err(),
            "External revisions cannot be rebased to obtain a label"
        );
    }

    #[test]
    fn all_ignores_missing_managed_sources_instead_of_aborting_originals() {
        let (root, engine, _media) = fixture();
        let managed = root.path().join("upscaled");
        fs::create_dir(&managed).unwrap();
        let mut source = engine.inner.lock().unwrap().state.sources[0].clone();
        source.id = "upscale-missing".into();
        source.location = managed
            .join("0123456789abcdef0123456789abcdef")
            .to_string_lossy()
            .into_owned();
        engine.inner.lock().unwrap().state.sources.push(source);
        let items = engine.upscale_all_items(None, 0).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source_id, "original");
    }

    #[test]
    fn worker_publication_cannot_rebase_an_external_collection_adoption() {
        let (root, engine, _media) = fixture();
        let result = result_folder(root.path());
        let externally_added = engine.accept_upscale_source(&result, 0).unwrap();
        // Legacy save remains idempotent after adoption; an active worker must stop.
        assert_eq!(
            engine.accept_upscale_source(&result, 0).unwrap().id,
            externally_added.id
        );
        assert!(engine
            .accept_upscale_source_with_revision(&result, 0)
            .is_err());
    }

    #[test]
    fn all_excludes_owned_images_even_when_recursive_original_source_contains_managed_root() {
        let (root, engine, _media) = fixture();
        let owned = root
            .path()
            .join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&owned).unwrap();
        fs::write(owned.join("old.jpg"), b"previous result").unwrap();
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.state.sources[0].location = root.path().to_string_lossy().into_owned();
            runtime.state.sources[0].recursive = true;
        }
        let items = engine.upscale_all_items(None, 0).unwrap();
        assert_eq!(items.len(), 1);
        assert!(!PathBuf::from(&items[0].url).starts_with(&owned));
    }

    #[test]
    fn own_publication_returns_exact_revision_while_external_changes_still_invalidate_inputs() {
        let (root, engine, media) = fixture();
        let result = result_folder(root.path());
        let (source, own_revision) = engine
            .accept_upscale_source_with_revision(&result, 0)
            .unwrap();
        assert!(!source.enabled);
        assert_eq!(own_revision, 1);
        assert!(engine.upscale_snapshot_input(&media, own_revision).is_ok());
        engine.set_running(false).unwrap();
        assert!(engine.upscale_snapshot_input(&media, own_revision).is_err());
    }

    #[test]
    fn detach_collection_clears_sources_catalog_current_but_keeps_rendered_wallpaper() {
        let (root, engine, _media) = fixture();
        let result = result_folder(root.path());
        let source = engine.accept_upscale_source(&result, 0).unwrap();
        let media = providers::item(
            result.join("001.jpg").to_string_lossy().into(),
            None,
            &source.id,
        );
        let wallpaper = root.path().join("wallpaper/rendered.bmp");
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.state.current = Some(media.clone());
            runtime.media.push(media.clone());
            runtime.wallpaper_path = Some(wallpaper.clone());
            runtime.persistent.catalogs.insert(
                source.id.clone(),
                Catalog {
                    fingerprint: storage::source_fingerprint(&source),
                    items: vec![media],
                },
            );
        }
        engine
            .detach_upscale_collection(&result.canonicalize().unwrap())
            .unwrap();
        let runtime = engine.inner.lock().unwrap();
        assert!(!runtime
            .state
            .sources
            .iter()
            .any(|candidate| candidate.id == source.id));
        assert!(runtime.state.current.is_none());
        assert!(!runtime.persistent.catalogs.contains_key(&source.id));
        assert!(!runtime.media.iter().any(|item| item.source_id == source.id));
        assert_eq!(runtime.wallpaper_path, Some(wallpaper));
    }

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
    fn enabling_saved_upscale_source_lists_images_after_reopening() {
        let (root, engine, _) = fixture();
        let output = root
            .path()
            .join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("001.jpg"), b"saved-image").unwrap();
        let revision = engine.revision.load(Ordering::SeqCst);
        let saved = engine.accept_upscale_source(&output, revision).unwrap();
        let mut state = engine.snapshot().unwrap();
        for source in &mut state.sources {
            source.enabled = source.id == saved.id;
        }
        engine.save_state(state).unwrap();
        let images = engine.list_media().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].source_id, saved.id);
        assert!(PathBuf::from(&images[0].url).is_file());
        let reopened =
            Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap())
                .unwrap();
        let images = reopened.list_media().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].source_id, saved.id);
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
        assert!(!saved.enabled);
        assert!(saved.folders.is_empty());
        assert_eq!(PathBuf::from(&saved.location), path.canonicalize().unwrap());
        assert_eq!(engine.snapshot().unwrap().settings.interval_seconds, 4321);
        assert!(!engine.snapshot().unwrap().running);
        assert_eq!(engine.inner.lock().unwrap().media.len(), 1);
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

#[tauri::command]
pub async fn get_photo_preview(
    id: String,
    source_id: String,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<crate::photo_preview::PhotoPreview, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.photo_preview(&id, &source_id))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod photo_preview_tests {
    use super::*;
    #[test]
    fn saved_pair_reopens_disabled_and_survives_cache_clear_and_deletes_with_collection() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::new(temp.path().join("data"), temp.path().join("cache/media")).unwrap();
        let original_dir = temp.path().join("originals");
        fs::create_dir(&original_dir).unwrap();
        let original_path = original_dir.join("original.png");
        image::DynamicImage::new_rgb8(20, 10)
            .save(&original_path)
            .unwrap();
        let engine = Arc::new(Engine::new(store.clone()).unwrap());
        let mut state = engine.snapshot().unwrap();
        state.sources.push(serde_json::from_value(serde_json::json!({"id":"original","name":"Original","kind":"local","location":original_dir})).unwrap());
        engine.save_state(state).unwrap();
        let original = engine.list_media().unwrap().remove(0);
        let manager = Arc::new(crate::upscale::UpscaleManager::new(engine.clone()));
        manager
            .start(vec![original.id.clone()], crate::upscale::UpscaleMode::Fast)
            .unwrap();
        let start = Instant::now();
        let status = loop {
            let status = manager.status().unwrap();
            if matches!(
                status.phase,
                crate::upscale::Phase::Ready
                    | crate::upscale::Phase::Error
                    | crate::upscale::Phase::Cancelled
            ) {
                break status;
            }
            assert!(start.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.phase, crate::upscale::Phase::Ready, "{status:?}");
        let result = &status.results[0];
        let media_id = result.media_id.as_deref().unwrap();
        let source_id = result.source_id.as_deref().unwrap();
        let published_items =
            providers::list_source(&engine.client, status.source.as_ref().unwrap()).unwrap();
        assert!(published_items
            .iter()
            .any(|item| item.id == media_id && item.source_id == source_id));
        let mut state = engine.snapshot().unwrap();
        for source in &mut state.sources {
            source.enabled = false;
        }
        engine.save_state(state).unwrap();
        let reopened = Engine::new(store).unwrap();
        let preview = reopened.photo_preview(media_id, source_id).unwrap();
        assert!(preview.before.is_some());
        assert_eq!(preview.image.width, 3840);
        assert!(reopened
            .snapshot()
            .unwrap()
            .sources
            .iter()
            .all(|source| !source.enabled));
        reopened.clear_image_cache().unwrap();
        assert!(reopened
            .photo_preview(media_id, source_id)
            .unwrap()
            .before
            .is_some());
        fs::remove_file(&original_path).unwrap();
        assert!(
            reopened
                .photo_preview(media_id, source_id)
                .unwrap()
                .comparison_unavailable
        );
        assert!(reopened
            .photo_preview(&original_path.to_string_lossy(), source_id)
            .is_err());
        let collection = PathBuf::from(status.source.as_ref().unwrap().location.clone());
        reopened.delete_upscale_collection(&collection).unwrap();
        assert!(!collection.exists());
        assert!(reopened.photo_preview(media_id, source_id).is_err());
        assert!(!PathBuf::from(preview.image.url).exists());
    }
    #[test]
    fn cache_trim_pins_both_current_photo_urls() {
        let temp = tempfile::tempdir().unwrap();
        let engine =
            Engine::new(Store::new(temp.path().join("data"), temp.path().join("cache")).unwrap())
                .unwrap();
        let result = engine.store.cache.join("result-photo.jpg");
        let original = engine.store.cache.join("original-photo.jpg");
        let old = engine.store.cache.join("old-photo.jpg");
        for path in [&result, &original, &old] {
            fs::write(path, [0; 16]).unwrap();
        }
        engine
            .trim_cache_excluding(0, &[result.clone(), original.clone()])
            .unwrap();
        assert!(result.exists() && original.exists());
        assert!(!old.exists());
    }
}

#[cfg(test)]
mod lock_state_tests {
    use super::*;
    use serde_json::{json, Value};

    fn fixture() -> (tempfile::TempDir, Engine) {
        let temp = tempfile::tempdir().unwrap();
        let engine =
            Engine::new(Store::new(temp.path().join("data"), temp.path().join("cache")).unwrap())
                .unwrap();
        let state = serde_json::from_value(json!({
            "sources":[{"id":"home","name":"Home","kind":"local","location":"home"}, {"id":"lock","name":"Lock","kind":"local","location":"lock"}],
            "settings":{"lockSourceId":"lock", "lockRunning":true},
            "current":{"id":"home-image","name":"Home","url":"home.jpg","sourceId":"home"},
            "lockCurrent":{"id":"lock-image","name":"Lock","url":"lock.jpg","sourceId":"lock"},
            "lockLastChanged":"2026-11-12T01:02:03Z", "lockError":"OS denied access"
        })).unwrap();
        engine.inner.lock().unwrap().state = state;
        (temp, engine)
    }

    #[test]
    fn lock_status_survives_stale_ui_save_and_restart_without_changing_home() {
        let (_temp, engine) = fixture();
        let mut stale = serde_json::to_value(engine.snapshot().unwrap()).unwrap();
        stale["lockCurrent"] = Value::Null;
        stale["lockLastChanged"] = Value::Null;
        stale["lockError"] = Value::Null;
        let saved = engine
            .save_state(serde_json::from_value(stale).unwrap())
            .unwrap();
        let value = serde_json::to_value(saved).unwrap();
        assert_eq!(value["lockCurrent"]["id"], "lock-image");
        assert_eq!(value["lockLastChanged"], "2026-11-12T01:02:03Z");
        assert_eq!(value["lockError"], "OS denied access");
        assert_eq!(value["current"]["id"], "home-image");
        let reopened = Engine::new(engine.store.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(reopened.snapshot().unwrap()).unwrap()["lockCurrent"]["id"],
            "lock-image"
        );
    }

    #[test]
    fn lock_selection_and_status_clear_when_source_disabled_or_removed() {
        for remove in [false, true] {
            let (_temp, engine) = fixture();
            let mut state = engine.snapshot().unwrap();
            if remove {
                state.sources.retain(|source| source.id != "lock");
            } else {
                state.sources[1].enabled = false;
            }
            let value = serde_json::to_value(engine.save_state(state).unwrap()).unwrap();
            assert_eq!(value["settings"]["lockSourceId"], "");
            assert_eq!(value["settings"]["lockRunning"], false);
            assert_eq!(value["lockCurrent"], Value::Null);
            assert_eq!(value["lockLastChanged"], Value::Null);
            assert_eq!(value["lockError"], Value::Null);
            assert_eq!(value["current"]["id"], "home-image");
        }
    }

    #[test]
    fn home_selection_falls_back_to_all_when_selected_source_is_disabled_or_removed() {
        for remove in [false, true] {
            let (_temp, engine) = fixture();
            let mut state = engine.snapshot().unwrap();
            state.settings.home_source_id = "home".into();
            if remove {
                state.sources.retain(|source| source.id != "home");
            } else {
                state.sources[0].enabled = false;
            }
            let saved = engine.save_state(state).unwrap();
            assert_eq!(saved.settings.home_source_id, "");
            assert_eq!(saved.settings.lock_source_id, "lock");
            let reopened = Engine::new(engine.store.clone()).unwrap();
            assert_eq!(reopened.snapshot().unwrap().settings.home_source_id, "");
        }
    }

    #[test]
    fn home_selection_normalizes_legacy_invalid_saved_source_on_load() {
        let (_temp, engine) = fixture();
        let mut state = engine.snapshot().unwrap();
        state.settings.home_source_id = "removed-source".into();
        engine.store.save(&state).unwrap();
        let reopened = Engine::new(engine.store.clone()).unwrap();
        assert_eq!(reopened.snapshot().unwrap().settings.home_source_id, "");
    }

    #[test]
    fn lock_only_save_preserves_home_rotation_deadline_and_library() {
        let (_temp, engine) = fixture();
        let due = Instant::now() + Duration::from_secs(500);
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.next_due = due;
            let home = runtime.state.current.clone().unwrap();
            runtime.media.push(home);
        }
        let mut state = engine.snapshot().unwrap();
        state.settings.lock_interval_seconds = 123;
        state.settings.lock_running = false;
        engine.save_state(state).unwrap();
        let runtime = engine.inner.lock().unwrap();
        assert_eq!(runtime.next_due, due);
        assert_eq!(runtime.media.len(), 1);
    }

    #[test]
    fn lock_connection_edit_invalidates_native_status_instead_of_accepting_ui_status() {
        let (_temp, engine) = fixture();
        let mut state = engine.snapshot().unwrap();
        state.sources[1].location = "new-lock".into();
        let value = serde_json::to_value(engine.save_state(state).unwrap()).unwrap();
        assert_eq!(value["settings"]["lockSourceId"], "lock");
        assert_eq!(value["lockCurrent"], Value::Null);
        assert_eq!(value["lockLastChanged"], Value::Null);
    }

    #[test]
    fn lock_failure_preserves_home_library_status_and_shuffle() {
        let (temp, engine) = fixture();
        let selected = temp.path().join("lock-source");
        fs::create_dir(&selected).unwrap();
        fs::write(selected.join("corrupt.jpg"), b"invalid image").unwrap();
        {
            let mut runtime = engine.inner.lock().unwrap();
            runtime.state.sources[1].location = selected.to_string_lossy().into_owned();
            let home = runtime.state.current.clone().unwrap();
            runtime.media.push(home);
            runtime.persistent.shuffle.known = vec!["home-image".into()];
        }
        // Corrupt bytes fail before any operating-system wallpaper call.
        let state = engine.rotate_lock(false, None, false).unwrap();
        assert!(state.lock_error.is_some());
        assert_eq!(state.current.as_ref().unwrap().id, "home-image");
        let runtime = engine.inner.lock().unwrap();
        assert_eq!(runtime.media.len(), 1);
        assert_eq!(runtime.persistent.shuffle.known, vec!["home-image"]);
    }

    #[test]
    fn lock_restart_restores_its_deadline_independently_of_home() {
        let (_temp, engine) = fixture();
        let mut state = engine.snapshot().unwrap();
        state.lock_last_changed = Some(chrono::Utc::now().to_rfc3339());
        engine.store.save(&state).unwrap();
        let reopened = Engine::new(engine.store.clone()).unwrap();
        let runtime = reopened.inner.lock().unwrap();
        assert!(runtime.lock_next_due > Instant::now() + Duration::from_secs(800));
        assert!(runtime.next_due <= Instant::now());
    }

    #[test]
    fn lock_automatic_rotation_respects_its_pause_deadline_and_shared_schedule() {
        for control in ["pause", "deadline", "schedule"] {
            let (_temp, engine) = fixture();
            {
                let mut runtime = engine.inner.lock().unwrap();
                runtime.state.running = true;
                match control {
                    "pause" => runtime.state.settings.lock_running = false,
                    "deadline" => runtime.lock_next_due = Instant::now() + Duration::from_secs(600),
                    _ => {
                        runtime.state.settings.schedule_enabled = true;
                        runtime.state.settings.days.clear();
                    }
                }
            }
            let before = serde_json::to_value(engine.snapshot().unwrap()).unwrap();
            let after =
                serde_json::to_value(engine.rotate_lock(true, None, false).unwrap()).unwrap();
            assert_eq!(after, before, "{control}");
        }
    }
}
