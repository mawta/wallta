//! Pinned local Real-ESRGAN runtime; no image is sent over the network.
use super::{decode_limited, jpeg, policy, read_bounded, storage, MAX_OUTPUT};
use image::{imageops::FilterType, DynamicImage};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[cfg(windows)]
#[path = "upscale_child.rs"]
mod child;

const ARCHIVE_URL: &str = "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesrgan-ncnn-vulkan-20220424-windows.zip";
pub(super) const ARCHIVE_SHA256: &str =
    "abc02804e17982a3be33675e4d471e91ea374e65b70167abc09e31acb412802d";
const MAX_ARCHIVE: u64 = 100 * 1024 * 1024;
const FILES: [(&str, &str); 4] = [
    (
        "realesrgan-ncnn-vulkan.exe",
        "07e49f7cbb4ede01ae4dd4c399d3a7e5846e3d2085c3128eff881e55cb7b1a0c",
    ),
    (
        "vcomp140.dll",
        "8f72ef2e483465444b2059fc6744d6cb22cd8d8a27f6fa56befd2a42dcd0f78b",
    ),
    (
        "models/realesrgan-x4plus.bin",
        "713ee713b0353afaa27976f0563a64a5043bd70b9bd8936c2e26e25ebcdbcddf",
    ),
    (
        "models/realesrgan-x4plus.param",
        "35330ececcea33b6c397a72548e788d5d53becee4734c50b7fada36e89f10a86",
    ),
];

pub(super) fn verify_hash(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected {
        return Err("AI runtime integrity check failed. Remove the runtime and retry.".into());
    }
    Ok(())
}

fn verify_install(directory: &Path) -> Result<(), String> {
    for (name, expected) in FILES {
        let file = directory.join(name);
        if fs::symlink_metadata(&file)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("AI runtime contains a symbolic link.".into());
        }
        verify_hash(&read_bounded(&file, 40 * 1024 * 1024)?, expected)?;
    }
    Ok(())
}

pub(super) fn prepare(
    root: &Path,
    cancel: &AtomicBool,
    check: impl Fn() -> Result<(), String>,
) -> Result<PathBuf, String> {
    check()?;
    let parent = root.join("ai-runtime");
    let destination = parent.join("realesrgan-20220424-x4plus");
    if verify_install(&destination).is_ok() {
        return Ok(destination);
    }
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let staging = parent.join(".realesrgan.pending");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        let archive = download(cancel, &check)?;
        check()?;
        verify_hash(&archive, ARCHIVE_SHA256)?;
        extract(&archive, &staging, &check)?;
        verify_install(&staging)?;
        storage::atomic_write(
            &staging.join("LICENSE-runtime.txt"),
            include_bytes!("upscale_runtime_LICENSE.txt"),
        )?;
        storage::atomic_write(
            &staging.join("LICENSE-model.txt"),
            include_bytes!("upscale_model_LICENSE.txt"),
        )?;
        check()?;
        if destination.exists() {
            fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
        }
        fs::rename(&staging, &destination).map_err(|e| e.to_string())?;
        Ok(destination.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(not(windows))]
fn download(
    _cancel: &AtomicBool,
    _check: &impl Fn() -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    Err("AI processing requires Windows.".into())
}

#[cfg(windows)]
fn download(
    cancel: &AtomicBool,
    check: &impl Fn() -> Result<(), String>,
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
                    attempt.error("Unexpected AI runtime download redirect")
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
            let mut response = client
                .get(ARCHIVE_URL)
                .header("User-Agent", "Wallta")
                .send()
                .await
                .map_err(|e| format!("Cannot download AI runtime: {e}"))?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            if response
                .content_length()
                .is_some_and(|size| size > MAX_ARCHIVE)
            {
                return Err("AI runtime archive exceeds its limit.".into());
            }
            let mut archive = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
                check()?;
                if archive.len() as u64 + chunk.len() as u64 > MAX_ARCHIVE {
                    return Err("AI runtime archive exceeds its limit.".into());
                }
                archive.extend_from_slice(&chunk);
            }
            Ok(archive)
        };
        tokio::select! { result = receive => result, result = stopped => result }
    })
}

fn extract(
    bytes: &[u8],
    directory: &Path,
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if archive.len() > 100 {
        return Err("Unexpected AI runtime archive.".into());
    }
    for (name, expected) in FILES {
        check()?;
        policy::safe_archive_name(name)?;
        let mut file = archive.by_name(name).map_err(|e| e.to_string())?;
        if file.is_dir()
            || file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            || file.size() > 40 * 1024 * 1024
        {
            return Err("Unsafe AI runtime archive entry.".into());
        }
        let mut output = Vec::new();
        file.by_ref()
            .take(40 * 1024 * 1024 + 1)
            .read_to_end(&mut output)
            .map_err(|e| e.to_string())?;
        if output.len() > 40 * 1024 * 1024 {
            return Err("AI runtime entry exceeds its limit.".into());
        }
        verify_hash(&output, expected)?;
        let path = directory.join(name);
        fs::create_dir_all(path.parent().ok_or("Invalid AI runtime path.")?)
            .map_err(|e| e.to_string())?;
        storage::atomic_write(&path, &output)?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn process(
    _runtime: &Path,
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
    original: &DynamicImage,
    directory: &Path,
    cancel: &AtomicBool,
    check: impl Fn() -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    check()?;
    verify_install(runtime)?;
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
            OsString::from("realesrgan-x4plus"),
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

    #[test]
    fn existing_runtime_must_verify_every_executable_dll_and_model() {
        let temp = tempfile::tempdir().unwrap();
        assert!(verify_install(temp.path()).is_err());
        for (name, _) in FILES {
            let path = temp.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"tampered").unwrap();
        }
        assert!(verify_install(temp.path()).is_err());
    }

    #[test]
    fn invalid_archive_cannot_publish_an_executable() {
        let temp = tempfile::tempdir().unwrap();
        assert!(extract(b"not a zip", temp.path(), &|| Ok(())).is_err());
        assert!(!temp.path().join("realesrgan-ncnn-vulkan.exe").exists());
    }
}
