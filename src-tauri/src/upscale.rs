//! Local-only, bounded background image processing. Caller values are media IDs,
//! never paths or executable names.
use crate::{commands::Engine, models::Source, storage};
use image::{
    imageops::FilterType, DynamicImage, GenericImageView, ImageDecoder, ImageReader, Limits,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::time::{Duration, Instant};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[path = "upscale_cleanup.rs"]
mod cleanup;
#[path = "upscale_collections.rs"]
mod collections;
#[path = "upscale_policy.rs"]
mod policy;
#[path = "upscale_runtime.rs"]
mod runtime;
pub use collections::{CollectionInfo, StorageInfo};
use policy::{check_operation, target_dimensions, validate_ids};
pub use runtime::ModelInfo;
#[cfg(test)]
use runtime::{verify_hash, ARCHIVE_SHA256};

const MAX_INPUT: u64 = 32 * 1024 * 1024;
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;
const CACHE_BUDGET: u64 = 256 * 1024 * 1024;
const MAX_PREVIEWS: usize = 20;
const ALGORITHM_VERSION: &str = "lanczos3-jpeg92-v1/realesrgan-x4plus-20220424-hd-v2";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpscaleMode {
    #[default]
    Fast,
    Ai,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Idle,
    Downloading,
    Processing,
    Ready,
    Cancelled,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpscaleResult {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub original_preview: String,
    pub result_preview: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpscaleStatus {
    pub job_id: Option<String>,
    pub phase: Phase,
    pub mode: UpscaleMode,
    pub completed: usize,
    pub processed: usize,
    pub failed: usize,
    pub total: usize,
    pub model: Option<String>,
    pub downloaded_bytes: u64,
    pub total_download_bytes: u64,
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub results: Vec<UpscaleResult>,
}

#[derive(Default)]
struct JobState {
    status: UpscaleStatus,
    running: bool,
    revision: u64,
    cancel: Arc<AtomicBool>,
    outputs: Vec<PathBuf>,
    saved: Option<Source>,
}

pub struct UpscaleManager {
    engine: Arc<Engine>,
    inner: Mutex<JobState>,
    // Serializes lifecycle changes and publication; never held by processing.
    control: Mutex<()>,
}

impl UpscaleManager {
    pub fn new(engine: Arc<Engine>) -> Self {
        // Startup is the only point with no active worker. A malformed saved
        // configuration must never make durable collections look orphaned.
        let references = match fs::read(engine.store.root.join("state.json")) {
            Ok(bytes) => serde_json::from_slice::<crate::models::AppState>(&bytes)
                .ok()
                .filter(|state| state.version == 1)
                .map(|state| local_references(&state.sources)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(Vec::new()),
            Err(_) => None,
        };
        let _ = cleanup::startup(
            &engine.store.cache,
            &engine.store.root,
            references.as_deref(),
        );
        Self {
            engine,
            inner: Mutex::new(JobState::default()),
            control: Mutex::new(()),
        }
    }

    pub fn status(&self) -> Result<UpscaleStatus, String> {
        Ok(self.inner.lock().map_err(|e| e.to_string())?.status.clone())
    }

    pub fn start(
        self: &Arc<Self>,
        ids: Vec<String>,
        mode: UpscaleMode,
    ) -> Result<UpscaleStatus, String> {
        self.start_with_model(ids, mode, None)
    }

    pub fn start_with_model(
        self: &Arc<Self>,
        ids: Vec<String>,
        mode: UpscaleMode,
        model: Option<&str>,
    ) -> Result<UpscaleStatus, String> {
        validate_ids(&ids)?;
        self.launch(Some(ids), None, mode, model)
    }

    pub fn start_all(
        self: &Arc<Self>,
        source_id: Option<String>,
        mode: UpscaleMode,
        model: Option<&str>,
    ) -> Result<UpscaleStatus, String> {
        self.launch(None, source_id, mode, model)
    }

    fn launch(
        self: &Arc<Self>,
        ids: Option<Vec<String>>,
        source_id: Option<String>,
        mode: UpscaleMode,
        model: Option<&str>,
    ) -> Result<UpscaleStatus, String> {
        let model = runtime::parse_model(model)?.to_owned();
        #[cfg(not(windows))]
        if mode == UpscaleMode::Ai {
            return Err("AI processing requires Windows and a Vulkan-capable GPU.".into());
        }
        let _control = self.control.lock().map_err(|e| e.to_string())?;
        let (job_id, cancel, revision, previous, status) = {
            let mut state = self.inner.lock().map_err(|e| e.to_string())?;
            if state.running {
                return Err(
                    "An image processing job is already running. Cancel it and wait for cleanup."
                        .into(),
                );
            }
            let previous = state.status.job_id.clone();
            let job_id = format!(
                "{:016x}{:016x}",
                rand::random::<u64>(),
                rand::random::<u64>()
            );
            let revision = self.engine.revision.load(Ordering::SeqCst);
            *state = JobState {
                status: UpscaleStatus {
                    job_id: Some(job_id.clone()),
                    phase: if mode == UpscaleMode::Ai {
                        Phase::Downloading
                    } else {
                        Phase::Processing
                    },
                    mode,
                    model: Some(model.clone()),
                    total: ids.as_ref().map_or(0, Vec::len),
                    message: Some("Preparing local image processing…".into()),
                    ..Default::default()
                },
                running: true,
                revision,
                ..Default::default()
            };
            (
                job_id,
                state.cancel.clone(),
                revision,
                previous,
                state.status.clone(),
            )
        };
        let manager = self.clone();
        std::thread::Builder::new()
            .name("wallta-upscale".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Some(previous) = previous {
                        let _ = fs::remove_dir_all(manager.job_directory(&previous));
                    }
                    manager.process(
                        &job_id,
                        ids,
                        source_id.as_deref(),
                        mode,
                        &model,
                        &cancel,
                        revision,
                    )
                }))
                .unwrap_or_else(|_| Err("Image processing failed unexpectedly.".into()));
                let should_cleanup = {
                    let mut state = manager.inner.lock().unwrap_or_else(|e| e.into_inner());
                    state.running = false;
                    if cancel.load(Ordering::SeqCst) {
                        state.status.phase = Phase::Cancelled;
                    } else if let Err(error) = result {
                        state.status.phase = Phase::Error;
                        state.status.error = Some(error);
                    } else {
                        state.status.phase = Phase::Ready;
                        state.status.message = Some(
                            "Completed images have been saved to the local Ảnh 4K collection."
                                .into(),
                        );
                    }
                    state.saved.is_none()
                        && matches!(state.status.phase, Phase::Cancelled | Phase::Error)
                };
                if should_cleanup {
                    let _ = fs::remove_dir_all(manager.job_directory(&job_id));
                    if let Ok(runtime) = manager.engine.inner.lock() {
                        let references = local_references(&runtime.state.sources);
                        let _ = cleanup::remove_unreferenced(
                            &manager.engine.store.root,
                            &manager.engine.store.root.join("upscaled").join(&job_id),
                            &references,
                        );
                    }
                }
            })
            .map_err(|e| {
                if let Ok(mut state) = self.inner.lock() {
                    state.running = false;
                    state.status.phase = Phase::Error;
                    state.status.error = Some(e.to_string());
                }
                e.to_string()
            })?;
        Ok(status)
    }

    pub fn cancel(&self, job_id: Option<&str>) -> Result<UpscaleStatus, String> {
        let _control = self.control.lock().map_err(|e| e.to_string())?;
        let cleanup = {
            let mut state = self.inner.lock().map_err(|e| e.to_string())?;
            if job_id.is_some_and(|id| state.status.job_id.as_deref() != Some(id)) {
                return Ok(state.status.clone());
            }
            state.cancel.store(true, Ordering::SeqCst);
            if state.status.job_id.is_some() {
                state.status.phase = Phase::Cancelled;
            }
            state.status.error = None;
            if state.running || state.saved.is_some() {
                None
            } else {
                state.status.job_id.clone()
            }
        };
        if let Some(job_id) = cleanup {
            let _ = fs::remove_dir_all(self.job_directory(&job_id));
        }
        self.status()
    }

    pub fn save(&self, job_id: &str) -> Result<Source, String> {
        let _control = self.control.lock().map_err(|e| e.to_string())?;
        let (outputs, revision, cancel) = {
            let state = self.inner.lock().map_err(|e| e.to_string())?;
            if state.status.job_id.as_deref() != Some(job_id) {
                return Err("This processing job is no longer available.".into());
            }
            if let Some(saved) = &state.saved {
                return Ok(saved.clone());
            }
            if !matches!(state.status.phase, Phase::Ready | Phase::Error)
                || state.running
                || state.outputs.is_empty()
            {
                return Err("Wait for all results before saving.".into());
            }
            (state.outputs.clone(), state.revision, state.cancel.clone())
        };
        self.check(&cancel, revision)?;
        let parent = self.engine.store.root.join("upscaled");
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let staging = parent.join(format!(".{job_id}.pending"));
        let destination = parent.join(job_id);
        let mut created_destination = false;
        // A directory from an earlier failed native append is safe to retry: it
        // contains copies only, and the native helper is idempotent by location.
        if !destination.is_dir() {
            fs::create_dir(&staging).map_err(|e| e.to_string())?;
            let copied = (|| {
                for (index, path) in outputs.iter().enumerate() {
                    self.check(&cancel, revision)?;
                    let bytes = read_bounded(path, MAX_OUTPUT)?;
                    storage::atomic_write(&staging.join(format!("{:03}.jpg", index + 1)), &bytes)?;
                }
                self.check(&cancel, revision)?;
                fs::rename(&staging, &destination).map_err(|e| e.to_string())
            })();
            if let Err(error) = copied {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
            created_destination = true;
        }
        // Root helper performs the final revision check under the fresh-state
        // mutation lock, closing the race between the check and source append.
        let published = self
            .check(&cancel, revision)
            .and_then(|()| self.engine.accept_upscale_source(&destination, revision));
        let source = match published {
            Ok(source) => source,
            Err(error) => {
                if created_destination {
                    // Source writes hold this same lock. Do not remove copies
                    // if another UI action has already adopted their folder.
                    if let Ok(runtime) = self.engine.inner.lock() {
                        let references = local_references(&runtime.state.sources);
                        let _ = cleanup::remove_unreferenced(
                            &self.engine.store.root,
                            &destination,
                            &references,
                        );
                    }
                }
                return Err(error);
            }
        };
        self.inner.lock().map_err(|e| e.to_string())?.saved = Some(source.clone());
        Ok(source)
    }

    fn job_directory(&self, id: &str) -> PathBuf {
        self.engine.store.cache.join("upscale/jobs").join(id)
    }
    fn check(&self, cancel: &AtomicBool, revision: u64) -> Result<(), String> {
        check_operation(
            cancel,
            revision,
            self.engine.revision.load(Ordering::SeqCst),
        )
    }

    pub fn models(&self) -> Result<Vec<ModelInfo>, String> {
        Ok(runtime::catalog(&self.engine.store.root))
    }

    pub fn storage(&self) -> Result<StorageInfo, String> {
        let state = self.engine.snapshot()?;
        collections::catalog(
            &self.engine.store.root,
            &self.engine.store.cache,
            &state.sources,
        )
    }

    pub fn clear_cache(&self) -> Result<StorageInfo, String> {
        let _control = self.control.lock().map_err(|e| e.to_string())?;
        if self.inner.lock().map_err(|e| e.to_string())?.running {
            return Err("Wait for image processing and downloads before clearing cache.".into());
        }
        self.engine.clear_image_cache()?;
        if let Some(base) = cleanup::managed_base(&self.engine.store.cache, "upscale") {
            // Only owned, validated cache subtrees. Models and saved files live elsewhere.
            for folder in ["results", "jobs"] {
                let path = base.join(folder);
                if cleanup::plain_directory(&path)
                    && path
                        .canonicalize()
                        .is_ok_and(|path| path.parent() == Some(base.as_path()))
                {
                    fs::remove_dir_all(path).map_err(|e| e.to_string())?;
                }
            }
        }
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .status
            .results
            .clear();
        self.storage()
    }

    pub fn delete_collection(&self, id: &str) -> Result<StorageInfo, String> {
        let _control = self.control.lock().map_err(|e| e.to_string())?;
        {
            let state = self.inner.lock().map_err(|e| e.to_string())?;
            if state.running && state.status.job_id.as_deref() == Some(id) {
                return Err(
                    "This collection is still being written. Cancel and wait before deleting it."
                        .into(),
                );
            }
        }
        let path = cleanup::collection_path(&self.engine.store.root, id)?;
        self.engine.delete_upscale_collection(&path)?;
        let mut state = self.inner.lock().map_err(|e| e.to_string())?;
        if state.status.job_id.as_deref() == Some(id) {
            state.saved = None;
            state.status.source = None;
            state.outputs.clear();
            state.status.results.clear();
        }
        drop(state);
        self.storage()
    }

    fn process(
        &self,
        job: &str,
        ids: Option<Vec<String>>,
        source_id: Option<&str>,
        mode: UpscaleMode,
        model: &str,
        cancel: &AtomicBool,
        mut revision: u64,
    ) -> Result<(), String> {
        self.check(cancel, revision)?;
        let all = if ids.is_none() {
            Some(self.engine.upscale_all_items(source_id, revision)?)
        } else {
            None
        };
        let ids = ids.unwrap_or_else(|| {
            all.as_ref()
                .unwrap()
                .iter()
                .map(|item| item.id.clone())
                .collect()
        });
        if ids.is_empty() {
            return Err("No images found in the enabled original sources.".into());
        }
        self.inner.lock().map_err(|e| e.to_string())?.status.total = ids.len();
        let directory = self.job_directory(job);
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let cache = self.engine.store.cache.join("upscale/results");
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        prune_results(&cache, CACHE_BUDGET)?;
        let mut ai_runtime = None;
        let mut manifest = CollectionInfo {
            id: job.into(),
            name: "Ảnh 4K".into(),
            model: (mode == UpscaleMode::Ai).then(|| model.to_owned()),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            count: 0,
            bytes: 0,
            source_id: None,
        };
        for (index, id) in ids.iter().enumerate() {
            self.check(cancel, revision)?;
            collections::ensure_free_space(&self.engine.store.root, 3 * MAX_OUTPUT + MAX_INPUT)?;
            {
                let mut state = self.inner.lock().map_err(|e| e.to_string())?;
                state.status.phase = Phase::Processing;
                state.status.message =
                    Some(format!("Processing image {} of {}…", index + 1, ids.len()));
            }
            let input = if let Some(items) = &all {
                self.engine
                    .upscale_snapshot_input(&items[index], revision)
                    .map(|bytes| (items[index].clone(), bytes))
            } else {
                self.engine
                    .upscale_input(id)
                    .and_then(|(media, _, bytes, input_revision)| {
                        check_operation(cancel, revision, input_revision)?;
                        Ok((media, bytes))
                    })
            };
            let (media, bytes) = match input {
                Ok(input) => input,
                Err(error) => {
                    self.record_failure(&error)?;
                    return Err(error);
                }
            };
            let prepared = self.prepare_image(
                &bytes,
                mode,
                model,
                &directory,
                &cache,
                cancel,
                revision,
                &mut ai_runtime,
            );
            let (output, width, height, before, after) = match prepared {
                Ok(prepared) => prepared,
                Err(ImageFailure::Item(error)) => {
                    self.check(cancel, revision)?;
                    self.record_failure(&error)?;
                    continue;
                }
                Err(ImageFailure::Fatal(error)) => {
                    self.record_failure(&error)?;
                    return Err(error);
                }
            };
            // Cancellation and publication share this short lifecycle lock.
            let _control = self.control.lock().map_err(|e| e.to_string())?;
            self.check(cancel, revision)?;
            let parent = self.engine.store.root.join("upscaled");
            fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
            if !cleanup::plain_directory(&parent) {
                return Err("Managed image storage contains an unsafe link.".into());
            }
            let collection = parent.join(job);
            if !collection.exists() {
                fs::create_dir(&collection).map_err(|e| e.to_string())?;
            }
            // Authorize the direct owned folder before writing or publishing any output.
            let collection = cleanup::collection_path(&self.engine.store.root, job)?;
            let path = collection.join(format!("{:03}.jpg", manifest.count + 1));
            storage::atomic_write(&path, &output)?;
            manifest.count += 1;
            manifest.bytes += output.len() as u64;
            collections::write_manifest(&collection, &manifest)?;
            let published = self.inner.lock().map_err(|e| e.to_string())?.saved.clone();
            let source = match published {
                Some(source) => {
                    self.check(cancel, revision)?;
                    source
                }
                None => {
                    let (source, own_revision) = self
                        .engine
                        .accept_upscale_source_with_revision(&collection, revision)?;
                    // This revision comes from inside the publication lock; never load-and-rebase
                    // an unrelated external mutation after releasing the engine lock.
                    revision = own_revision;
                    source
                }
            };
            let mut state = self.inner.lock().map_err(|e| e.to_string())?;
            state.revision = revision;
            state.saved = Some(source.clone());
            state.status.source = Some(source.clone());
            state.status.completed += 1;
            state.status.processed += 1;
            manifest.source_id = Some(source.id);
            // Retain at most twenty preview pairs, regardless of the batch size.
            if state.status.results.len() < MAX_PREVIEWS {
                let before_path = directory.join(format!("{index}-before.jpg"));
                let after_path = directory.join(format!("{index}-after.jpg"));
                storage::atomic_write(&before_path, &before)?;
                storage::atomic_write(&after_path, &after)?;
                state.status.results.push(UpscaleResult {
                    id: media.id,
                    name: media.name,
                    width,
                    height,
                    original_preview: before_path.to_string_lossy().into_owned(),
                    result_preview: after_path.to_string_lossy().into_owned(),
                });
            }
        }
        self.check(cancel, revision)?;
        let state = self.inner.lock().map_err(|e| e.to_string())?;
        if state.status.failed > 0 {
            return Err(format!(
                "{} images could not be processed. Completed images have been saved.",
                state.status.failed
            ));
        }
        Ok(())
    }

    fn record_failure(&self, error: &str) -> Result<(), String> {
        let mut state = self.inner.lock().map_err(|e| e.to_string())?;
        state.status.failed += 1;
        state.status.processed += 1;
        state.status.message = Some(error.to_owned());
        Ok(())
    }

    fn prepare_image(
        &self,
        bytes: &[u8],
        mode: UpscaleMode,
        model: &str,
        directory: &Path,
        cache: &Path,
        cancel: &AtomicBool,
        revision: u64,
        ai_runtime: &mut Option<PathBuf>,
    ) -> Result<(Vec<u8>, u32, u32, Vec<u8>, Vec<u8>), ImageFailure> {
        use ImageFailure::{Fatal, Item};
        if bytes.len() as u64 > MAX_INPUT {
            return Err(Item("Original image exceeds the 32 MB limit.".into()));
        }
        let original = decode(bytes).map_err(Item)?;
        let before = jpeg(&original.thumbnail(1200, 1200)).map_err(Item)?;
        let key = model_cache_key(bytes, mode, model);
        let cached = cache.join(format!("{key}.jpg"));
        let output = match read_bounded(&cached, MAX_OUTPUT).and_then(|bytes| {
            if decode(&bytes)?.dimensions()
                != target_dimensions(original.width(), original.height())?
            {
                return Err("Invalid cached result.".into());
            }
            Ok(bytes)
        }) {
            Ok(bytes) => bytes,
            Err(_) => {
                let bytes = if mode == UpscaleMode::Ai
                    && policy::needs_ai(original.width(), original.height()).map_err(Item)?
                {
                    if ai_runtime.is_none() {
                        self.inner
                            .lock()
                            .map_err(|e| Fatal(e.to_string()))?
                            .status
                            .phase = Phase::Downloading;
                        *ai_runtime = Some(
                            runtime::prepare(
                                &self.engine.store.root,
                                model,
                                cancel,
                                || self.check(cancel, revision),
                                |downloaded, total| {
                                    if let Ok(mut state) = self.inner.lock() {
                                        state.status.downloaded_bytes = downloaded;
                                        state.status.total_download_bytes = total;
                                    }
                                },
                            )
                            .map_err(Fatal)?,
                        );
                        self.inner
                            .lock()
                            .map_err(|e| Fatal(e.to_string()))?
                            .status
                            .phase = Phase::Processing;
                    }
                    runtime::process(
                        ai_runtime
                            .as_ref()
                            .ok_or_else(|| Fatal("AI runtime is unavailable.".into()))?,
                        model,
                        &original,
                        directory,
                        cancel,
                        || self.check(cancel, revision),
                    )
                    .map_err(Item)?
                } else {
                    fast_process(bytes).map_err(Item)?
                };
                self.check(cancel, revision).map_err(Fatal)?;
                if bytes.len() as u64 > MAX_OUTPUT {
                    return Err(Item("Processed image exceeds the 64 MB limit.".into()));
                }
                prune_results(cache, CACHE_BUDGET.saturating_sub(bytes.len() as u64))
                    .map_err(Fatal)?;
                storage::atomic_write(&cached, &bytes).map_err(Fatal)?;
                bytes
            }
        };
        let result = decode(&output).map_err(Item)?;
        let after = jpeg(&result.thumbnail(1200, 1200)).map_err(Item)?;
        Ok((output, result.width(), result.height(), before, after))
    }
}

enum ImageFailure {
    Item(String),
    Fatal(String),
}

pub(crate) fn checked_collection_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    cleanup::collection_path(root, id)
}

