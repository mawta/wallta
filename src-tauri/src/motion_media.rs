#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaFormat {
    Image,
    Gif,
    Mp4,
    Webm,
}

impl MediaFormat {
    pub fn is_motion(self) -> bool {
        self != Self::Image
    }
    pub fn is_video(self) -> bool {
        matches!(self, Self::Mp4 | Self::Webm)
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Image => "img",
            Self::Gif => "gif",
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }
}

pub fn detect_format(bytes: &[u8]) -> Result<MediaFormat, String> {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Ok(MediaFormat::Gif);
    }
    if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && [
            b"isom", b"iso2", b"mp41", b"mp42", b"avc1", b"M4V ", b"dash", b"MSNV",
        ]
        .iter()
        .any(|brand| bytes[8..12] == **brand)
    {
        return Ok(MediaFormat::Mp4);
    }
    if bytes.starts_with(b"\x1a\x45\xdf\xa3")
        && bytes[..bytes.len().min(4096)]
            .windows(4)
            .any(|v| v == b"webm")
    {
        return Ok(MediaFormat::Webm);
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"BM")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
    {
        return Ok(MediaFormat::Image);
    }
    Err("Unsupported media. Choose JPEG, PNG, WebP, BMP, GIF, MP4 or WebM.".into())
}

/// Read only bounded bytes, including when a remote server omits Content-Length.
pub fn read_bounded(mut reader: impl std::io::Read) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(4096)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read media.".to_string())?;
    let video = detect_format(&bytes).is_ok_and(MediaFormat::is_video);
    let limit = if video {
        MAX_VIDEO_BYTES
    } else {
        MAX_STILL_BYTES
    };
    let remaining = limit.saturating_add(1).saturating_sub(bytes.len() as u64);
    reader
        .take(remaining)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read media.".to_string())?;
    if bytes.len() as u64 > limit {
        return Err(if video {
            "Video exceeds the 128 MB limit."
        } else {
            "Image exceeds the 32 MB limit."
        }
        .into());
    }
    Ok(bytes)
}

pub const MAX_VIDEO_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_STILL_BYTES: u64 = 32 * 1024 * 1024;

