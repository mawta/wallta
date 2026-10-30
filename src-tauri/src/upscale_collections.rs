//! Durable outputs live outside all reusable caches and are addressed by owned IDs.
use super::{cleanup, read_bounded, storage};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInfo {
    pub id: String,
    pub name: String,
    pub model: Option<String>,
    pub created_at: Option<String>,
    pub count: usize,
    pub bytes: u64,
    pub source_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub cache_bytes: u64,
    pub collections: Vec<CollectionInfo>,
}

pub(super) fn write_manifest(path: &Path, info: &CollectionInfo) -> Result<(), String> {
    storage::atomic_write(
        &path.join("collection.json"),
        &serde_json::to_vec(info).map_err(|e| e.to_string())?,
    )
}

pub(super) fn catalog(
    root: &Path,
    cache: &Path,
    sources: &[crate::models::Source],
) -> Result<StorageInfo, String> {
    let mut collections = Vec::new();
    if let Some(base) = cleanup::managed_base(root, "upscaled") {
        for entry in fs::read_dir(&base).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if !cleanup::owned_job(&id) {
                continue;
            }
            let Ok(path) = cleanup::collection_path(root, &id) else {
                continue;
            };
            let mut info = read_bounded(&path.join("collection.json"), 16 * 1024)
                .and_then(|bytes| {
                    serde_json::from_slice::<CollectionInfo>(&bytes).map_err(|e| e.to_string())
                })
                .unwrap_or(CollectionInfo {
                    id: id.clone(),
                    name: "Ảnh 4K · legacy".into(),
                    model: None,
                    created_at: None,
                    count: 0,
                    bytes: 0,
                    source_id: None,
                });
            // The folder name, native file sizes and fresh state are authoritative.
            info.id = id;
            info.count = 0;
            info.bytes = 0;
            for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
                if metadata.is_file() && entry.path().extension().is_some_and(|ext| ext == "jpg") {
                    info.count += 1;
                    info.bytes += metadata.len();
                }
            }
            info.source_id = sources
                .iter()
                .find(|source| {
                    source.kind == "local"
                        && source.effective_folders().iter().any(|location| {
                            Path::new(location)
                                .canonicalize()
                                .is_ok_and(|folder| folder == path)
                        })
                })
                .map(|source| source.id.clone());
            collections.push(info);
        }
    }
    collections.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));
    Ok(StorageInfo {
        cache_bytes: cache_size(cache)?,
        collections,
    })
}

fn cache_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    if !cleanup::plain_directory(path) {
        return Err("Cache contains an unsafe link.".into());
    }
    let mut size: u64 = 0;
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if cleanup::plain_directory(&entry.path()) {
                size = size.saturating_add(cache_size(&entry.path())?);
            }
        } else if metadata.is_file() {
            size = size.saturating_add(metadata.len());
        }
    }
    Ok(size)
}

/// Reserve room for one bounded image and temporary model files, not an aggregate job cap.
#[cfg(windows)]
pub(super) fn ensure_free_space(path: &Path, needed: u64) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            path: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    if unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(format!(
            "Cannot check free disk space: {}",
            std::io::Error::last_os_error()
        ));
    }
    if available < needed {
        return Err("Not enough free disk space for the next image. Saved images are kept.".into());
    }
    Ok(())
}
#[cfg(not(windows))]
pub(super) fn ensure_free_space(_path: &Path, _needed: u64) -> Result<(), String> {
    Ok(())
}