fn local_references(sources: &[Source]) -> Vec<PathBuf> {
    sources
        .iter()
        .filter(|source| source.kind == "local")
        .flat_map(|source| {
            std::iter::once(PathBuf::from(&source.location)).chain(
                source
                    .folders
                    .iter()
                    .map(|folder| PathBuf::from(&folder.location)),
            )
        })
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

fn cache_key(bytes: &[u8], mode: UpscaleMode) -> String {
    model_cache_key(bytes, mode, "photo")
}

fn model_cache_key(bytes: &[u8], mode: UpscaleMode, model: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(ALGORITHM_VERSION);
    hash.update(model.as_bytes());
    hash.update([mode as u8]);
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    decode_limited(bytes, 32_000_000)
}

fn decode_limited(bytes: &[u8], max_pixels: u64) -> Result<DynamicImage, String> {
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err("Image exceeds the processing byte limit.".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let format = reader.format();
    let (w, h) =
        ImageReader::with_format(Cursor::new(bytes), format.ok_or("Unknown image format.")?)
            .into_dimensions()
            .map_err(|e| e.to_string())?;
    if u64::from(w) * u64::from(h) > max_pixels {
        return Err("Image exceeds the processing pixel limit.".into());
    }
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("Image cannot be decoded within the memory limit: {e}"))?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Image cannot be decoded within the memory limit: {e}"))?;
    image.apply_orientation(orientation);
    Ok(image)
}

fn jpeg(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 92)
        .encode_image(&image.to_rgb8())
        .map_err(|e| e.to_string())?;
    Ok(output)
}

