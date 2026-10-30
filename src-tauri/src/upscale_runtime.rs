//! Pinned local Real-ESRGAN runtime; no image is sent over the network.
use super::{decode_limited, jpeg, policy, read_bounded, MAX_OUTPUT};
use image::{imageops::FilterType, DynamicImage};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[cfg(windows)]
#[path = "upscale_child.rs"]
mod child;
#[path = "upscale_models.rs"]
mod models;

const ASSET_BASE: &str = "https://github.com/mawta/wallta/releases/download/ai-models-v1";
// Retained as the provenance of the exact official files pinned in models.
#[cfg(test)]
pub(super) const ARCHIVE_SHA256: &str =
    "abc02804e17982a3be33675e4d471e91ea374e65b70167abc09e31acb412802d";
const INSTALL_DIRECTORY: &str = "realesrgan-20220424-x4plus";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub bytes: u64,
    pub installed: bool,
    pub download_bytes: u64,
    pub runtime_bytes: u64,
    pub runtime_installed: bool,
}

pub(super) fn parse_model(id: Option<&str>) -> Result<&'static str, String> {
    Ok(models::selected(id)?.id)
}

pub(super) fn verify_hash(bytes: &[u8], expected: &str) -> Result<(), String> {
    if format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err(
            "AI runtime integrity check failed. Retry to repair the selected files.".into(),
        );
    }
    Ok(())
}

fn plain_path(path: &Path, directory: bool) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !metadata.file_type().is_symlink()
        && if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        }
}

