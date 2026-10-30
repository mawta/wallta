use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) fn ai_input_dimensions(width: u32, height: u32) -> Result<(u32, u32), String> {
    let (w, h) = target_dimensions(width, height)?;
    if width <= w / 2 && height <= h / 2 {
        return Ok((width, height));
    }
    Ok(((w / 2).max(1), (h / 2).max(1)))
}

pub(crate) fn needs_ai(width: u32, height: u32) -> Result<bool, String> {
    let (target_w, target_h) = target_dimensions(width, height)?;
    Ok(width < target_w || height < target_h)
}

pub(crate) fn target_dimensions(width: u32, height: u32) -> Result<(u32, u32), String> {
    if width == 0 || height == 0 {
        return Err("Image dimensions must be positive.".into());
    }
    let (max_w, max_h) = if width >= height {
        (3840_u64, 2160_u64)
    } else {
        (2160, 3840)
    };
    let (w, h) = (u64::from(width), u64::from(height));
    Ok(if max_w * h <= max_h * w {
        (max_w as u32, (h * max_w / w).max(1) as u32)
    } else {
        ((w * max_h / h).max(1) as u32, max_h as u32)
    })
}

pub(crate) fn validate_ids(ids: &[String]) -> Result<(), String> {
    if ids.is_empty()
        || ids.len() > 50
        || ids.iter().any(|id| id.trim().is_empty() || id.len() > 1024)
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err("Select between 1 and 50 unique image IDs.".into());
    }
    Ok(())
}

pub(crate) fn safe_archive_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.contains(['\\', ':', '\0'])
        || name.starts_with('/')
        || name
            .trim_end_matches('/')
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("Unsafe AI runtime archive path.".into());
    }
    Ok(())
}

pub(crate) fn check_operation(
    cancel: &AtomicBool,
    expected: u64,
    current: u64,
) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("Processing cancelled.".into());
    }
    if expected != current {
        return Err("Sources changed during processing. Start a new job.".into());
    }
    Ok(())
}

pub(crate) fn collection_label(scope: &str, model: Option<&str>, when: &str) -> String {
    let mut clean = String::new();
    for character in scope
        .trim()
        .chars()
        .filter(|character| !character.is_control())
    {
        if clean.len() + character.len_utf8() > 48 {
            break;
        }
        clean.push(character);
    }
    let scope = if clean.trim().is_empty() {
        "Tất cả nguồn"
    } else {
        clean.trim()
    };
    let model = match model {
        Some("photo") => "Photo",
        Some("anime") => "Anime",
        Some("anime-light") => "Anime Light",
        _ => "Fast",
    };
    format!("Ảnh 4K · {scope} · {model} · {when}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_names_distinguish_original_source_model_and_date() {
        let when = "2026-10-30 12:34:56";
        let photo = collection_label("Du lịch", Some("photo"), when);
        let anime = collection_label("Du lịch", Some("anime"), when);
        let fast = collection_label("Gia đình", None, when);
        assert!(photo.contains("Du lịch") && photo.contains("Photo") && photo.contains(when));
        assert!(anime.contains("Anime"));
        assert!(fast.contains("Gia đình") && fast.contains("Fast"));
        assert_ne!(photo, anime);
        assert_ne!(fast, photo);
        assert!(
            collection_label("Tất cả nguồn", Some("anime-light"), when).contains("Tất cả nguồn")
        );
        let bounded = collection_label(&"\nDu lịch\t".repeat(100), Some("photo"), when);
        assert!(bounded.len() <= 128);
        assert!(!bounded.chars().any(char::is_control));
    }

    #[test]
    fn four_k_box_preserves_landscape_portrait_and_square_aspect() {
        assert_eq!(target_dimensions(1920, 1080).unwrap(), (3840, 2160));
        assert_eq!(target_dimensions(1000, 2000).unwrap(), (1920, 3840));
        assert_eq!(target_dimensions(800, 800).unwrap(), (2160, 2160));
        assert_eq!(target_dimensions(4000, 1000).unwrap(), (3840, 960));
        assert!(target_dimensions(0, 1).is_err());
    }

    #[test]
    fn image_ids_are_nonempty_unique_and_bounded() {
        assert!(validate_ids(&[]).is_err());
        assert!(validate_ids(&["a".into(), "a".into()]).is_err());
        assert!(validate_ids(&[" ".into()]).is_err());
        assert!(validate_ids(&(0..51).map(|n| n.to_string()).collect::<Vec<_>>()).is_err());
        assert!(validate_ids(&["a".into(), "b".into()]).is_ok());
    }

    #[test]
    fn extraction_rejects_windows_paths_traversal_and_links() {
        for path in [
            "../evil.exe",
            "models/../../evil",
            "C:/evil",
            "/evil",
            "models\\evil",
            "models/a:b",
            "models/./evil",
        ] {
            assert!(safe_archive_name(path).is_err(), "{path}");
        }
        assert!(safe_archive_name("models/realesrgan-x4plus.bin").is_ok());
    }

    #[test]
    fn cancellation_and_source_revision_are_rechecked() {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        assert!(check_operation(&cancel, 1, 1).is_ok());
        assert!(check_operation(&cancel, 1, 2).is_err());
        cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            check_operation(&cancel, 1, 1).unwrap_err(),
            "Processing cancelled."
        );
    }

    #[test]
    fn ai_input_preserves_full_hd_before_bounded_four_times_inference() {
        assert_eq!(ai_input_dimensions(8000, 4500).unwrap(), (1920, 1080));
        assert_eq!(ai_input_dimensions(1000, 2000).unwrap(), (960, 1920));
        assert_eq!(ai_input_dimensions(1920, 1080).unwrap(), (1920, 1080));
        assert_eq!(ai_input_dimensions(200, 100).unwrap(), (200, 100));
        assert_eq!(ai_input_dimensions(5000, 5000).unwrap(), (1080, 1080));
    }

    #[test]
    fn four_k_or_larger_originals_do_not_lose_detail_through_ai() {
        assert!(!needs_ai(3840, 2160).unwrap());
        assert!(!needs_ai(8000, 4500).unwrap());
        assert!(!needs_ai(2160, 2160).unwrap());
        assert!(!needs_ai(2160, 3840).unwrap());
        assert!(needs_ai(1920, 1080).unwrap());
        assert!(needs_ai(800, 800).unwrap());
    }
}