fn fast_process(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let image = decode(bytes)?;
    let (w, h) = target_dimensions(image.width(), image.height())?;
    jpeg(&image.resize_exact(w, h, FilterType::Lanczos3))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("Processing file exceeds its size limit.".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Processing file exceeds its size limit.".into());
    }
    Ok(bytes)
}

fn prune_results(directory: &Path, budget: u64) -> Result<(), String> {
    let mut files = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !metadata.is_file()
                || name.len() != 68
                || !name.ends_with(".jpg")
                || !name.as_bytes()[..64].iter().all(|b| b.is_ascii_hexdigit())
            {
                return None;
            }
            Some((metadata.modified().ok(), metadata.len(), entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|file| file.0);
    let mut size = files.iter().map(|file| file.1).sum::<u64>();
    for (_, length, path) in files {
        if size <= budget {
            break;
        }
        fs::remove_file(path).map_err(|e| e.to_string())?;
        size -= length;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_results_have_separate_cache_identity() {
        let bytes = png_test_image();
        assert_ne!(
            model_cache_key(&bytes, UpscaleMode::Ai, "photo"),
            model_cache_key(&bytes, UpscaleMode::Ai, "anime")
        );
        assert_ne!(
            model_cache_key(&bytes, UpscaleMode::Ai, "anime"),
            model_cache_key(&bytes, UpscaleMode::Ai, "anime-light")
        );
    }

    #[test]
    fn cache_cleanup_preserves_models_and_collections_and_rejects_active_worker() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::new(
                crate::storage::Store::new(temp.path().join("data"), temp.path().join("cache"))
                    .unwrap(),
            )
            .unwrap(),
        );
        let manager = UpscaleManager::new(engine.clone());
        let saved = engine
            .store
            .root
            .join("upscaled/0123456789abcdef0123456789abcdef");
        let models = engine.store.root.join("ai-runtime/models");
        fs::create_dir_all(&saved).unwrap();
        fs::create_dir_all(&models).unwrap();
        fs::write(saved.join("001.jpg"), b"saved").unwrap();
        fs::write(models.join("model.bin"), b"model").unwrap();
        let cache = engine.store.cache.join("upscale/results");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("result.jpg"), b"reusable").unwrap();
        fs::write(engine.store.cache.join("original.img"), b"original cache").unwrap();
        manager.inner.lock().unwrap().running = true;
        assert!(manager.clear_cache().is_err());
        assert!(cache.join("result.jpg").exists());
        manager.inner.lock().unwrap().running = false;
        let revision = engine.revision.load(Ordering::SeqCst);
        let storage = manager.clear_cache().unwrap();
        assert_eq!(storage.cache_bytes, 0);
        assert_eq!(storage.collections.len(), 1);
        assert_eq!(storage.collections[0].count, 1);
        assert!(saved.join("001.jpg").is_file());
        assert!(models.join("model.bin").is_file());
        assert_eq!(engine.revision.load(Ordering::SeqCst), revision);
        assert!(manager.delete_collection("../outside").is_err());
        manager
            .delete_collection("0123456789abcdef0123456789abcdef")
            .unwrap();
        assert!(!saved.exists());
    }

    #[test]
    fn all_snapshots_more_than_fifty_originals_and_excludes_owned_outputs() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::new(
                crate::storage::Store::new(temp.path().join("data"), temp.path().join("cache"))
                    .unwrap(),
            )
            .unwrap(),
        );
        let originals = temp.path().join("originals");
        fs::create_dir(&originals).unwrap();
        for index in 0..61 {
            fs::write(originals.join(format!("{index}.png")), png_test_image()).unwrap();
        }
        let owned = engine
            .store
            .root
            .join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&owned).unwrap();
        fs::write(owned.join("old.jpg"), b"previous result").unwrap();
        {
            let mut runtime = engine.inner.lock().unwrap();
            for (id, location) in [("originals", originals), ("upscale-old", owned)] {
                runtime.state.sources.push(
                    serde_json::from_value(serde_json::json!({
                        "id": id, "name": id, "kind":"local", "location":location
                    }))
                    .unwrap(),
                );
            }
        }
        let manager = Arc::new(UpscaleManager::new(engine.clone()));
        manager.start_all(None, UpscaleMode::Fast, None).unwrap();
        let status = wait_finished(&manager);
        assert_eq!(status.total, 61);
        assert_eq!(status.processed, 61);
        assert_eq!(status.completed, 61);
        assert_eq!(status.failed, 0);
        assert!(status.results.len() <= 20);
        let source = status.source.unwrap();
        assert!(!source.enabled);
        assert_eq!(
            fs::read_dir(&source.location)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jpg"))
                .count(),
            61
        );
        assert_eq!(engine.snapshot().unwrap().sources.len(), 3);
        assert_eq!(
            manager.save(status.job_id.as_deref().unwrap()).unwrap().id,
            source.id
        );
    }

    #[test]
    fn cancellation_keeps_already_published_outputs_and_save_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::new(
                crate::storage::Store::new(temp.path().join("data"), temp.path().join("cache"))
                    .unwrap(),
            )
            .unwrap(),
        );
        let manager = UpscaleManager::new(engine.clone());
        let directory = engine
            .store
            .root
            .join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("001.jpg"), b"completed").unwrap();
        let source = engine.accept_upscale_source(&directory, 0).unwrap();
        {
            let mut state = manager.inner.lock().unwrap();
            state.status.job_id = Some("0123456789abcdef0123456789abcdef".into());
            state.status.completed = 1;
            state.status.phase = Phase::Processing;
            state.status.source = Some(source.clone());
            state.saved = Some(source.clone());
        }
        let status = manager.cancel(None).unwrap();
        assert_eq!(status.phase, Phase::Cancelled);
        assert_eq!(status.completed, 1);
        assert!(directory.join("001.jpg").is_file());
        assert_eq!(
            manager.save(status.job_id.as_deref().unwrap()).unwrap().id,
            source.id
        );
    }

    fn wait_finished(manager: &UpscaleManager) -> UpscaleStatus {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let status = manager.status().unwrap();
            if matches!(status.phase, Phase::Ready | Phase::Error | Phase::Cancelled)
                && !manager.inner.lock().unwrap().running
            {
                return status;
            }
            assert!(Instant::now() < deadline, "Worker timed out: {status:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn rejects_unknown_modes() {
        assert!(serde_json::from_str::<UpscaleMode>("\"cloud\"").is_err());
    }

    #[test]
    fn runtime_hash_rejects_modified_downloads() {
        assert!(verify_hash(b"modified runtime", ARCHIVE_SHA256).is_err());
        assert!(verify_hash(
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .is_ok());
    }

    #[test]
    fn cache_key_binds_content_mode_and_model_version() {
        assert_ne!(
            cache_key(b"a", UpscaleMode::Fast),
            cache_key(b"b", UpscaleMode::Fast)
        );
        assert_ne!(
            cache_key(b"a", UpscaleMode::Fast),
            cache_key(b"a", UpscaleMode::Ai)
        );
    }

    #[test]
    fn fast_processing_makes_a_real_jpeg_and_preserves_original_bytes() {
        let image = image::DynamicImage::new_rgb8(1, 100);
        let mut original = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut original, image::ImageFormat::Png)
            .unwrap();
        let bytes = original.into_inner();
        let before = bytes.clone();
        let output = fast_process(&bytes).unwrap();
        assert_eq!(bytes, before);
        assert_eq!(
            image::load_from_memory(&output).unwrap().dimensions(),
            (38, 3840)
        );
        assert!(output.starts_with(&[0xff, 0xd8]));
    }

    #[test]
    fn phone_jpeg_orientation_is_applied_before_target_dimensions() {
        let original = jpeg(&image::DynamicImage::new_rgb8(8, 4)).unwrap();
        // Minimal little-endian EXIF IFD: orientation=6 (90 degrees clockwise).
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut bytes = original[..2].to_vec();
        bytes.extend_from_slice(&[0xff, 0xe1]);
        bytes.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(exif);
        bytes.extend_from_slice(&original[2..]);
        assert_eq!(decode(&bytes).unwrap().dimensions(), (4, 8));
        assert_eq!(
            decode(&fast_process(&bytes).unwrap()).unwrap().dimensions(),
            (1920, 3840)
        );
    }

    #[test]
    fn save_is_idempotent_and_original_is_not_modified() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let cache = temp.path().join("cache");
        let engine =
            Arc::new(Engine::new(crate::storage::Store::new(root, cache).unwrap()).unwrap());
        let original = temp.path().join("original.png");
        fs::write(&original, b"original untouched").unwrap();
        let manager = UpscaleManager::new(engine.clone());
        let directory = manager.job_directory("test-job");
        fs::create_dir_all(&directory).unwrap();
        let result = directory.join("0.jpg");
        fs::write(&result, fast_process(&png_test_image()).unwrap()).unwrap();
        {
            let mut state = manager.inner.lock().unwrap();
            state.status.job_id = Some("test-job".into());
            state.status.phase = Phase::Ready;
            state.revision = engine.revision.load(Ordering::SeqCst);
            state.outputs.push(result);
        }
        let first = manager.save("test-job").unwrap();
        let second = manager.save("test-job").unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(engine.snapshot().unwrap().sources.len(), 1);
        assert_eq!(fs::read(&original).unwrap(), b"original untouched");
        assert!(Path::new(&first.location).join("001.jpg").is_file());
    }

    #[test]
    fn later_image_failure_keeps_completed_full_resolution_results_saveable() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::new(
                crate::storage::Store::new(temp.path().join("data"), temp.path().join("cache"))
                    .unwrap(),
            )
            .unwrap(),
        );
        let original = temp.path().join("original.png");
        let bytes = png_test_image();
        fs::write(&original, &bytes).unwrap();
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"original", "name":"Original", "kind":"local", "location":temp.path()
        }))
        .unwrap();
        let media = crate::providers::item(original.to_string_lossy().into(), None, &source.id);
        {
            let mut state = engine.inner.lock().unwrap();
            state.state.sources.push(source);
            state.media.push(media.clone());
        }
        let manager = Arc::new(UpscaleManager::new(engine.clone()));
        let started = manager
            .start(vec![media.id, "missing-image".into()], UpscaleMode::Fast)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            let status = manager.status().unwrap();
            if status.phase == Phase::Error {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "Worker did not finish: {:?}",
                status
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.completed, 1);
        assert_eq!(status.total, 2);
        assert_eq!(
            status.results.len(),
            1,
            "A later failure discarded a completed image"
        );
        assert!(Path::new(&status.results[0].result_preview).is_file());
        let saved = manager.save(started.job_id.as_deref().unwrap()).unwrap();
        let saved_bytes = fs::read(Path::new(&saved.location).join("001.jpg")).unwrap();
        assert_eq!(decode(&saved_bytes).unwrap().dimensions(), (38, 3840));
        assert_eq!(fs::read(original).unwrap(), bytes);
        assert_eq!(
            manager.save(started.job_id.as_deref().unwrap()).unwrap().id,
            saved.id
        );
    }

    #[test]
    fn stale_results_and_cancelled_results_cannot_be_saved() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::new(
                crate::storage::Store::new(temp.path().join("data"), temp.path().join("cache"))
                    .unwrap(),
            )
            .unwrap(),
        );
        let manager = UpscaleManager::new(engine.clone());
        {
            let mut state = manager.inner.lock().unwrap();
            state.status.job_id = Some("stale".into());
            state.status.phase = Phase::Ready;
            state.outputs.push(temp.path().join("missing.jpg"));
        }
        engine.revision.fetch_add(1, Ordering::SeqCst);
        assert!(manager
            .save("stale")
            .unwrap_err()
            .contains("Sources changed"));
        assert_eq!(manager.cancel(Some("wrong")).unwrap().phase, Phase::Ready);
        assert_eq!(
            manager.cancel(Some("stale")).unwrap().phase,
            Phase::Cancelled
        );
        assert!(manager.save("stale").is_err());
    }

    #[test]
    fn failed_native_append_removes_only_new_unpublished_copies() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::new(
                crate::storage::Store::new(temp.path().join("data"), temp.path().join("cache"))
                    .unwrap(),
            )
            .unwrap(),
        );
        let manager = UpscaleManager::new(engine.clone());
        let job = "0123456789abcdef0123456789abcdef";
        let directory = manager.job_directory(job);
        fs::create_dir_all(&directory).unwrap();
        let output = directory.join("0.jpg");
        fs::write(&output, b"processed copy").unwrap();
        {
            let mut state = manager.inner.lock().unwrap();
            state.status.job_id = Some(job.into());
            state.status.phase = Phase::Ready;
            state.revision = engine.revision.load(Ordering::SeqCst);
            state.outputs.push(output.clone());
        }
        fs::create_dir(engine.store.root.join("state.json")).unwrap();
        assert!(manager.save(job).is_err());
        assert!(!engine.store.root.join("upscaled").join(job).exists());
        assert!(output.is_file());
        assert!(engine.snapshot().unwrap().sources.is_empty());
    }

    fn png_test_image() -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 100)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }
}