fn runtime_directory(root: &Path, create: bool) -> Result<PathBuf, String> {
    if !plain_path(root, true) {
        return Err("Managed AI storage is unavailable or contains a link.".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut directory = root.clone();
    for component in ["ai-runtime", INSTALL_DIRECTORY, "models"] {
        let next = directory.join(component);
        if create
            && fs::symlink_metadata(&next).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            fs::create_dir(&next).map_err(|e| e.to_string())?;
        }
        if !plain_path(&next, true) {
            return Err("Managed AI storage is unavailable or contains a link.".into());
        }
        let canonical = next.canonicalize().map_err(|e| e.to_string())?;
        if canonical.parent() != Some(directory.as_path()) || !canonical.starts_with(&root) {
            return Err("AI storage escaped its managed directory.".into());
        }
        directory = canonical;
    }
    Ok(directory
        .parent()
        .ok_or("Invalid AI storage directory.")?
        .to_path_buf())
}

fn asset_installed(directory: &Path, asset: &models::Asset) -> bool {
    if !plain_path(directory, true) || !plain_path(&directory.join("models"), true) {
        return false;
    }
    let file = directory.join(asset.path);
    if !plain_path(&file, false) || !fs::metadata(&file).is_ok_and(|m| m.len() == asset.bytes) {
        return false;
    }
    read_bounded(&file, asset.bytes)
        .and_then(|bytes| verify_hash(&bytes, asset.sha256))
        .is_ok()
}

fn verify_install(directory: &Path, model: &models::Model) -> Result<(), String> {
    if models::RUNTIME
        .iter()
        .chain(model.files)
        .any(|asset| !asset_installed(directory, asset))
    {
        return Err(
            "AI runtime or selected model is missing or failed its integrity check.".into(),
        );
    }
    Ok(())
}

/// Reading metadata never creates directories, downloads, or trusts file presence alone.
pub(super) fn catalog(root: &Path) -> Vec<ModelInfo> {
    let directory = runtime_directory(root, false).ok();
    let installed = |asset: &models::Asset| {
        directory
            .as_ref()
            .is_some_and(|d| asset_installed(d, asset))
    };
    let runtime_bytes = models::RUNTIME.iter().map(|a| a.bytes).sum();
    let runtime_installed = models::RUNTIME.iter().all(&installed);
    models::MODELS
        .iter()
        .map(|model| ModelInfo {
            id: model.id.into(),
            name: model.name.into(),
            description: model.description.into(),
            bytes: model.files.iter().map(|a| a.bytes).sum(),
            installed: model.files.iter().all(&installed),
            download_bytes: model
                .files
                .iter()
                .filter(|a| !installed(a))
                .map(|a| a.bytes)
                .sum(),
            runtime_bytes,
            runtime_installed,
        })
        .collect()
}

/// Reuses the verified 0.1.5/0.1.6 installation in place, adds only missing files,
/// and never fetches the old archive or any unselected model pair.
pub(super) fn prepare(
    root: &Path,
    model: &str,
    cancel: &AtomicBool,
    check: impl Fn() -> Result<(), String>,
    progress: impl Fn(u64, u64),
) -> Result<PathBuf, String> {
    let model = models::selected(Some(model))?;
    let check = || {
        if cancel.load(Ordering::SeqCst) {
            return Err("Processing cancelled.".into());
        }
        check()
    };
    check()?;
    let directory = runtime_directory(root, true)?;
    cleanup_staging(&directory)?;
    let missing = models::missing_assets(model, |a| asset_installed(&directory, a));
    let total = missing.iter().map(|a| a.bytes).sum();
    let mut received = 0;
    progress(0, total);
    for asset in missing {
        check()?;
        super::collections::ensure_free_space(&directory, asset.bytes + 8 * 1024 * 1024)?;
        let bytes = download(asset, cancel, &check, &|bytes| {
            progress(received + bytes, total)
        })?;
        check()?;
        install_asset(&directory, asset, &bytes, &check)?;
        received += asset.bytes;
    }
    check()?;
    verify_install(&directory, model)?;
    write_license(
        &directory,
        "LICENSE-runtime.txt",
        include_bytes!("upscale_runtime_LICENSE.txt"),
        &check,
    )?;
    write_license(
        &directory,
        "LICENSE-model.txt",
        include_bytes!("upscale_model_LICENSE.txt"),
        &check,
    )?;
    Ok(directory)
}

fn cleanup_staging(directory: &Path) -> Result<(), String> {
    for parent in [directory.to_path_buf(), directory.join("models")] {
        if !plain_path(&parent, true) {
            return Err("Unsafe AI staging directory.".into());
        }
        for entry in fs::read_dir(&parent).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let owned = name
                .to_str()
                .and_then(|name| name.strip_prefix(".wallta-ai-"))
                .and_then(|name| name.strip_suffix(".pending"))
                .is_some_and(|id| {
                    id.len() == 32
                        && id
                            .bytes()
                            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                });
            if owned && plain_path(&entry.path(), false) {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn write_license(
    directory: &Path,
    name: &str,
    bytes: &[u8],
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    publish_bytes(directory, name, bytes, check)
}

fn install_asset(
    directory: &Path,
    asset: &models::Asset,
    bytes: &[u8],
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    if bytes.len() as u64 != asset.bytes {
        return Err("AI asset has an unexpected byte length.".into());
    }
    verify_hash(bytes, asset.sha256)?;
    publish_bytes(directory, asset.path, bytes, check)?;
    if !asset_installed(directory, asset) {
        return Err("AI asset failed verification after publication.".into());
    }
    Ok(())
}

/// A unique create-new sibling prevents following an existing .tmp link. Only a
/// completed verified file is atomically renamed over the old asset.
fn publish_bytes(
    directory: &Path,
    name: &str,
    bytes: &[u8],
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    check()?;
    policy::safe_archive_name(name)?;
    let target = directory.join(name);
    let parent = target.parent().ok_or("Invalid AI asset directory.")?;
    if !plain_path(directory, true)
        || !plain_path(parent, true)
        || !parent
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(directory.canonicalize().map_err(|e| e.to_string())?)
        || fs::symlink_metadata(&target).is_ok_and(|_| !plain_path(&target, false))
    {
        return Err("Unsafe AI asset path.".into());
    }
    let staging = parent.join(format!(
        ".wallta-ai-{:032x}.pending",
        rand::random::<u128>()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        check()?;
        if !plain_path(parent, true) {
            return Err("Unsafe AI asset directory.".into());
        }
        atomic_rename(&staging, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

#[cfg(windows)]
fn atomic_rename(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
#[cfg(not(windows))]
fn atomic_rename(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn download(
    _asset: &models::Asset,
    _cancel: &AtomicBool,
    _check: &impl Fn() -> Result<(), String>,
    _progress: &impl Fn(u64),
) -> Result<Vec<u8>, String> {
    Err("AI processing requires Windows.".into())
}

#[cfg(windows)]
fn download(
    asset: &models::Asset,
    cancel: &AtomicBool,
    check: &impl Fn() -> Result<(), String>,
    progress: &impl Fn(u64),
) -> Result<Vec<u8>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let url = attempt.url();
                if attempt.previous().len() > 5
                    || url.scheme() != "https"
                    || !matches!(
                        url.host_str(),
                        Some(
                            "github.com"
                                | "release-assets.githubusercontent.com"
                                | "objects.githubusercontent.com"
                        )
                    )
                {
                    attempt.error("Unexpected AI asset download redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|e| e.to_string())?;
        let stopped = async {
            loop {
                check()?;
                if cancel.load(Ordering::SeqCst) {
                    return Err::<Vec<u8>, String>("Processing cancelled.".into());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        let receive = async {
            let name = asset
                .path
                .rsplit('/')
                .next()
                .ok_or("Invalid pinned AI asset.")?;
            let mut response = client
                .get(format!("{ASSET_BASE}/{name}"))
                .header("User-Agent", "Wallta")
                .send()
                .await
                .map_err(|e| format!("Cannot download AI asset: {e}"))?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            if response
                .content_length()
                .is_some_and(|size| size != asset.bytes)
            {
                return Err("AI asset has an unexpected byte length.".into());
            }
            let mut bytes = Vec::with_capacity(asset.bytes as usize);
            while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
                check()?;
                if bytes.len() as u64 + chunk.len() as u64 > asset.bytes {
                    return Err("AI asset exceeds its byte limit.".into());
                }
                bytes.extend_from_slice(&chunk);
                progress(bytes.len() as u64);
            }
            if bytes.len() as u64 != asset.bytes {
                return Err("AI asset download was incomplete.".into());
            }
            verify_hash(&bytes, asset.sha256)?;
            Ok(bytes)
        };
        tokio::select! { result = receive => result, result = stopped => result }
    })
}

#[cfg(not(windows))]
pub(super) fn process(
    _runtime: &Path,
    _model: &str,
    _original: &DynamicImage,
    _directory: &Path,
    _cancel: &AtomicBool,
    _check: impl Fn() -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    Err("AI processing requires Windows.".into())
}

#[cfg(windows)]
pub(super) fn process(
    runtime: &Path,
    model: &str,
    original: &DynamicImage,
    directory: &Path,
    cancel: &AtomicBool,
    check: impl Fn() -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    check()?;
    let model = models::selected(Some(model))?;
    verify_install(runtime, model)?;
    let input = directory.join("ai-input.png");
    let output = directory.join("ai-output.png");
    let result = (|| {
        let (w, h) = policy::ai_input_dimensions(original.width(), original.height())?;
        let image = original.resize_exact(w, h, FilterType::Lanczos3).to_rgb8();
        let mut file = fs::File::create(&input).map_err(|e| e.to_string())?;
        DynamicImage::ImageRgb8(image)
            .write_to(&mut file, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        drop(file);
        if output.exists() {
            fs::remove_file(&output).map_err(|e| e.to_string())?;
        }
        check()?;
        use std::ffi::OsString;
        let args = vec![
            OsString::from("-i"),
            input.clone().into_os_string(),
            OsString::from("-o"),
            output.clone().into_os_string(),
            OsString::from("-m"),
            runtime.join("models").into_os_string(),
            OsString::from("-n"),
            OsString::from(model.runtime_name),
            OsString::from("-s"),
            OsString::from("4"),
            OsString::from("-t"),
            OsString::from("128"),
            OsString::from("-j"),
            OsString::from("1:1:1"),
            OsString::from("-f"),
            OsString::from("png"),
        ];
        let child =
            child::JobChild::spawn(&runtime.join("realesrgan-ncnn-vulkan.exe"), &args, runtime)?;
        let started = Instant::now();
        loop {
            check()?;
            if cancel.load(Ordering::SeqCst) {
                return Err("Processing cancelled.".into());
            }
            if started.elapsed() > Duration::from_secs(10 * 60) {
                return Err("AI processing exceeded the 10 minute image limit.".into());
            }
            if fs::metadata(&output).is_ok_and(|meta| meta.len() > MAX_OUTPUT) {
                return Err("AI result exceeds its size limit.".into());
            }
            if let Some(success) = child.try_wait()? {
                if !success {
                    return Err("AI processing failed. A working Vulkan GPU driver is required. Try Fast mode.".into());
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        check()?;
        // NCNN creates a 4× image (up to 7680×4320); only this bounded
        // intermediate gets the 40 MP limit. Originals stay at 32 MP.
        let image = decode_limited(&read_bounded(&output, MAX_OUTPUT)?, 40_000_000)?;
        if image.width() != w * 4 || image.height() != h * 4 {
            return Err("AI runtime returned unexpected dimensions.".into());
        }
        let (target_w, target_h) = policy::target_dimensions(original.width(), original.height())?;
        jpeg(&image.resize_exact(target_w, target_h, FilterType::Lanczos3))
    })();
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&output);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> models::Asset {
        models::Asset {
            path: "models/test.bin",
            bytes: 7,
            sha256: "239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5",
        }
    }

    #[test]
    fn metadata_lists_three_models_without_creating_or_downloading_files() {
        let temp = tempfile::tempdir().unwrap();
        let before = fs::read_dir(temp.path()).unwrap().count();
        let entries = catalog(temp.path());
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].id, "photo");
        assert_eq!(entries[1].bytes, 8_973_790);
        assert_eq!(entries[2].download_bytes, 1_250_445);
        assert!(entries
            .iter()
            .all(|entry| !entry.installed && !entry.runtime_installed));
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), before);
    }

    #[test]
    fn published_asset_is_exact_length_and_hash_verified() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("models")).unwrap();
        let asset = fixture();
        install_asset(temp.path(), &asset, b"payload", &|| Ok(())).unwrap();
        assert!(asset_installed(temp.path(), &asset));
        fs::write(temp.path().join(asset.path), b"changed").unwrap();
        assert!(!asset_installed(temp.path(), &asset));
        assert!(install_asset(temp.path(), &asset, b"corrupt", &|| Ok(())).is_err());
        assert_eq!(fs::read(temp.path().join(asset.path)).unwrap(), b"changed");
        assert!(install_asset(temp.path(), &asset, b"payload-extra", &|| Ok(())).is_err());
    }

    #[test]
    fn cancellation_before_publication_keeps_existing_files_and_removes_staging() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("models")).unwrap();
        let asset = fixture();
        fs::write(temp.path().join(asset.path), b"old-file").unwrap();
        let checks = std::cell::Cell::new(0);
        assert!(install_asset(temp.path(), &asset, b"payload", &|| {
            checks.set(checks.get() + 1);
            if checks.get() > 1 {
                Err("Processing cancelled.".into())
            } else {
                Ok(())
            }
        })
        .is_err());
        assert_eq!(
            checks.get(),
            2,
            "Cancellation must be checked after staging too."
        );
        assert_eq!(fs::read(temp.path().join(asset.path)).unwrap(), b"old-file");
        assert_eq!(fs::read_dir(temp.path().join("models")).unwrap().count(), 1);
    }

    #[test]
    fn interrupted_download_staging_is_reclaimed_without_removing_other_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("models")).unwrap();
        let owned = temp
            .path()
            .join("models/.wallta-ai-0123456789abcdef0123456789abcdef.pending");
        let arbitrary = temp.path().join("models/my-file.pending");
        fs::write(&owned, b"incomplete").unwrap();
        fs::write(&arbitrary, b"keep").unwrap();
        cleanup_staging(temp.path()).unwrap();
        assert!(!owned.exists());
        assert_eq!(fs::read(arbitrary).unwrap(), b"keep");
    }

    #[test]
    fn existing_runtime_must_verify_every_executable_dll_and_selected_model() {
        let temp = tempfile::tempdir().unwrap();
        let model = models::selected(Some("photo")).unwrap();
        assert!(verify_install(temp.path(), model).is_err());
        for asset in models::RUNTIME.iter().chain(model.files) {
            let path = temp.path().join(asset.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"tampered").unwrap();
        }
        assert!(verify_install(temp.path(), model).is_err());
        assert!(verify_install(temp.path(), models::selected(Some("anime")).unwrap()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn runtime_directory_cannot_escape_through_a_junction_or_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        // Developer Mode/administrator permission is not always available in CI.
        if std::os::windows::fs::symlink_dir(outside.path(), temp.path().join("ai-runtime"))
            .is_err()
        {
            return;
        }
        assert!(runtime_directory(temp.path(), true).is_err());
        assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
    }
}
