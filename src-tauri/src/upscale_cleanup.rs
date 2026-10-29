//! Cleanup only names owned by the upscale pipeline, under canonical managed roots.
use std::{
    fs,
    path::{Path, PathBuf},
};

fn owned_job(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn plain_directory(path: &Path) -> bool {
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

fn managed_base(root: &Path, relative: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let base = root.join(relative);
    if !plain_directory(&base) {
        return None;
    }
    let canonical = base.canonicalize().ok()?;
    canonical.starts_with(&root).then_some(canonical)
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
            remove_unreferenced(root, &entry.map_err(|e| e.to_string())?.path(), references)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_removes_owned_orphans_and_preserves_referenced_collections() {
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
        assert!(!outputs.join(owned).exists());
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
