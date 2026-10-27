use super::UpdateStatus;
use crate::{commands::Engine, lan::LanManager, storage, update_policy as policy, wallpaper};
use reqwest::Url;
use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::watch;

const ENDPOINT: &str = "https://github.com/mawta/wallta/releases/latest/download/latest.json";

struct Inner {
    status: UpdateStatus,
    gate: policy::JobGate,
    cancel: Option<watch::Sender<bool>>,
    candidate: Option<Update>,
    package: Option<Vec<u8>>,
}
pub struct UpdateManager {
    inner: Mutex<Inner>,
    attempt_file: PathBuf,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl UpdateManager {
    pub fn new(root: PathBuf) -> Self {
        let attempt_file = root.join("update-attempt.json");
        let mut status = UpdateStatus::new();
        status.last_checked = std::fs::read(&attempt_file)
            .ok()
            .filter(|b| b.len() <= 64)
            .and_then(|b| serde_json::from_slice(&b).ok());
        Self {
            inner: Mutex::new(Inner {
                status,
                gate: policy::JobGate::default(),
                cancel: None,
                candidate: None,
                package: None,
            }),
            attempt_file,
        }
    }
    pub fn status(&self) -> UpdateStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn check(self: &Arc<Self>, app: tauri::AppHandle, automatic: bool) {
        let mut inner = self.inner.lock().unwrap();
        if inner.gate.running
            || matches!(inner.status.phase.as_str(), "ready" | "installing")
            || !policy::allow_attempt(inner.status.last_checked, now(), automatic)
        {
            return;
        }
        let generation = inner.gate.begin().unwrap();
        let (sender, receiver) = watch::channel(false);
        inner.cancel = Some(sender);
        inner.candidate = None;
        inner.package = None;
        inner.status.phase = "checking".into();
        inner.status.error = None;
        inner.status.version = None;
        inner.status.notes = None;
        inner.status.downloaded_bytes = 0;
        inner.status.total_bytes = None;
        inner.status.last_checked = Some(now());
        let attempt = inner.status.last_checked.unwrap();
        drop(inner);
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = async {
                // Persist the attempt before networking, including failed checks and restarts.
                storage::atomic_write(&manager.attempt_file, attempt.to_string().as_bytes())?;
                let bytes = fetch(ENDPOINT, policy::MANIFEST_LIMIT, receiver, None).await?;
                let raw: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|_| "Invalid update manifest.".to_string())?;
                let version = raw
                    .get("version")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing update version.")?;
                if policy::stable_version(version).is_none()
                    || raw.get("draft").and_then(|v| v.as_bool()) == Some(true)
                    || raw.get("prerelease").and_then(|v| v.as_bool()) == Some(true)
                {
                    return Err("Only stable updates are supported.".into());
                }
                if !policy::newer_stable(env!("CARGO_PKG_VERSION"), version) {
                    return Ok(None);
                }
                let platform = raw
                    .pointer("/platforms/windows-x86_64")
                    .ok_or("Missing Windows update.")?;
                let asset = platform
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing Windows update URL.")?;
                validate_asset(asset, version)?;
                let signature = platform
                    .get("signature")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .ok_or("Missing update signature.")?;
                let lan = app.state::<Arc<LanManager>>().inner().clone();
                let updater = app
                    .updater_builder()
                    .target("windows-x86_64")
                    .on_before_exit(move || lan.stop())
                    .build()
                    .map_err(|e| e.to_string())?;
                // Restore the exact bounded manifest; no second manifest request is made.
                let update = updater
                    .restore_update(raw.clone())
                    .map_err(|e| e.to_string())?
                    .ok_or("Update is no longer newer.")?;
                if update.version != version
                    || update.download_url.as_str() != asset
                    || update.signature != signature
                {
                    return Err("Update identity does not match the manifest.".into());
                }
                Ok(Some(update))
            }
            .await;
            let mut inner = manager.inner.lock().unwrap();
            if inner.gate.current(generation) {
                match result {
                    Ok(Some(update)) => {
                        inner.status.phase = "available".into();
                        inner.status.version = Some(update.version.clone());
                        inner.status.notes = update.body.clone();
                        inner.candidate = Some(update);
                    }
                    Ok(None) => inner.status.phase = "upToDate".into(),
                    Err(error) => {
                        inner.status.phase = "error".into();
                        inner.status.error = Some(error);
                    }
                }
            }
            inner.gate.finish();
            inner.cancel = None;
        });
    }

    pub fn download(self: &Arc<Self>, app: tauri::AppHandle) {
        let mut inner = self.inner.lock().unwrap();
        if inner.gate.running || !matches!(inner.status.phase.as_str(), "available" | "error") {
            return;
        }
        let Some(update) = inner.candidate.clone() else {
            return;
        };
        let generation = inner.gate.begin().unwrap();
        let (sender, receiver) = watch::channel(false);
        inner.cancel = Some(sender);
        inner.package = None;
        inner.status.phase = "downloading".into();
        inner.status.error = None;
        inner.status.downloaded_bytes = 0;
        inner.status.total_bytes = None;
        drop(inner);
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = async {
                validate_asset(update.download_url.as_str(), &update.version)?;
                let engine = app.state::<Arc<Engine>>().inner().clone();
                let bytes = fetch(
                    update.download_url.as_str(),
                    policy::DOWNLOAD_LIMIT,
                    receiver,
                    Some((&manager, generation, &engine)),
                )
                .await?;
                // Official verification also requires signature-bound version for restored metadata.
                update
                    .verify(&bytes)
                    .map_err(|e| format!("Update signature verification failed: {e}"))?;
                Ok::<_, String>(bytes)
            }
            .await;
            let mut inner = manager.inner.lock().unwrap();
            if inner.gate.current(generation) {
                match result {
                    Ok(bytes) => {
                        inner.status.phase = "ready".into();
                        inner.package = Some(bytes);
                    }
                    Err(error) => {
                        inner.status.phase = "error".into();
                        inner.status.error = Some(error);
                        inner.status.downloaded_bytes = 0;
                        inner.status.total_bytes = None;
                    }
                }
            }
            inner.gate.finish();
            inner.cancel = None;
        });
    }

    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.status.phase == "installing" {
            return;
        }
        inner.gate.cancel();
        if let Some(sender) = &inner.cancel {
            sender.send_replace(true);
        }
        // No partial files are persisted; dropping the bounded buffer removes partial bytes.
        inner.package = None;
        inner.status.phase = if inner.candidate.is_some() {
            "available"
        } else {
            "idle"
        }
        .into();
        inner.status.error = None;
        inner.status.downloaded_bytes = 0;
        inner.status.total_bytes = None;
    }

    pub fn install(self: &Arc<Self>, app: tauri::AppHandle) {
        let mut inner = self.inner.lock().unwrap();
        if inner.gate.running || inner.status.phase != "ready" {
            return;
        }
        let Some(update) = inner.candidate.clone() else {
            return;
        };
        let Some(bytes) = inner.package.take() else {
            return;
        };
        inner.status.phase = "installing".into();
        inner.status.error = None;
        inner.gate.begin();
        drop(inner);
        let manager = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = (|| {
                validate_asset(update.download_url.as_str(), &update.version)?;
                if !policy::newer_stable(env!("CARGO_PKG_VERSION"), &update.version) {
                    return Err("Update is no longer newer.".into());
                }
                update.verify(&bytes).map_err(|e| e.to_string())?;
                let engine = app.state::<Arc<Engine>>();
                // The native barrier waits for prior saves, invalidates in-flight rotations,
                // and rejects new mutations until install fails or the plugin exits the app.
                let _barrier = engine.begin_update_install()?;
                update.install(&bytes).map_err(|e| e.to_string())
            })();
            // Windows success exits inside the plugin. Failure leaves wallpaper data intact.
            let mut inner = manager.inner.lock().unwrap();
            inner.gate.finish();
            inner.status.phase = "error".into();
            inner.status.error = Some(
                result
                    .err()
                    .unwrap_or_else(|| "Installer did not restart Wallta.".into()),
            );
            inner.status.downloaded_bytes = 0;
            inner.status.total_bytes = None;
        });
    }
}

