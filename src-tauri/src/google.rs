use crate::{commands::Engine, models::Source, providers, storage, wallpaper};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleStatus {
    phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_url: Option<String>,
    poll_interval_seconds: u64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selection_name: Option<String>,
}
struct Session {
    status: Mutex<GoogleStatus>,
    cancelled: AtomicBool,
    credential: Mutex<Option<Credential>>,
    import_directory: Mutex<Option<PathBuf>>,
    started: Instant,
}
static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<Session>>>> = OnceLock::new();
static CREDENTIAL_LOCK: Mutex<()> = Mutex::new(());
static CREDENTIAL_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn sessions() -> &'static Mutex<HashMap<String, Arc<Session>>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn replace_session(
    all: &mut HashMap<String, Arc<Session>>,
    source_id: String,
    session: Arc<Session>,
    now: Instant,
) -> Result<Vec<Arc<Session>>, String> {
    let mut retired = Vec::new();
    all.retain(|_, old| {
        if now.saturating_duration_since(old.started) < Duration::from_secs(1800) {
            return true;
        }
        old.cancelled.store(true, Ordering::SeqCst);
        retired.push(old.clone());
        false
    });
    if all.len() >= 32 && !all.contains_key(&source_id) {
        return Err("Có quá nhiều kết nối đang chờ. Khởi động lại Wallta rồi thử lại.".into());
    }
    if let Some(old) = all.insert(source_id, session) {
        old.cancelled.store(true, Ordering::SeqCst);
        retired.push(old);
    }
    Ok(retired)
}
fn retire_session(engine: &Engine, session: &Session, discard_imports: bool) -> Result<(), String> {
    session.cancelled.store(true, Ordering::SeqCst);
    if !discard_imports {
        return Ok(());
    }
    // Match save_state's lock order, and retain the state lock until deletion finishes.
    // Neither worker writes nor a concurrent save may publish another reference during cleanup.
    let runtime = engine.inner.lock().map_err(|e| e.to_string())?;
    let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
    let directory = session
        .import_directory
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    if let Some(directory) = directory {
        let mut sources = runtime.state.sources.clone();
        // A save can persist state before a later step fails. Protect those durable references too.
        match engine.store.load() {
            Ok(saved) => sources.extend(saved.sources),
            Err(_) => return Ok(()),
        }
        cleanup_import_directories(
            &engine.store.root,
            std::iter::once(directory.as_path()),
            &sources,
        );
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
struct Credential {
    client_id: String,
    client_secret: String,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}
fn random_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}
fn challenge(verifier: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = Sha256::digest(verifier.as_bytes());
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        result.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        result.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(n & 63) as usize] as char);
        }
    }
    result
}
fn callback_code(request: &str, state: &str) -> Result<String, String> {
    let target = request
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("GET "))
        .and_then(|line| line.split(' ').next())
        .ok_or("Invalid OAuth callback.")?;
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Invalid callback URL.")?;
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    if url.path() != "/oauth/callback" || query.get("state").map(String::as_str) != Some(state) {
        return Err("OAuth response state did not match. Try connecting again.".into());
    }
    if query.contains_key("error") {
        return Err("Google authorization was declined.".into());
    }
    query
        .get("code")
        .filter(|code| !code.is_empty())
        .cloned()
        .ok_or("Google did not return an authorization code.".into())
}
pub(crate) fn read_json(request: reqwest::blocking::RequestBuilder) -> Result<Value, String> {
    let bytes = providers::bounded_response(
        request
            .send()
            .map_err(|_| "Google could not be reached. Check the connection.")?,
        4 * 1024 * 1024,
    )?;
    serde_json::from_slice(&bytes).map_err(|_| "Google returned an invalid response.".into())
}
fn set_status(session: &Session, status: GoogleStatus) {
    if let Ok(mut current) = session.status.lock() {
        *current = status;
    }
}
fn check_cancel(session: &Session, deadline: Instant) -> Result<(), String> {
    if session.cancelled.load(Ordering::SeqCst) {
        return Err("Google connection cancelled.".into());
    }
    if Instant::now() > deadline {
        return Err("Google connection expired. Connect again.".into());
    }
    Ok(())
}
fn wait(session: &Session, seconds: u64, deadline: Instant) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(seconds.max(1));
    while Instant::now() < until {
        check_cancel(session, deadline)?;
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}
fn credential_from_token(
    token: &Value,
    client_id: String,
    client_secret: String,
) -> Result<Credential, String> {
    Ok(Credential {
        client_id,
        client_secret,
        access_token: token["access_token"]
            .as_str()
            .ok_or("Google did not grant access.")?
            .into(),
        refresh_token: token["refresh_token"].as_str().unwrap_or("").into(),
        expires_at: chrono::Utc::now().timestamp() + token["expires_in"].as_i64().unwrap_or(3600)
            - 60,
    })
}
#[cfg(windows)]
fn credential_entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("app.wallta.google", id)
        .map_err(|_| "Could not open protected Google storage.".into())
}
fn save_credential(id: &str, credential: &Credential) -> Result<(), String> {
    #[cfg(windows)]
    {
        credential_entry(id)?
            .set_password(&serde_json::to_string(credential).map_err(|e| e.to_string())?)
            .map_err(|_| "Could not securely save Google authorization.".into())
    }
    #[cfg(not(windows))]
    {
        let _ = (id, credential);
        Err("Google account storage requires the Windows build.".into())
    }
}
fn load_credential(id: &str) -> Result<Option<Credential>, String> {
    #[cfg(windows)]
    {
        match credential_entry(id)?.get_password() {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|_| "Saved Google authorization is invalid. Connect again.".into()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("Could not read protected Google authorization.".into()),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        Ok(None)
    }
}
pub fn disconnect(id: &str) -> Result<(), String> {
    if let Some(session) = sessions().lock().map_err(|e| e.to_string())?.remove(id) {
        session.cancelled.store(true, Ordering::SeqCst);
    }
    let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
    CREDENTIAL_EPOCH.fetch_add(1, Ordering::SeqCst);
    #[cfg(windows)]
    {
        match credential_entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Could not remove saved Google authorization.".into()),
        }
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}
pub fn commit_connection(id: &str) -> Result<(), String> {
    let pending = sessions()
        .lock()
        .map_err(|e| e.to_string())?
        .get(id)
        .cloned();
    if let Some(session) = pending {
        let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
        if !session.cancelled.load(Ordering::SeqCst) {
            if let Some(credential) = session
                .credential
                .lock()
                .map_err(|e| e.to_string())?
                .as_ref()
            {
                save_credential(id, credential)?;
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn google_cancel(
    source_id: String,
    discard_imports: bool,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let session = sessions()
            .lock()
            .map_err(|e| e.to_string())?
            .remove(&source_id);
        if let Some(session) = session {
            retire_session(&engine, &session, discard_imports)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
pub fn authorize_source(source: &Source) -> Result<Source, String> {
    let mut resolved = source.clone();
    if source.kind != "drive" {
        return Ok(resolved);
    }
    let (saved, epoch) = {
        let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
        (
            load_credential(&source.id)?,
            CREDENTIAL_EPOCH.load(Ordering::SeqCst),
        )
    };
    if let Some(mut credential) = saved {
        if credential.expires_at <= chrono::Utc::now().timestamp() {
            if credential.refresh_token.is_empty() {
                return Err("Google access expired. Connect this source again.".into());
            }
            let token = read_json(
                providers::client()?
                    .post("https://oauth2.googleapis.com/token")
                    .form(&[
                        ("client_id", credential.client_id.as_str()),
                        ("client_secret", credential.client_secret.as_str()),
                        ("refresh_token", credential.refresh_token.as_str()),
                        ("grant_type", "refresh_token"),
                    ]),
            )?;
            credential.access_token = token["access_token"]
                .as_str()
                .ok_or("Google access could not be refreshed. Connect again.")?
                .into();
            credential.expires_at =
                chrono::Utc::now().timestamp() + token["expires_in"].as_i64().unwrap_or(3600) - 60;
            let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
            if CREDENTIAL_EPOCH.load(Ordering::SeqCst) != epoch {
                return Err("Google connection changed while refreshing. Try again.".into());
            }
            save_credential(&source.id, &credential)?;
        }
        resolved.access_token = Some(credential.access_token);
    }
    Ok(resolved)
}

/// A connected Drive source can be kept disabled while its folder is chosen later.
pub(crate) fn validate_source_location(source: &Source) -> Result<(), String> {
    let pending_drive = source.kind == "drive"
        && !source.enabled
        && source
            .google
            .as_ref()
            .is_some_and(|connection| connection.provider == "drive" && connection.connected);
    if source.name.trim().is_empty() || (source.location.trim().is_empty() && !pending_drive) {
        return Err("Every source needs a name and location.".into());
    }
    Ok(())
}

pub(crate) struct DriveAccess {
    pub access_token: String,
    pub account: Option<String>,
    session: Option<Arc<Session>>,
}
impl DriveAccess {
    pub fn ensure_active(&self) -> Result<(), String> {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.cancelled.load(Ordering::SeqCst))
        {
            return Err("Google connection cancelled.".into());
        }
        Ok(())
    }
    pub fn selected(&self, id: &str, name: &str) -> Result<(), String> {
        self.ensure_active()?;
        if let Some(session) = &self.session {
            let mut status = session.status.lock().map_err(|e| e.to_string())?;
            self.ensure_active()?;
            status.location = Some(id.into());
            status.selection_name = Some(name.into());
        }
        Ok(())
    }
}

pub(crate) fn drive_access(engine: &Engine, source_id: &str) -> Result<DriveAccess, String> {
    let session = sessions()
        .lock()
        .map_err(|e| e.to_string())?
        .get(source_id)
        .cloned();
    if let Some(session) = session {
        let status = session.status.lock().map_err(|e| e.to_string())?.clone();
        if status.phase == "ready" && status.source_kind.as_deref() == Some("drive") {
            let credential = session
                .credential
                .lock()
                .map_err(|e| e.to_string())?
                .clone();
            if let Some(credential) = credential {
                if credential.expires_at > chrono::Utc::now().timestamp() {
                    let access = DriveAccess {
                        access_token: credential.access_token,
                        account: status.account,
                        session: Some(session),
                    };
                    access.ensure_active()?;
                    return Ok(access);
                }
            }
        }
    }
    let source = engine
        .snapshot()?
        .sources
        .into_iter()
        .find(|source| source.id == source_id && source.kind == "drive")
        .ok_or("Kết nối Google Drive trước khi chọn thư mục.")?;
    let account = source
        .google
        .as_ref()
        .and_then(|connection| connection.account.clone());
    let resolved = authorize_source(&source)?;
    let access_token = resolved
        .access_token
        .filter(|token| !token.is_empty())
        .ok_or("Không tìm thấy quyền Google Drive đã lưu. Kết nối lại nguồn ảnh.")?;
    Ok(DriveAccess {
        access_token,
        account,
        session: None,
    })
}

fn account_name(value: &Value, email_field: &str, name_field: &str) -> Option<String> {
    [email_field, name_field]
        .iter()
        .filter_map(|field| value[*field].as_str())
        .find(|value| !value.trim().is_empty() && value.len() <= 512)
        .map(str::to_owned)
}

fn google_account(
    client: &reqwest::blocking::Client,
    access_token: &str,
    drive: bool,
) -> Option<String> {
    if drive {
        let about = read_json(
            client
                .get("https://www.googleapis.com/drive/v3/about")
                .bearer_auth(access_token)
                .timeout(Duration::from_secs(5))
                .query(&[("fields", "user(displayName,emailAddress)")]),
        )
        .ok()?;
        account_name(&about["user"], "emailAddress", "displayName")
    } else {
        // Photos imports retain account provenance through the requested OpenID email scope.
        let profile = read_json(
            client
                .get("https://openidconnect.googleapis.com/v1/userinfo")
                .bearer_auth(access_token)
                .timeout(Duration::from_secs(5)),
        )
        .ok()?;
        account_name(&profile, "email", "name")
    }
}

fn authorization_scope(kind: &str) -> &'static str {
    if kind == "drive" {
        "https://www.googleapis.com/auth/drive.readonly"
    } else {
        "openid email https://www.googleapis.com/auth/photospicker.mediaitems.readonly"
    }
}

fn connect(
    session: Arc<Session>,
    source_id: String,
    kind: String,
    client_id: String,
    client_secret: String,
    root: PathBuf,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(900);
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "Could not start the local Google callback.")?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let redirect = format!(
        "http://127.0.0.1:{}/oauth/callback",
        listener.local_addr().map_err(|e| e.to_string())?.port()
    );
    let state = random_string(48);
    let verifier = random_string(64);
    let scope = authorization_scope(&kind);
    let mut authorization =
        url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
    authorization.query_pairs_mut().extend_pairs([
        ("client_id", client_id.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        ("scope", scope),
        ("state", state.as_str()),
        ("code_challenge", challenge(&verifier).as_str()),
        ("code_challenge_method", "S256"),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ]);
    set_status(
        &session,
        GoogleStatus {
            phase: "authorizing".into(),
            verification_url: Some(authorization.to_string()),
            poll_interval_seconds: 2,
            message: "Hoàn tất đăng nhập Google trong trình duyệt rồi quay lại Wallta.".into(),
            ..Default::default()
        },
    );
    open::that(authorization.as_str())
        .map_err(|_| "Could not open the system browser for Google sign-in.")?;
    let code = loop {
        check_cancel(&session, deadline)?;
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut bytes = [0u8; 16384];
                let count = stream
                    .read(&mut bytes)
                    .map_err(|_| "Could not read OAuth callback.")?;
                let parsed = callback_code(&String::from_utf8_lossy(&bytes[..count]), &state);
                let success = parsed.is_ok();
                let body = if success {
                    "<html><meta charset=utf-8><title>Wallta</title><p>Đã cấp quyền. Bạn có thể đóng trang này và quay lại Wallta.</p></html>"
                } else {
                    "<html><title>Wallta</title><p>Invalid authorization callback. Return to Wallta to try again.</p></html>"
                };
                let response=format!("HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",if success {"200 OK"}else{"400 Bad Request"},body.len(),body);
                let _ = stream.write_all(response.as_bytes());
                match parsed {
                    Ok(code) => break code,
                    Err(error) if error == "Google authorization was declined." => {
                        return Err(error)
                    }
                    Err(_) => (),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150))
            }
            Err(_) => return Err("Google callback listener failed.".into()),
        }
    };
    let client = providers::client()?;
    let token = read_json(client.post("https://oauth2.googleapis.com/token").form(&[
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code", code.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("code_verifier", verifier.as_str()),
        ("grant_type", "authorization_code"),
    ]))?;
    let credential = credential_from_token(&token, client_id, client_secret)?;
    check_cancel(&session, deadline)?;
    let account = google_account(&client, &credential.access_token, kind == "drive");
    check_cancel(&session, deadline)?;
    if kind == "drive" {
        let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
        check_cancel(&session, deadline)?;
        *session.credential.lock().map_err(|e| e.to_string())? = Some(credential);
        set_status(
            &session,
            GoogleStatus {
                phase: "ready".into(),
                poll_interval_seconds: 2,
                message: "Đã kết nối Google Drive. Chọn thư mục ảnh trong Wallta.".into(),
                source_kind: Some("drive".into()),
                account,
                ..Default::default()
            },
        );
        return Ok(());
    }
    let picker = read_json(
        client
            .post("https://photospicker.googleapis.com/v1/sessions")
            .bearer_auth(&credential.access_token)
            .json(&json!({})),
    )?;
    let picker_id = picker["id"]
        .as_str()
        .ok_or("Google did not create a photo selection session.")?
        .to_owned();
    let result = import_picker(
        &client,
        &session,
        &credential,
        &picker,
        &picker_id,
        &source_id,
        &root,
        deadline,
        account,
    );
    let _ = client
        .delete(format!(
            "https://photospicker.googleapis.com/v1/sessions/{}",
            urlencoding_segment(&picker_id)
        ))
        .bearer_auth(&credential.access_token)
        .send();
    result
}
fn urlencoding_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
fn import_directory(root: &std::path::Path, source_id: &str) -> PathBuf {
    root.join("google-import")
        .join(format!("{source_id}-{}", random_string(24)))
}
fn prepare_import_directory(
    session: &Session,
    root: &std::path::Path,
    source_id: &str,
    deadline: Instant,
) -> Result<PathBuf, String> {
    let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
    check_cancel(session, deadline)?;
    let mut pending = session.import_directory.lock().map_err(|e| e.to_string())?;
    let directory = import_directory(root, source_id);
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    *pending = Some(directory.clone());
    Ok(directory)
}

/// Remove only imports formerly referenced by saved sources, after a successful save.
/// Cleanup failures must not turn a durable connection update into a failed save.
pub(crate) fn cleanup_obsolete_imports(
    root: &std::path::Path,
    before: &[Source],
    after: &[Source],
) {
    cleanup_import_directories(
        root,
        before
            .iter()
            .filter(|source| source.kind == "local")
            .map(|source| std::path::Path::new(&source.location)),
        after,
    );
}
fn cleanup_import_directories<'a>(
    root: &std::path::Path,
    directories: impl Iterator<Item = &'a std::path::Path>,
    sources: &[Source],
) {
    let Ok(app_root) = root.canonicalize() else {
        return;
    };
    let import_path = app_root.join("google-import");
    let Ok(import_metadata) = std::fs::symlink_metadata(&import_path) else {
        return;
    };
    if !import_metadata.is_dir() || import_metadata.file_type().is_symlink() {
        return;
    }
    let Ok(import_root) = import_path.canonicalize() else {
        return;
    };
    if !import_root.is_absolute() || import_root.parent() != Some(app_root.as_path()) {
        return;
    }
    let mut references = Vec::new();
    for source in sources {
        match std::path::Path::new(&source.location).canonicalize() {
            Ok(path) => references.push(path),
            // An unresolved local alias may still refer to a candidate. Retain data if uncertain.
            Err(_) if source.kind == "local" => return,
            Err(_) => (),
        }
    }
    for directory in directories {
        let Ok(target) = directory.canonicalize() else {
            continue;
        };
        if !target.is_absolute()
            || target.parent() != Some(import_root.as_path())
            || references
                .iter()
                .any(|reference| reference.starts_with(&target) || target.starts_with(reference))
        {
            continue;
        }
        // Recheck the absolute resolved target immediately before the recursive deletion.
        if import_path.canonicalize().ok().as_ref() != Some(&import_root)
            || target.canonicalize().ok().as_ref() != Some(&target)
            || !std::fs::symlink_metadata(&target)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        {
            continue;
        }
        let _ = std::fs::remove_dir_all(&target);
    }
}
fn import_picker(
    client: &reqwest::blocking::Client,
    session: &Session,
    credential: &Credential,
    picker: &Value,
    picker_id: &str,
    source_id: &str,
    root: &std::path::Path,
    deadline: Instant,
    account: Option<String>,
) -> Result<(), String> {
    let picker_url = picker["pickerUri"]
        .as_str()
        .ok_or("Google did not return a photo selection link.")?
        .to_owned();
    let parsed =
        url::Url::parse(&picker_url).map_err(|_| "Invalid Google Photos selection link.")?;
    if parsed.scheme() != "https"
        || !parsed
            .host_str()
            .is_some_and(|host| host == "google.com" || host.ends_with(".google.com"))
    {
        return Err("Invalid Google Photos selection host.".into());
    }
    set_status(session,GoogleStatus {phase:"selecting".into(),verification_url:Some(picker_url.clone()),poll_interval_seconds:5,message:"Chọn những ảnh muốn nhập trên Google Photos. Bộ ảnh này được lưu vào thiết bị; không tự đồng bộ album.".into(),..Default::default()});
    let _ = open::that(&picker_url);
    let mut selection = picker.clone();
    while !selection["mediaItemsSet"].as_bool().unwrap_or(false) {
        let interval = selection["pollingConfig"]["pollInterval"]
            .as_str()
            .and_then(|v| v.trim_end_matches('s').parse::<f64>().ok())
            .unwrap_or(5.0)
            .ceil() as u64;
        wait(session, interval.max(1), deadline)?;
        selection = read_json(
            client
                .get(format!(
                    "https://photospicker.googleapis.com/v1/sessions/{}",
                    urlencoding_segment(picker_id)
                ))
                .bearer_auth(&credential.access_token),
        )?;
    }
    set_status(
        session,
        GoogleStatus {
            phase: "selecting".into(),
            poll_interval_seconds: 2,
            message: "Đang tải ảnh đã chọn về thiết bị…".into(),
            ..Default::default()
        },
    );
    let directory = prepare_import_directory(session, root, source_id, deadline)?;
    let mut page = String::new();
    let mut count = 0;
    let mut total = 0u64;
    let mut seen = std::collections::HashSet::new();
    loop {
        check_cancel(session, deadline)?;
        let response = read_json(
            client
                .get("https://photospicker.googleapis.com/v1/mediaItems")
                .bearer_auth(&credential.access_token)
                .query(&[
                    ("sessionId", picker_id),
                    ("pageSize", "100"),
                    ("pageToken", page.as_str()),
                ]),
        )?;
        for media in response["mediaItems"]
            .as_array()
            .ok_or("Google Photos selection contains no images.")?
        {
            check_cancel(session, deadline)?;
            let file = &media["mediaFile"];
            if !file["mimeType"]
                .as_str()
                .unwrap_or("")
                .starts_with("image/")
            {
                continue;
            }
            if count >= 500 {
                return Err("Chỉ nhập tối đa 500 ảnh mỗi lần. Hãy chọn bộ ảnh nhỏ hơn.".into());
            }
            let base = file["baseUrl"].as_str().ok_or("Photo URL is missing.")?;
            let url = providers::network_url(base)?;
            if url.scheme() != "https"
                || !url
                    .host_str()
                    .is_some_and(|host| host.ends_with(".googleusercontent.com"))
            {
                return Err("Google returned an unexpected photo host.".into());
            }
            let bytes = providers::bounded_response(
                client
                    .get(format!("{base}=w3840-h2160"))
                    .bearer_auth(&credential.access_token)
                    .send()
                    .map_err(|_| "Could not import the selected photo.")?,
                providers::MAX_IMAGE_BYTES,
            )?;
            total += bytes.len() as u64;
            if total > 512 * 1024 * 1024 {
                return Err("Bộ ảnh vượt giới hạn nhập 512 MB. Chọn ít ảnh hơn.".into());
            }
            wallpaper::thumbnail(&bytes)?;
            let extension = match image::guess_format(&bytes)
                .map_err(|_| "Unsupported imported photo format.")?
            {
                image::ImageFormat::Jpeg => "jpg",
                image::ImageFormat::Png => "png",
                image::ImageFormat::WebP => "webp",
                image::ImageFormat::Gif => "gif",
                image::ImageFormat::Bmp => "bmp",
                _ => return Err("Unsupported imported photo format.".into()),
            };
            check_cancel(session, deadline)?;
            let id = format!(
                "{:x}",
                Sha256::digest(media["id"].as_str().unwrap_or(base).as_bytes())
            );
            {
                let _guard = CREDENTIAL_LOCK.lock().map_err(|e| e.to_string())?;
                check_cancel(session, deadline)?;
                storage::atomic_write(&directory.join(format!("{id}.{extension}")), &bytes)?;
            }
            count += 1;
        }
        let next = response["nextPageToken"].as_str().unwrap_or("");
        if next.is_empty() {
            break;
        }
        if !seen.insert(next.to_owned()) || seen.len() > 100 {
            return Err("Google returned invalid photo pagination.".into());
        }
        page = next.into();
    }
    if count == 0 {
        return Err("Chưa chọn ảnh khả dụng. Hãy chọn JPG, PNG hoặc WebP.".into());
    }
    set_status(
        session,
        GoogleStatus {
            phase: "ready".into(),
            poll_interval_seconds: 2,
            message: format!("Đã nhập {count} ảnh Google Photos vào thiết bị."),
            location: Some(directory.to_string_lossy().into_owned()),
            source_kind: Some("local".into()),
            account,
            ..Default::default()
        },
    );
    Ok(())
}
#[tauri::command]
pub async fn google_begin(
    source_id: String,
    kind: String,
    client_id: String,
    client_secret: String,
    display_name: String,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<GoogleStatus, String> {
    let _ = display_name;
    if !["drive", "photos"].contains(&kind.as_str())
        || source_id.len() != 36
        || !source_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
    {
        return Err("Invalid Google source.".into());
    }
    if !client_id.ends_with(".apps.googleusercontent.com") {
        return Err("Nhập OAuth Client ID loại Desktop app từ Google Cloud Console.".into());
    }
    let initial = GoogleStatus {
        phase: "authorizing".into(),
        poll_interval_seconds: 2,
        message: "Đang mở trình duyệt để kết nối Google…".into(),
        ..Default::default()
    };
    let session = Arc::new(Session {
        status: Mutex::new(initial.clone()),
        cancelled: AtomicBool::new(false),
        credential: Mutex::new(None),
        import_directory: Mutex::new(None),
        started: Instant::now(),
    });
    let retired = {
        let mut all = sessions().lock().map_err(|e| e.to_string())?;
        replace_session(&mut all, source_id.clone(), session.clone(), Instant::now())?
    };
    // Release SESSIONS before taking the engine state lock: saves take them in reverse order.
    for old in retired {
        let _ = retire_session(engine.inner(), &old, true);
    }
    let engine = engine.inner().clone();
    let root = engine.store.root.clone();
    std::thread::spawn(move || {
        if let Err(error) = connect(
            session.clone(),
            source_id,
            kind,
            client_id,
            client_secret,
            root,
        ) {
            let _ = retire_session(&engine, &session, true);
            set_status(
                &session,
                GoogleStatus {
                    phase: "error".into(),
                    message: error,
                    poll_interval_seconds: 5,
                    ..Default::default()
                },
            );
        }
    });
    Ok(initial)
}
#[tauri::command]
pub fn google_poll(source_id: String) -> Result<GoogleStatus, String> {
    let session = sessions()
        .lock()
        .map_err(|e| e.to_string())?
        .get(&source_id)
        .cloned()
        .ok_or("Kết nối Google đã hết hạn. Kết nối lại.")?;
    let result = session.status.lock().map_err(|e| e.to_string())?.clone();
    Ok(result)
}
#[tauri::command]
pub async fn google_disconnect(source_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || disconnect(&source_id))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pending_session(directory: Option<PathBuf>, started: Instant) -> Arc<Session> {
        Arc::new(Session {
            status: Mutex::new(GoogleStatus::default()),
            cancelled: AtomicBool::new(false),
            credential: Mutex::new(None),
            import_directory: Mutex::new(directory),
            started,
        })
    }
    #[test]
    fn replaced_and_expired_sessions_discard_only_unsaved_imports() {
        let root = tempfile::tempdir().unwrap();
        let import_root = root.path().join("google-import");
        let first = import_root.join("first");
        let partial = import_root.join("partial");
        let saved = import_root.join("saved");
        for directory in [&first, &partial, &saved] {
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(directory.join("image.jpg"), b"keep-image").unwrap();
        }
        let engine = Engine::new(
            storage::Store::new(root.path().into(), root.path().join("cache")).unwrap(),
        )
        .unwrap();
        let mut saved_alias = photo_source("saved", &saved.join("..").join("saved"));
        saved_alias.enabled = false;
        engine.inner.lock().unwrap().state.sources.push(saved_alias);
        let started = Instant::now();
        let now = started + Duration::from_secs(1801);
        let replaced = pending_session(Some(first.clone()), now);
        let expired = pending_session(Some(partial.clone()), started);
        let referenced = pending_session(Some(saved.clone()), started);
        let mut all = HashMap::from([
            ("replaced".into(), replaced.clone()),
            ("expired".into(), expired.clone()),
            ("saved".into(), referenced.clone()),
        ]);
        let replacement = pending_session(None, now);
        for session in
            replace_session(&mut all, "replaced".into(), replacement.clone(), now).unwrap()
        {
            retire_session(&engine, &session, true).unwrap();
        }
        assert!(!first.exists());
        assert!(!partial.exists());
        assert!(saved.is_dir());
        assert_eq!(
            std::fs::read(saved.join("image.jpg")).unwrap(),
            b"keep-image"
        );
        assert!(replaced.cancelled.load(Ordering::SeqCst));
        assert!(expired.cancelled.load(Ordering::SeqCst));
        assert!(referenced.cancelled.load(Ordering::SeqCst));
        assert_eq!(all.len(), 1);
        assert!(Arc::ptr_eq(all.get("replaced").unwrap(), &replacement));
    }
    #[test]
    fn failed_partial_import_is_removed_and_cannot_be_recreated_after_cancel() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            storage::Store::new(root.path().into(), root.path().join("cache")).unwrap(),
        )
        .unwrap();
        let session = pending_session(None, Instant::now());
        let deadline = Instant::now() + Duration::from_secs(30);
        let partial = prepare_import_directory(&session, root.path(), "source", deadline).unwrap();
        std::fs::write(partial.join("partial.jpg"), b"partial").unwrap();
        retire_session(&engine, &session, true).unwrap();
        assert!(!partial.exists());
        assert!(prepare_import_directory(&session, root.path(), "source", deadline).is_err());
        assert_eq!(
            std::fs::read_dir(root.path().join("google-import"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn retiring_session_preserves_durable_reference_after_partially_failed_save() {
        let root = tempfile::tempdir().unwrap();
        let imported = root.path().join("google-import").join("durable");
        std::fs::create_dir_all(&imported).unwrap();
        let engine = Engine::new(
            storage::Store::new(root.path().into(), root.path().join("cache")).unwrap(),
        )
        .unwrap();
        let mut durable = engine.snapshot().unwrap();
        durable.sources.push(photo_source("saved", &imported));
        // State is durable, but a later save step failed before publishing it in memory.
        engine.store.save(&durable).unwrap();
        let session = pending_session(Some(imported.clone()), Instant::now());
        retire_session(&engine, &session, true).unwrap();
        assert!(imported.is_dir());
    }
    fn photo_source(id: &str, location: &std::path::Path) -> Source {
        serde_json::from_value(json!({"id":id,"name":"Photos","kind":"local","location":location,"google":{"provider":"photos","connected":true}})).unwrap()
    }
    #[test]
    fn saved_reselection_cleans_only_obsolete_import_locations() {
        let root = tempfile::tempdir().unwrap();
        let import_root = root.path().join("google-import");
        let old = import_root.join("old");
        let new = import_root.join("new");
        let pending = import_root.join("pending");
        for directory in [&old, &new, &pending] {
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(directory.join("photo.jpg"), b"imported-image").unwrap();
        }
        cleanup_obsolete_imports(
            root.path(),
            &[photo_source("photos", &old)],
            &[photo_source("photos", &new)],
        );
        assert!(!old.exists());
        assert!(new.is_dir());
        assert!(pending.is_dir());
        assert_eq!(
            std::fs::read(new.join("photo.jpg")).unwrap(),
            b"imported-image"
        );
        assert_eq!(
            std::fs::read(pending.join("photo.jpg")).unwrap(),
            b"imported-image"
        );
    }
    #[test]
    fn cleanup_preserves_disabled_alias_references_and_arbitrary_local_folders() {
        let root = tempfile::tempdir().unwrap();
        let import_root = root.path().join("google-import");
        let old = import_root.join("old");
        let new = import_root.join("new");
        let arbitrary = root.path().join("personal-photos");
        for directory in [&old, &new, &arbitrary] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let alias = new.join("..").join("old");
        let mut referenced = photo_source("disabled", &alias);
        referenced.enabled = false;
        cleanup_obsolete_imports(
            root.path(),
            &[
                photo_source("photos", &old),
                photo_source("personal", &arbitrary),
            ],
            &[photo_source("photos", &new), referenced],
        );
        assert!(old.is_dir());
        assert!(arbitrary.is_dir());
    }
    #[cfg(unix)]
    #[test]
    fn cleanup_preserves_symlink_alias_references() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("google-import").join("old");
        std::fs::create_dir_all(&old).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&old, &alias).unwrap();
        cleanup_obsolete_imports(
            root.path(),
            &[photo_source("photos", &old)],
            &[photo_source("alias", &alias)],
        );
        assert!(old.is_dir());
    }
    #[test]
    fn photos_authorization_requests_email_identity_and_picker_access() {
        let photos: Vec<_> = authorization_scope("photos").split_whitespace().collect();
        assert_eq!(
            photos,
            vec![
                "openid",
                "email",
                "https://www.googleapis.com/auth/photospicker.mediaitems.readonly"
            ]
        );
        assert_eq!(
            authorization_scope("drive"),
            "https://www.googleapis.com/auth/drive.readonly"
        );
    }
    #[test]
    fn account_display_name_is_used_when_email_is_unavailable() {
        assert_eq!(
            account_name(
                &json!({"emailAddress":"", "displayName":"Wallta User"}),
                "emailAddress",
                "displayName"
            )
            .as_deref(),
            Some("Wallta User")
        );
        assert!(account_name(&json!({}), "emailAddress", "displayName").is_none());
    }
    #[test]
    fn photos_reselection_uses_a_new_import_directory() {
        let root = std::path::Path::new("test-root");
        let first = import_directory(root, "source");
        let second = import_directory(root, "source");
        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(root.join("google-import").as_path()));
        assert_eq!(second.parent(), first.parent());
    }
    #[test]
    fn canceling_photos_reselection_preserves_the_saved_import() {
        let root = tempfile::tempdir().unwrap();
        let saved = root.path().join("google-import").join("saved");
        let pending = root.path().join("google-import").join("pending");
        let outside = root.path().join("outside");
        for directory in [&saved, &pending, &outside] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let source: Source = serde_json::from_value(json!({
            "id":"source", "name":"Photos", "kind":"local", "location":saved
        }))
        .unwrap();
        cleanup_import_directories(
            root.path(),
            [&saved, &pending, &outside]
                .map(PathBuf::as_path)
                .into_iter(),
            &[source],
        );
        assert!(saved.is_dir());
        assert!(!pending.exists());
        assert!(outside.is_dir());
    }
    #[test]
    fn pkce_uses_rfc7636_s256_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn callback_rejects_foreign_state_and_wrong_path() {
        assert!(callback_code(
            "GET /oauth/callback?code=abc&state=wrong HTTP/1.1\r\n",
            "expected"
        )
        .is_err());
        assert!(callback_code(
            "GET /other?code=abc&state=expected HTTP/1.1\r\n",
            "expected"
        )
        .is_err());
        assert_eq!(
            callback_code(
                "GET /oauth/callback?code=abc%2B123&state=expected HTTP/1.1\r\n",
                "expected"
            )
            .unwrap(),
            "abc+123"
        );
    }
}
