//! Cleanup only names owned by the upscale pipeline, under canonical managed roots.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn owned_job(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn plain_directory(path: &Path) -> bool {
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
    metadata.is_dir() && !metadata.file_type().is_symlink()
}

pub(super) fn managed_base(root: &Path, relative: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let base = root.join(relative);
    if !plain_directory(&base) {
        return None;
    }
    let canonical = base.canonicalize().ok()?;
    canonical.starts_with(&root).then_some(canonical)
}

pub(super) fn canonical_with_missing_tail(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match ancestor.canonicalize() {
            Ok(mut canonical) => {
                for name in missing.iter().rev() {
                    canonical.push(name);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| error.to_string())?
                        .to_os_string(),
                );
                if !ancestor.pop() {
                    return Err(error.to_string());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

pub(crate) fn remove_unreferenced(
    root: &Path,
    path: &Path,
    references: &[PathBuf],
) -> Result<(), String> {
    let Some(base) = managed_base(root, "upscaled") else {
        return Ok(());
    };
    if !plain_directory(path) {
        return Ok(());
    }
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if canonical.parent() != Some(base.as_path()) {
        return Ok(());
    }
    let Some(name) = canonical.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let pending = name
        .strip_prefix('.')
        .and_then(|name| name.strip_suffix(".pending"));
    if !owned_job(name) && !pending.is_some_and(owned_job) {
        return Ok(());
    }
    if references
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .any(|path| canonical.starts_with(&path) || path.starts_with(&canonical))
    {
        return Ok(());
    }
    fs::remove_dir_all(&canonical).map_err(|e| e.to_string())
}

pub(crate) fn startup(
    cache: &Path,
    root: &Path,
    references: Option<&[PathBuf]>,
) -> Result<(), String> {
    if let Some(base) = managed_base(cache, "upscale/jobs") {
        for entry in fs::read_dir(&base).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_name().to_str().is_some_and(owned_job) || !plain_directory(&entry.path())
            {
                continue;
            }
            let path = entry.path().canonicalize().map_err(|e| e.to_string())?;
            if path.parent() == Some(base.as_path()) {
                fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
            }
        }
    }
    if let (Some(base), Some(references)) = (managed_base(root, "upscaled"), references) {
        for entry in fs::read_dir(base).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            // Completed hex folders, including legacy versions without a manifest,
            // remain user-owned after their source is removed. Storage can list and
            // explicitly delete them. Only unpublished .pending copies are reclaimed.
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(owned_job)
            {
                continue;
            }
            remove_unreferenced(root, &path, references)?;
        }
    }
    Ok(())
}

/// Return only a direct, non-reparse owned folder. Never accept caller paths.
pub(super) fn collection_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !owned_job(id) {
        return Err("Invalid managed collection ID.".into());
    }
    let base = managed_base(root, "upscaled").ok_or("Managed collections are unavailable.")?;
    let path = base.join(id);
    if !plain_directory(&path) {
        return Err("Collection contains an unsafe link.".into());
    }
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if canonical.parent() != Some(base.as_path()) {
        return Err("Collection escapes managed storage.".into());
    }
    validate_tree(&canonical)?;
    Ok(canonical)
}

