use crate::{models::Settings, providers::MAX_IMAGE_BYTES, storage::atomic_write};
use image::{imageops, DynamicImage, GenericImageView, ImageFormat, ImageReader, Limits};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("Image exceeds the 32 MB download limit.".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "Unknown image format.".to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(20000);
    limits.max_image_height = Some(20000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| "Image is corrupt, unsupported, or exceeds the 256 MB decode limit. The current wallpaper has been kept.".to_string())
}
pub fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let image = decode(bytes)?.thumbnail(1600, 1600);
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}
pub fn render(
    bytes: &[u8],
    settings: &Settings,
    dimensions: (u32, u32),
) -> Result<Vec<u8>, String> {
    let image = decode(bytes)?;
    let (width, height) = dimensions;
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || u64::from(width) * u64::from(height) > 40_000_000
    {
        return Err("Unsupported display dimensions.".into());
    }
    let filter = imageops::FilterType::Lanczos3;
    let canvas = match settings.fit.as_str() {
        "stretch" => image.resize_exact(width, height, filter),
        "fit" => {
            let mut background = if settings.blur_background {
                image.resize_to_fill(width, height, filter).blur(25.0)
            } else {
                DynamicImage::new_rgb8(width, height)
            };
            let foreground = image.resize(width, height, filter);
            let (fw, fh) = foreground.dimensions();
            imageops::overlay(
                &mut background,
                &foreground,
                ((width - fw) / 2) as i64,
                ((height - fh) / 2) as i64,
            );
            background
        }
        _ => image.resize_to_fill(width, height, filter),
    };
    let mut output = Cursor::new(Vec::new());
    canvas
        .to_rgb8()
        .write_to(&mut output, ImageFormat::Bmp)
        .map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}

pub fn apply(bytes: &[u8], settings: &Settings, cache: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
            fn SystemParametersInfoW(action: u32, param: u32, data: *const u16, flags: u32) -> i32;
        }
        let (width, height) = unsafe { (GetSystemMetrics(0), GetSystemMetrics(1)) };
        let bitmap = render(bytes, settings, (width.max(1) as u32, height.max(1) as u32))?;
        if bitmap.len() as u64 > settings.cache_limit_mb * 1024 * 1024 {
            return Err("The display bitmap exceeds the configured cache budget. Increase the cache limit for this screen resolution.".into());
        }
        // Alternate files ensure a failed decode/write never damages the wallpaper currently on screen.
        let path = cache.join(format!(
            "wallpaper-{}.bmp",
            chrono::Utc::now().timestamp_millis()
        ));
        atomic_write(&path, &bitmap)?;
        let desktop = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey_with_flags("Control Panel\\Desktop", winreg::enums::KEY_SET_VALUE)
            .map_err(|e| e.to_string())?;
        desktop
            .set_value("WallpaperStyle", &"10")
            .map_err(|e| e.to_string())?;
        desktop
            .set_value("TileWallpaper", &"0")
            .map_err(|e| e.to_string())?;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { SystemParametersInfoW(0x0014, 0, wide.as_ptr(), 0x1 | 0x2) } == 0 {
            let _ = std::fs::remove_file(&path);
            return Err(format!(
                "Windows could not apply the wallpaper: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(path)
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, settings, cache);
        Err("This desktop build can apply wallpapers only on Windows.".into())
    }
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{
            enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
            RegKey,
        };
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                KEY_SET_VALUE,
            )
            .map_err(|e| e.to_string())?;
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            key.set_value("Wallta", &format!("\"{}\" --background", exe.display()))
                .map_err(|e| e.to_string())
        } else {
            match key.delete_value("Wallta") {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
    #[cfg(not(windows))]
    {
        if enabled {
            Err("Autostart is available in the Windows build.".into())
        } else {
            Ok(())
        }
    }
}

pub fn wifi_connected() -> bool {
    #[cfg(windows)]
    unsafe {
        use std::ffi::c_void;
        #[repr(C)]
        struct Guid {
            a: u32,
            b: u16,
            c: u16,
            d: [u8; 8],
        }
        #[repr(C)]
        struct Interface {
            guid: Guid,
            description: [u16; 256],
            state: u32,
        }
        #[repr(C)]
        struct List {
            count: u32,
            index: u32,
            first: Interface,
        }
        #[link(name = "wlanapi")]
        extern "system" {
            fn WlanOpenHandle(
                version: u32,
                reserved: *const c_void,
                negotiated: *mut u32,
                handle: *mut *mut c_void,
            ) -> u32;
            fn WlanEnumInterfaces(
                handle: *mut c_void,
                reserved: *const c_void,
                list: *mut *mut List,
            ) -> u32;
            fn WlanFreeMemory(memory: *mut c_void);
            fn WlanCloseHandle(handle: *mut c_void, reserved: *const c_void) -> u32;
        }
        let mut version = 0;
        let mut handle = std::ptr::null_mut();
        if WlanOpenHandle(2, std::ptr::null(), &mut version, &mut handle) != 0 {
            return false;
        }
        let mut list = std::ptr::null_mut();
        let ok = if WlanEnumInterfaces(handle, std::ptr::null(), &mut list) == 0 && !list.is_null()
        {
            let interfaces = std::slice::from_raw_parts(
                std::ptr::addr_of!((*list).first),
                (*list).count.min(256) as usize,
            );
            let connected = interfaces.iter().any(|i| i.state == 1);
            WlanFreeMemory(list.cast());
            connected
        } else {
            false
        };
        WlanCloseHandle(handle, std::ptr::null());
        ok
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_images_are_rejected_before_applying() {
        assert!(render(b"not an image", &Settings::default(), (20, 20)).is_err());
    }
    #[test]
    fn fit_preserves_entire_image_and_fills_letterbox() {
        let source = image::RgbImage::from_pixel(20, 10, image::Rgb([255, 0, 0]));
        let mut encoded = Cursor::new(Vec::new());
        source.write_to(&mut encoded, ImageFormat::Png).unwrap();
        let mut settings = Settings::default();
        settings.fit = "fit".into();
        let bitmap = render(encoded.get_ref(), &settings, (20, 20)).unwrap();
        let output = image::load_from_memory(&bitmap).unwrap().to_rgb8();
        assert_eq!(*output.get_pixel(0, 0), image::Rgb([0, 0, 0]));
        assert_eq!(*output.get_pixel(10, 10), image::Rgb([255, 0, 0]));
    }
}
