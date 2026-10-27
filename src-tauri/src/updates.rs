//! The webview supplies intent only. All update identities and bytes stay native.
use serde::Serialize;
use std::sync::Arc;
#[cfg(not(all(windows, target_arch = "x86_64")))]
use std::sync::Mutex;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current_version: String,
    phase: String,
    version: Option<String>,
    notes: Option<String>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
    last_checked: Option<u64>,
}

impl UpdateStatus {
    fn new() -> Self {
        Self {
            current_version: env!("CARGO_PKG_VERSION").into(),
            phase: if cfg!(all(windows, target_arch = "x86_64")) {
                "idle"
            } else {
                "unsupported"
            }
            .into(),
            version: None,
            notes: None,
            downloaded_bytes: 0,
            total_bytes: None,
            error: None,
            last_checked: None,
        }
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod windows;
#[cfg(all(windows, target_arch = "x86_64"))]
pub use windows::UpdateManager;

#[cfg(not(all(windows, target_arch = "x86_64")))]
pub struct UpdateManager(Mutex<UpdateStatus>);
#[cfg(not(all(windows, target_arch = "x86_64")))]
impl UpdateManager {
    pub fn new(_root: std::path::PathBuf) -> Self {
        Self(Mutex::new(UpdateStatus::new()))
    }
    pub fn status(&self) -> UpdateStatus {
        self.0.lock().unwrap().clone()
    }
}

#[tauri::command]
pub fn update_status(manager: tauri::State<'_, Arc<UpdateManager>>) -> UpdateStatus {
    manager.status()
}

#[tauri::command]
pub fn update_check(
    automatic: bool,
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<UpdateManager>>,
) -> UpdateStatus {
    #[cfg(all(windows, target_arch = "x86_64"))]
    manager.check(app, automatic);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    let _ = (automatic, app);
    manager.status()
}

#[tauri::command]
pub fn update_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<UpdateManager>>,
) -> UpdateStatus {
    #[cfg(all(windows, target_arch = "x86_64"))]
    manager.download(app);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    let _ = app;
    manager.status()
}

#[tauri::command]
pub fn update_cancel(manager: tauri::State<'_, Arc<UpdateManager>>) -> UpdateStatus {
    #[cfg(all(windows, target_arch = "x86_64"))]
    manager.cancel();
    manager.status()
}

#[tauri::command]
pub fn update_install(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<UpdateManager>>,
) -> UpdateStatus {
    #[cfg(all(windows, target_arch = "x86_64"))]
    manager.install(app);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    let _ = app;
    manager.status()
}