fn validate_tree(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("Collection contains a reparse point.".into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("Collection contains a symbolic link.".into());
        }
        if metadata.is_dir() {
            validate_tree(&entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_owned_folder_retains_its_canonical_existing_parent() {
        let temp =
            std::env::temp_dir().join(format!("wallta-missing-owned-{}", std::process::id()));
        let base = temp.join("upscaled");
        fs::create_dir_all(&base).unwrap();
        let missing = base.join("0123456789abcdef0123456789abcdef");
        let normalized = canonical_with_missing_tail(&missing).unwrap();
        assert!(
            normalized.starts_with(base.canonicalize().unwrap()),
            "Missing managed paths lost canonical containment: {normalized:?}"
        );
        assert!(!missing.exists());
        fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn startup_preserves_legacy_owned_collections_without_source_references() {
        let temp = std::env::temp_dir().join(format!("wallta-legacy-owned-{}", std::process::id()));
        let root = temp.join("root");
        let collection = root.join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&collection).unwrap();
        fs::write(collection.join("001.jpg"), b"legacy durable result").unwrap();
        startup(&temp.join("cache"), &root, Some(&[])).unwrap();
        assert!(
            collection.join("001.jpg").is_file(),
            "Legacy saved files must remain discoverable in storage"
        );
        fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn collection_ids_cannot_escape_or_delete_arbitrary_folders() {
        let temp = std::env::temp_dir().join(format!("wallta-owned-id-{}", std::process::id()));
        let root = temp.join("root");
        let owned = "0123456789abcdef0123456789abcdef";
        fs::create_dir_all(root.join("upscaled").join(owned)).unwrap();
        fs::create_dir_all(root.join("upscaled/holiday")).unwrap();
        for id in [
            "..",
            "../holiday",
            "holiday",
            "C:\\outside",
            "",
            "0123456789ABCDEF0123456789ABCDEF",
        ] {
            assert!(collection_path(&root, id).is_err(), "{id}");
        }
        assert!(collection_path(&root, owned).is_ok());
        fs::remove_dir_all(&temp).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn collection_rejects_directory_junctions_and_nested_reparse_points() {
        use std::os::windows::process::CommandExt;
        let temp = std::env::temp_dir().join(format!("wallta-owned-link-{}", std::process::id()));
        let root = temp.join("root");
        let outside = temp.join("outside");
        let owned = "0123456789abcdef0123456789abcdef";
        let folder = root.join("upscaled").join(owned);
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.jpg"), b"external").unwrap();
        let link = folder.join("escape");
        let output = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Could not create junction: {:?}",
            output
        );
        assert!(collection_path(&root, owned).is_err());
        fs::remove_dir(&link).unwrap();
        assert!(outside.join("keep.jpg").is_file());
        fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn startup_retains_durable_manifest_collections_after_source_removal() {
        let temp = std::env::temp_dir().join(format!("wallta-durable-{}", std::process::id()));
        let root = temp.join("root");
        let collection = root.join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&collection).unwrap();
        fs::write(collection.join("collection.json"), b"{}").unwrap();
        fs::write(collection.join("001.jpg"), b"durable output").unwrap();
        startup(&temp.join("cache"), &root, Some(&[])).unwrap();
        assert!(
            collection.join("001.jpg").is_file(),
            "Saved collections must outlive source removal"
        );
        fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn startup_removes_temporary_jobs_and_pending_copies_but_preserves_collections() {
        let temp =
            std::env::temp_dir().join(format!("wallta-upscale-cleanup-{}", std::process::id()));
        fs::create_dir_all(&temp).unwrap();
        let cache = temp.join("cache");
        let root = temp.join("root");
        let jobs = cache.join("upscale/jobs");
        let outputs = root.join("upscaled");
        let owned = "0123456789abcdef0123456789abcdef";
        let kept = "fedcba9876543210fedcba9876543210";
        for path in [
            jobs.join(owned),
            jobs.join("user-folder"),
            outputs.join(owned),
            outputs.join(kept),
            outputs.join(format!(".{owned}.pending")),
            outputs.join("holiday"),
        ] {
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("image.jpg"), b"image").unwrap();
        }
        let protected = outputs.join(kept).canonicalize().unwrap();
        startup(&cache, &root, Some(&[protected])).unwrap();
        assert!(!jobs.join(owned).exists());
        assert!(outputs.join(owned).exists());
        assert!(!outputs.join(format!(".{owned}.pending")).exists());
        assert!(jobs.join("user-folder/image.jpg").exists());
        assert!(outputs.join(kept).join("image.jpg").exists());
        assert!(outputs.join("holiday/image.jpg").exists());
        fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn unreadable_saved_configuration_preserves_all_durable_outputs() {
        let temp =
            std::env::temp_dir().join(format!("wallta-upscale-unreadable-{}", std::process::id()));
        let root = temp.join("root");
        let path = root.join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&path).unwrap();
        startup(&temp.join("cache"), &root, None).unwrap();
        assert!(path.is_dir());
        fs::remove_dir_all(&temp).unwrap();
    }
}
