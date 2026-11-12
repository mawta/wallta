//! Trusted, local-only primary-display wallpaper host. Explorer's desktop embedding
//! is an undocumented Windows interface; failure leaves the still wallpaper intact.
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
};
use tauri::Manager;

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
static PLAYBACK_FAILED: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
static WINDOW_LABEL: Mutex<Option<String>> = Mutex::new(None);
static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn current_window() -> Option<tauri::WebviewWindow> {
    let label = WINDOW_LABEL.lock().ok()?.clone()?;
    APP.get()?.get_webview_window(&label)
}

pub fn initialize(app: tauri::AppHandle) -> Result<(), String> {
    APP.set(app)
        .map_err(|_| "Motion wallpaper host already initialized.".into())
}

pub fn stop() {
    let label = WINDOW_LABEL.lock().ok().and_then(|mut label| label.take());
    if let (Some(app), Some(label)) = (APP.get(), label) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    }
    PLAYBACK_FAILED.store(false, Ordering::SeqCst);
}

pub fn set_paused(paused: bool) -> Result<(), String> {
    PAUSED.store(paused, Ordering::SeqCst);
    if let Some(window) = current_window() {
        window
            .eval(&format!("window.walltaMotionPause?.({paused})"))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Invoke from a worker, never a synchronous WebView command or main-thread callback.
pub fn start(path: &Path, fit: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use crate::motion_media::{self, MediaFormat};
        use tauri::{
            webview::{NewWindowResponse, PageLoadEvent},
            WebviewUrl, WebviewWindowBuilder,
        };
        let app = APP
            .get()
            .ok_or("Motion wallpaper host is not initialized.")?;
        let root = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("media");
        let canonical = path
            .canonicalize()
            .map_err(|_| "Cached motion media is no longer available.")?;
        let root = root
            .canonicalize()
            .map_err(|_| "Motion cache is not available.")?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            return Err(
                "Motion playback requires media staged inside the application cache.".into(),
            );
        }
        let bytes = motion_media::read_bounded(
            std::fs::File::open(&canonical).map_err(|e| e.to_string())?,
        )?;
        let format = motion_media::detect_format(&bytes)?;
        if !format.is_motion() {
            return Err("This file is a still image.".into());
        }
        let poster = motion_media::poster_bytes(&bytes, &root)?;
        let poster_path = canonical.with_extension("poster.png");
        crate::storage::atomic_write(&poster_path, &poster)?;
        let worker = unsafe { native::desktop_worker()? };
        // Retain a supported still poster if Explorer restarts or the codec fails.
        stop();
        let media = canonical.clone();
        let fit = fit.to_owned();
        let gif = format == MediaFormat::Gif;
        let label = format!("wallta-motion-{}", SEQUENCE.fetch_add(1, Ordering::Relaxed));
        let window = WebviewWindowBuilder::new(
            app,
            &label,
            WebviewUrl::App("motion-player.html".into()),
        )
        .title("Wallta motion wallpaper")
        .visible(false)
        .focused(false)
        .focusable(false)
        .skip_taskbar(true)
        .decorations(false)
        .resizable(false)
        .shadow(false)
        .on_navigation(|url| {
            if url.path() == "/motion-player-error" {
                PLAYBACK_FAILED.store(true, Ordering::SeqCst);
                return false;
            }
            // Only the bundled page may ever navigate in this WebView.
            trusted_player_url(url)
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_page_load(move |window, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let configured = (|| -> Result<(), String> {
                let url = window
                    .convert_file_src(&media, None)
                    .map_err(|e| e.to_string())?;
                let poster = window
                    .convert_file_src(&poster_path, None)
                    .map_err(|e| e.to_string())?;
                let config =
                    serde_json::json!({ "url": url, "poster": poster, "gif": gif, "fit": fit });
                window
                    .eval(&format!(
                        "window.walltaMotionConfigure({config});window.walltaMotionPause({})",
                        PAUSED.load(Ordering::SeqCst)
                    ))
                    .map_err(|e| e.to_string())
            })();
            if configured.is_err() {
                PLAYBACK_FAILED.store(true, Ordering::SeqCst);
            }
        })
        .build()
        .map_err(|e| format!("Could not create motion wallpaper: {e}"))?;
        *WINDOW_LABEL.lock().map_err(|e| e.to_string())? = Some(label);
        let result = (|| {
            let hwnd = window.hwnd().map_err(|e| e.to_string())?;
            unsafe {
                native::attach(hwnd.0 as isize, worker)?;
            }
            window.show().map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            stop();
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (path, fit);
        Err("Desktop motion playback is available on Windows.".into())
    }
}

fn trusted_player_url(url: &url::Url) -> bool {
    if url.path() != "/motion-player.html" || url.query().is_some() || url.fragment().is_some() {
        return false;
    }
    let bundled =
        url.scheme() == "http" && url.host_str() == Some("tauri.localhost") && url.port().is_none();
    let dev = cfg!(debug_assertions)
        && url.scheme() == "http"
        && url.host_str() == Some("localhost")
        && url.port() == Some(1420);
    bundled || dev
}

/// Call during the existing scheduler tick. Explorer restart is a recoverable error;
/// do not leave a detached WebView floating above the user's applications.
pub fn health() -> Result<(), String> {
    if PLAYBACK_FAILED.load(Ordering::SeqCst) {
        stop();
        return Err(
            "Motion playback failed. Use a GIF or an MP4/WebM codec supported by this PC.".into(),
        );
    }
    #[cfg(windows)]
    if let Some(window) = current_window() {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        if !unsafe { native::attached(hwnd.0 as isize) } {
            stop();
            return Err("Windows Explorer restarted. Apply the motion wallpaper again.".into());
        }
        unsafe {
            native::resize(hwnd.0 as isize, native::parent(hwnd.0 as isize))?;
        }
    } else if WINDOW_LABEL.lock().is_ok_and(|label| label.is_some()) {
        stop();
        return Err(
            "Windows closed the motion wallpaper surface. Apply the wallpaper again.".into(),
        );
    }
    Ok(())
}

#[cfg(windows)]
mod native {
    type Hwnd = isize;
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> Hwnd;
        fn FindWindowExW(parent: Hwnd, after: Hwnd, class: *const u16, title: *const u16) -> Hwnd;
        fn EnumWindows(
            callback: unsafe extern "system" fn(Hwnd, isize) -> i32,
            param: isize,
        ) -> i32;
        fn SendMessageTimeoutW(
            hwnd: Hwnd,
            message: u32,
            wparam: usize,
            lparam: isize,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> isize;
        fn GetParent(hwnd: Hwnd) -> Hwnd;
        fn SetParent(hwnd: Hwnd, parent: Hwnd) -> Hwnd;
        fn GetWindowLongW(hwnd: Hwnd, index: i32) -> i32;
        fn SetWindowLongW(hwnd: Hwnd, index: i32, value: i32) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn SetWindowPos(
            hwnd: Hwnd,
            after: Hwnd,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        fn IsWindow(hwnd: Hwnd) -> i32;
        fn GetClassNameW(hwnd: Hwnd, name: *mut u16, max: i32) -> i32;
        fn ScreenToClient(hwnd: Hwnd, point: *mut Point) -> i32;
        fn SetLayeredWindowAttributes(hwnd: Hwnd, color: u32, alpha: u8, flags: u32) -> i32;
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    unsafe extern "system" fn find_worker(top: Hwnd, param: isize) -> i32 {
        if FindWindowExW(top, 0, wide("SHELLDLL_DefView").as_ptr(), std::ptr::null()) != 0 {
            let worker = FindWindowExW(0, top, wide("WorkerW").as_ptr(), std::ptr::null());
            if worker != 0 {
                *(param as *mut Hwnd) = worker;
                return 0;
            }
        }
        1
    }
    pub unsafe fn desktop_worker() -> Result<Hwnd, String> {
        let progman = FindWindowW(wide("Progman").as_ptr(), std::ptr::null());
        if progman == 0 {
            return Err("Windows Explorer desktop is unavailable.".into());
        }
        let mut result = 0;
        SendMessageTimeoutW(progman, 0x052c, 0, 0, 2, 1000, &mut result);
        // Recent Windows 11 shells compose desktop children inside Progman.
        // A layered child sits below the icon view and above its WorkerW.
        if raised(progman) {
            let child = FindWindowExW(progman, 0, wide("WorkerW").as_ptr(), std::ptr::null());
            let icons = FindWindowExW(
                progman,
                0,
                wide("SHELLDLL_DefView").as_ptr(),
                std::ptr::null(),
            );
            if child == 0 || icons == 0 {
                return Err(
                    "Windows desktop composition is not ready. Apply the wallpaper again.".into(),
                );
            }
            return Ok(progman);
        }
        let mut worker: Hwnd = 0;
        EnumWindows(find_worker, &mut worker as *mut Hwnd as isize);
        if worker == 0 {
            return Err(
                "This Windows desktop does not expose a background wallpaper surface.".into(),
            );
        }
        Ok(worker)
    }
    pub unsafe fn attach(hwnd: Hwnd, worker: Hwnd) -> Result<(), String> {
        let style = GetWindowLongW(hwnd, -16) as u32;
        SetWindowLongW(hwnd, -16, ((style & !0x8000_0000) | 0x4000_0000) as i32);
        if raised(worker) {
            let extended = GetWindowLongW(hwnd, -20) as u32;
            SetWindowLongW(hwnd, -20, (extended | 0x0008_0000) as i32);
            if SetLayeredWindowAttributes(hwnd, 0, 255, 2) == 0 {
                return Err("Windows could not compose the motion wallpaper surface.".into());
            }
        }
        SetParent(hwnd, worker);
        if GetParent(hwnd) != worker {
            return Err(
                "Windows refused to attach the motion wallpaper behind desktop icons.".into(),
            );
        }
        resize(hwnd, worker)
    }
    pub unsafe fn parent(hwnd: Hwnd) -> Hwnd {
        GetParent(hwnd)
    }
    unsafe fn raised(hwnd: Hwnd) -> bool {
        GetWindowLongW(hwnd, -20) as u32 & 0x0020_0000 != 0
    }
    pub unsafe fn resize(hwnd: Hwnd, worker: Hwnd) -> Result<(), String> {
        let (width, height) = (GetSystemMetrics(0), GetSystemMetrics(1));
        let mut origin = Point { x: 0, y: 0 };
        if ScreenToClient(worker, &mut origin) == 0 {
            return Err("Windows desktop coordinates are unavailable.".into());
        }
        let after = if raised(worker) {
            let icons = FindWindowExW(
                worker,
                0,
                wide("SHELLDLL_DefView").as_ptr(),
                std::ptr::null(),
            );
            if icons == 0 {
                return Err("Windows desktop icon surface disappeared.".into());
            }
            icons
        } else {
            1
        };
        if width <= 0
            || height <= 0
            || SetWindowPos(
                hwnd,
                after,
                origin.x,
                origin.y,
                width,
                height,
                0x0010 | 0x0020 | 0x0040,
            ) == 0
        {
            return Err("Windows could not size the primary-display wallpaper.".into());
        }
        Ok(())
    }
    pub unsafe fn attached(hwnd: Hwnd) -> bool {
        let parent = GetParent(hwnd);
        if IsWindow(hwnd) == 0 || parent == 0 || IsWindow(parent) == 0 {
            return false;
        }
        let mut name = [0u16; 64];
        let length = GetClassNameW(parent, name.as_mut_ptr(), name.len() as i32);
        if length <= 0 {
            return false;
        }
        let class = String::from_utf16_lossy(&name[..length as usize]);
        class == "WorkerW" || (class == "Progman" && raised(parent))
    }
}