fn transport(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
}
fn validate_asset(text: &str, version: &str) -> Result<(), String> {
    let url = Url::parse(text).map_err(|_| "Invalid update URL.".to_string())?;
    if !transport(&url)
        || url.query().is_some()
        || !policy::release_path(url.host_str().unwrap_or(""), url.path(), version)
    {
        return Err("Update URL is outside the approved release.".into());
    }
    Ok(())
}

async fn cancellable<T>(
    receiver: &mut watch::Receiver<bool>,
    future: impl Future<Output = Result<T, reqwest::Error>>,
) -> Result<T, String> {
    if *receiver.borrow() {
        return Err("Update cancelled.".into());
    }
    tokio::select! {
        _ = receiver.changed() => Err("Update cancelled.".into()),
        result = tokio::time::timeout(Duration::from_secs(20), future) => result.map_err(|_| "Update request timed out.".to_string())?.map_err(|_| "Unable to download update. Check your connection.".to_string()),
    }
}

async fn fetch(
    url: &str,
    limit: u64,
    mut receiver: watch::Receiver<bool>,
    progress: Option<(&UpdateManager, u64, &Engine)>,
) -> Result<Vec<u8>, String> {
    if let Some((_, _, engine)) = progress {
        if engine.snapshot()?.settings.wifi_only && !wallpaper::wifi_connected() {
            return Err("Connect to Wi-Fi to download this update.".into());
        }
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .user_agent(concat!("Wallta/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            if attempt.previous().len() >= 6
                || !transport(url)
                || !policy::redirect_path(url.host_str().unwrap_or(""), url.path())
            {
                attempt.error("Unapproved update redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = cancellable(
        &mut receiver,
        client.get(url).header("Accept-Encoding", "identity").send(),
    )
    .await?;
    if !response.status().is_success() {
        return Err(format!(
            "Update server returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    let total = response.content_length();
    if total.is_some_and(|n| n > limit) {
        return Err("Update exceeds the allowed size.".into());
    }
    let mut bytes = Vec::new();
    let mut size = 0;
    loop {
        if let Some((_, _, engine)) = progress {
            if engine.snapshot()?.settings.wifi_only && !wallpaper::wifi_connected() {
                return Err("Connect to Wi-Fi to download this update.".into());
            }
        }
        let Some(chunk) = cancellable(&mut receiver, response.chunk()).await? else {
            break;
        };
        size = policy::append_size(size, chunk.len(), limit).map_err(str::to_string)?;
        // Explicit bounded reservation avoids an allocator doubling beyond the size cap.
        if bytes.capacity() < size as usize {
            let capacity = bytes
                .capacity()
                .max(64 * 1024)
                .saturating_mul(2)
                .max(size as usize)
                .min(limit as usize);
            bytes.reserve_exact(capacity - bytes.len());
        }
        bytes.extend_from_slice(&chunk);
        if let Some((manager, generation, _)) = progress {
            let mut inner = manager.inner.lock().unwrap();
            if !inner.gate.current(generation) {
                return Err("Update cancelled.".into());
            }
            inner.status.downloaded_bytes = size;
            inner.status.total_bytes = total;
        }
    }
    if total.is_some_and(|n| n != size) || bytes.is_empty() {
        return Err("Update download is incomplete.".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_rejects_transport_and_parser_bypasses() {
        let asset =
            "https://github.com/mawta/wallta/releases/download/v0.1.3/Wallta_0.1.3_x64-setup.exe";
        assert!(validate_asset(asset, "0.1.3").is_ok());
        for invalid in [
            asset.replace("https:", "http:"),
            asset.replace("github.com", "github.com.attacker.test"),
            asset.replace("github.com", "user@github.com"),
            asset.replace("github.com", "github.com:8443"),
            format!("{asset}?other=1"),
            format!("{asset}#fragment"),
            asset.replace("Wallta_", "%57allta_"),
            asset.replace("v0.1.3/", "v0.1.2/"),
        ] {
            assert!(validate_asset(&invalid, "0.1.3").is_err(), "{invalid}");
        }
    }

    #[test]
    fn cancellation_interrupts_an_in_flight_request_without_waiting_for_timeout() {
        let (sender, mut receiver) = watch::channel(false);
        tauri::async_runtime::block_on(async {
            let request = cancellable(
                &mut receiver,
                std::future::pending::<Result<(), reqwest::Error>>(),
            );
            let cancel = async {
                sender.send_replace(true);
            };
            let (result, _) = tokio::join!(request, cancel);
            assert_eq!(result.unwrap_err(), "Update cancelled.");
        });
    }

    #[test]
    fn precancelled_request_never_polls_network_future() {
        let (sender, mut receiver) = watch::channel(false);
        sender.send_replace(true);
        let result = tauri::async_runtime::block_on(cancellable::<()>(&mut receiver, async {
            panic!("cancelled request was polled")
        }));
        assert_eq!(result.unwrap_err(), "Update cancelled.");
    }
}
