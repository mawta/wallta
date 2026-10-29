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
#[path = "upscale_policy.rs"]
mod policy;
#[path = "upscale_runtime.rs"]
mod runtime;
use policy::{check_operation, target_dimensions, validate_ids};
#[cfg(test)]
use runtime::{verify_hash, ARCHIVE_SHA256};

const MAX_INPUT: u64 = 32 * 1024 * 1024;
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;
const CACHE_BUDGET: u64 = 256 * 1024 * 1024;
const JOB_BUDGET: u64 = 768 * 1024 * 1024;
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
    pub total: usize,
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
        validate_ids(&ids)?;
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
                    total: ids.len(),
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
                    manager.process(&job_id, &ids, mode, &cancel, revision)
                }))
                .unwrap_or_else(|_| Err("Image processing failed unexpectedly.".into()));
                let should_cleanup = {
                    let mut state = manager.inner.lock().unwrap_or_else(|e| e.into_inner());
                    state.running = false;
                    if cancel.load(Ordering::SeqCst) {
                        state.status.phase = Phase::Cancelled;
                        state.status.results.clear();
                        state.outputs.clear();
                    } else if let Err(error) = result {
                        state.status.phase = Phase::Error;
                        state.status.error = Some(error);
                    } else {
                        state.status.phase = Phase::Ready;
                        state.status.message = Some(
                            "Results are ready. Save to create a local Ảnh 4K collection.".into(),
                        );
                    }
                    state.status.phase == Phase::Cancelled
                        || (state.status.phase == Phase::Error && state.outputs.is_empty())
                };
                if should_cleanup {
                    let _ = fs::remove_dir_all(manager.job_directory(&job_id));
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
            if job_id.is_some_and(|id| state.status.job_id.as_deref() != Some(id))
                || state.saved.is_some()
            {
                return Ok(state.status.clone());
            }
            state.cancel.store(true, Ordering::SeqCst);
            if state.status.job_id.is_some() {
                state.status.phase = Phase::Cancelled;
            }
            state.status.results.clear();
            state.outputs.clear();
            state.status.error = None;
            if state.running {
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

    fn process(
        &self,
        job: &str,
        ids: &[String],
        mode: UpscaleMode,
        cancel: &AtomicBool,
        revision: u64,
    ) -> Result<(), String> {
        self.check(cancel, revision)?;
        let directory = self.job_directory(job);
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let cache = self.engine.store.cache.join("upscale/results");
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        prune_results(&cache, CACHE_BUDGET)?;
        let mut runtime = None;
        let mut disk_used = 0;
        for (index, id) in ids.iter().enumerate() {
            self.check(cancel, revision)?;
            // Each AI invocation has its own timeout. A progressing batch may
            // legitimately take longer than 30 minutes across multiple images.
            {
                let mut state = self.inner.lock().map_err(|e| e.to_string())?;
                state.status.phase = Phase::Processing;
                state.status.message =
                    Some(format!("Processing image {} of {}…", index + 1, ids.len()));
            }
            let (media, _, bytes, input_revision) = self.engine.upscale_input(id)?;
            check_operation(cancel, revision, input_revision)?;
            self.check(cancel, revision)?;
            if bytes.len() as u64 > MAX_INPUT {
                return Err("Original image exceeds the 32 MB limit.".into());
            }
            let original = decode(&bytes)?;
            let original_preview = jpeg(&original.thumbnail(1200, 1200))?;
            let key = cache_key(&bytes, mode);
            let cached = cache.join(format!("{key}.jpg"));
            let output = match read_bounded(&cached, MAX_OUTPUT).and_then(|bytes| {
                let decoded = decode(&bytes)?;
                if decoded.dimensions() != target_dimensions(original.width(), original.height())? {
                    return Err("Invalid cached result.".into());
                }
                Ok(bytes)
            }) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let bytes = if mode == UpscaleMode::Ai
                        && policy::needs_ai(original.width(), original.height())?
                    {
                        if runtime.is_none() {
                            {
                                let mut state = self.inner.lock().map_err(|e| e.to_string())?;
                                state.status.phase = Phase::Downloading;
                                state.status.message =
                                    Some("Preparing verified local AI runtime…".into());
                            }
                            runtime =
                                Some(runtime::prepare(&self.engine.store.root, cancel, || {
                                    self.check(cancel, revision)
                                })?);
                            self.inner.lock().map_err(|e| e.to_string())?.status.phase =
                                Phase::Processing;
                        }
                        runtime::process(
                            runtime.as_ref().ok_or("AI runtime is unavailable.")?,
                            &original,
                            &directory,
                            cancel,
                            || self.check(cancel, revision),
                        )?
                    } else {
                        fast_process(&bytes)?
                    };
                    self.check(cancel, revision)?;
                    if bytes.len() as u64 > MAX_OUTPUT {
                        return Err("Processed image exceeds the 64 MB limit.".into());
                    }
                    // Evict before the write so the persistent cache stays bounded.
                    prune_results(&cache, CACHE_BUDGET.saturating_sub(bytes.len() as u64))?;
                    storage::atomic_write(&cached, &bytes)?;
                    bytes
                }
            };
            self.check(cancel, revision)?;
            let result = decode(&output)?;
            let result_preview = jpeg(&result.thumbnail(1200, 1200))?;
            disk_used += (output.len() + original_preview.len() + result_preview.len()) as u64;
            if disk_used > JOB_BUDGET {
                return Err(
                    "Processing results exceed the 768 MB job disk limit. Select fewer images."
                        .into(),
                );
            }
            let path = directory.join(format!("{index}.jpg"));
            let before = directory.join(format!("{index}-before.jpg"));
            let after = directory.join(format!("{index}-after.jpg"));
            storage::atomic_write(&path, &output)?;
            storage::atomic_write(&before, &original_preview)?;
            storage::atomic_write(&after, &result_preview)?;
            self.check(cancel, revision)?;
            let mut state = self.inner.lock().map_err(|e| e.to_string())?;
            self.check(cancel, revision)?;
            state.outputs.push(path);
            state.status.results.push(UpscaleResult {
                id: media.id,
                name: media.name,
                width: result.width(),
                height: result.height(),
                original_preview: before.to_string_lossy().into_owned(),
                result_preview: after.to_string_lossy().into_owned(),
            });
            state.status.completed = index + 1;
        }
        self.check(cancel, revision)
    }
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
    let mut hash = Sha256::new();
    hash.update(ALGORITHM_VERSION);
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