/// Content-addressed files stay inside the native cache, never arbitrary caller paths.
pub fn stage(bytes: &[u8], directory: &std::path::Path) -> Result<std::path::PathBuf, String> {
    use sha2::{Digest, Sha256};
    let format = detect_format(bytes)?;
    let limit = if format.is_video() {
        MAX_VIDEO_BYTES
    } else {
        MAX_STILL_BYTES
    };
    if bytes.len() as u64 > limit {
        return Err("Media exceeds its size limit.".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!(
        "motion-{:x}.{}",
        Sha256::digest(bytes),
        format.extension()
    ));
    if !path.is_file() {
        crate::storage::atomic_write(&path, bytes)?;
    }
    Ok(path)
}

/// A first frame for previews and Windows Lock, which accepts still images only.
pub fn poster_bytes(bytes: &[u8], directory: &std::path::Path) -> Result<Vec<u8>, String> {
    crate::wallpaper::thumbnail(&wallpaper_bytes(bytes, directory)?)
}

/// Keep original photo/GIF resolution for native rendering. Preview callers use
/// poster_bytes, while Windows wallpaper/Lock render from this full-sized frame.
pub fn wallpaper_bytes(bytes: &[u8], directory: &std::path::Path) -> Result<Vec<u8>, String> {
    let format = detect_format(bytes)?;
    if !format.is_video() {
        if bytes.len() as u64 > MAX_STILL_BYTES {
            return Err("Image exceeds the 32 MB limit.".into());
        }
        return Ok(bytes.to_vec());
    }
    let path = stage(bytes, directory)?;
    #[cfg(windows)]
    {
        video_poster(&path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Video poster extraction is available on Windows.".into())
    }
}

#[cfg(windows)]
fn video_poster(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use windows::{
        Foundation::TimeSpan,
        Media::Editing::{MediaClip, MediaComposition, VideoFramePrecision},
        Storage::{StorageFile, Streams::DataReader},
    };
    // Rotation runs on native worker threads. WinRT requires an apartment on
    // each caller, even when WebView2 has initialized COM on the UI thread.
    #[link(name = "combase")]
    extern "system" {
        fn RoInitialize(kind: u32) -> i32;
        fn RoUninitialize();
    }
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { RoUninitialize() };
            }
        }
    }
    let initialized = unsafe { RoInitialize(1) };
    if initialized < 0 && initialized != 0x80010106u32 as i32 {
        return Err("Windows could not initialize its video decoder.".into());
    }
    let _apartment = Apartment(initialized >= 0);
    let extract = || -> windows::core::Result<Vec<u8>> {
        let file =
            StorageFile::GetFileFromPathAsync(&windows::core::HSTRING::from(path.as_os_str()))?
                .join()?;
        let clip = MediaClip::CreateFromFileAsync(&file)?.join()?;
        let properties = clip.GetVideoEncodingProperties()?;
        let (width, height) = (properties.Width()?, properties.Height()?);
        if width == 0 || height == 0 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80070057u32 as i32,
            )));
        }
        let scale = 3840.0 / f64::from(width.max(height));
        let thumb_width = (f64::from(width) * scale.min(1.0)).round().max(1.0) as i32;
        let thumb_height = (f64::from(height) * scale.min(1.0)).round().max(1.0) as i32;
        let composition = MediaComposition::new()?;
        composition.Clips()?.Append(&clip)?;
        let thumbnail = composition
            .GetThumbnailAsync(
                TimeSpan { Duration: 0 },
                thumb_width,
                thumb_height,
                VideoFramePrecision::NearestFrame,
            )?
            .join()?;
        let size = thumbnail.Size()?;
        if size == 0 || size > MAX_STILL_BYTES {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80070057u32 as i32,
            )));
        }
        let reader = DataReader::CreateDataReader(&thumbnail.GetInputStreamAt(0)?)?;
        if reader.LoadAsync(size as u32)?.join()? != size as u32 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x80004005u32 as i32,
            )));
        }
        let mut bytes = vec![0; size as usize];
        reader.ReadBytes(&mut bytes)?;
        Ok(bytes)
    };
    let encoded = extract().map_err(|_| {
        "Windows could not decode this video. Use an MP4/WebM codec supported by this PC."
            .to_string()
    })?;
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_content_without_trusting_file_names() {
        assert_eq!(detect_format(b"GIF89a123456"), Ok(MediaFormat::Gif));
        assert_eq!(
            detect_format(b"\0\0\0\x18ftypisom\0\0\0\0"),
            Ok(MediaFormat::Mp4)
        );
        assert_eq!(
            detect_format(b"\x1a\x45\xdf\xa3\x42\x82\x84webm"),
            Ok(MediaFormat::Webm)
        );
        assert_eq!(detect_format(b"\x89PNG\r\n\x1a\n"), Ok(MediaFormat::Image));
        assert!(detect_format(b"<html>video.mp4</html>").is_err());
        assert!(detect_format(b"\x1a\x45\xdf\xa3matroska").is_err());
    }
    #[test]
    fn read_limits_use_actual_content_including_extensionless_video() {
        let mut image = std::io::Cursor::new(vec![0u8; MAX_STILL_BYTES as usize + 1]);
        assert!(read_bounded(&mut image).unwrap_err().contains("32 MB"));
        assert_eq!(image.position(), MAX_STILL_BYTES + 1);
        let mut video = b"\0\0\0\x18ftypisom\0\0\0\0".to_vec();
        video.resize(MAX_STILL_BYTES as usize + 1, 0);
        assert_eq!(
            read_bounded(std::io::Cursor::new(video)).unwrap().len(),
            MAX_STILL_BYTES as usize + 1
        );
    }
}
