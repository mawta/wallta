//! Temporary capability-based HTTP transfer. No routes expose application paths or credentials.
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    net::{Ipv4Addr, Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SESSION_TTL: Duration = Duration::from_secs(600);
const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FILES: usize = 50;
const MAX_PIXELS: u64 = 120_000_000;
const HEADER_LIMIT: usize = 16 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanStatus {
    pub active: bool,
    pub urls: Vec<String>,
    pub expires_at: u64,
    pub received_count: usize,
    pub received_bytes: u64,
    pub outgoing: Vec<Outgoing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Outgoing {
    pub id: String,
    pub name: String,
    pub size: u64,
}
pub struct ExportInput {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}
struct Export {
    public: Outgoing,
    path: PathBuf,
    mime: &'static str,
}
#[derive(Default)]
struct Received {
    count: usize,
    bytes: u64,
}
struct Shared {
    token: String,
    urls: Vec<String>,
    hosts: Vec<String>,
    expires_at: u64,
    deadline: Instant,
    stopped: AtomicBool,
    socket: Mutex<Option<TcpStream>>,
    received: Mutex<Received>,
    outgoing: Vec<Export>,
    dir: PathBuf,
}
impl Shared {
    fn alive(&self) -> bool {
        !self.stopped.load(Ordering::SeqCst) && Instant::now() < self.deadline
    }
    fn status(&self) -> LanStatus {
        if !self.alive() {
            return LanStatus::default();
        }
        let received = self.received.lock().unwrap_or_else(|e| e.into_inner());
        LanStatus {
            active: true,
            urls: self.urls.clone(),
            expires_at: self.expires_at,
            received_count: received.count,
            received_bytes: received.bytes,
            outgoing: self.outgoing.iter().map(|e| e.public.clone()).collect(),
            error: None,
        }
    }
    fn cancel(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(socket) = self
            .socket
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}
struct Session {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl Session {
    fn stop(&mut self) {
        self.shared.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.shared.dir);
    }
}
/// All native mutations hold this mutex, including the confirmation/persistence transaction.
pub struct LanManager {
    root: PathBuf,
    session: Mutex<Option<Session>>,
}
impl LanManager {
    pub fn new(app_root: &Path) -> Result<Self, String> {
        let root = app_root.join("lan-transfer");
        fs::create_dir_all(root.join("staging")).map_err(io_error)?;
        // Never follow a stale symlink/junction. Delete only session names owned by this feature.
        for entry in fs::read_dir(root.join("staging")).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            if valid_token(&entry.file_name().to_string_lossy()) {
                let kind = entry.file_type().map_err(io_error)?;
                if kind.is_dir() && !kind.is_symlink() {
                    let _ = fs::remove_dir_all(entry.path());
                } else if kind.is_symlink() || kind.is_file() {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        Ok(Self {
            root,
            session: Mutex::new(None),
        })
    }
    pub fn start_with(
        &self,
        resolve: impl FnOnce() -> Result<Vec<ExportInput>, String>,
    ) -> Result<LanStatus, String> {
        let mut current = self.session.lock().map_err(lock_error)?;
        *current = None;
        let addresses = if_addrs::get_if_addrs()
            .map_err(|_| "Could not read LAN interfaces.".to_string())?
            .into_iter()
            .filter_map(|i| match i.ip() {
                std::net::IpAddr::V4(ip) if ip.is_private() => Some(ip),
                _ => None,
            })
            .collect();
        self.start_locked(&mut current, resolve()?, addresses, SESSION_TTL)
    }
    #[cfg(test)]
    fn start_on(
        &self,
        outgoing: Vec<ExportInput>,
        addresses: Vec<Ipv4Addr>,
        ttl: Duration,
    ) -> Result<LanStatus, String> {
        let mut current = self.session.lock().map_err(lock_error)?;
        *current = None;
        self.start_locked(&mut current, outgoing, addresses, ttl)
    }
    fn start_locked(
        &self,
        current: &mut Option<Session>,
        outgoing: Vec<ExportInput>,
        mut addresses: Vec<Ipv4Addr>,
        ttl: Duration,
    ) -> Result<LanStatus, String> {
        addresses.sort();
        addresses.dedup();
        if addresses.is_empty() {
            return Err("No private Wi-Fi/LAN IPv4 address found. Connect both devices to the same trusted network.".into());
        }
        if outgoing.len() > MAX_FILES {
            return Err("Select at most 50 local images.".into());
        }
        let token = random_id();
        let dir = self.root.join("staging").join(&token);
        fs::create_dir(&dir).map_err(io_error)?;
        let prepared = (|| {
            fs::create_dir(dir.join("incoming")).map_err(io_error)?;
            fs::create_dir(dir.join("outgoing")).map_err(io_error)?;
            let mut total = 0u64;
            let mut exports = Vec::new();
            for input in outgoing {
                if !safe_id(&input.id) || exports.iter().any(|e: &Export| e.public.id == input.id) {
                    return Err("Invalid or duplicate selected image.".into());
                }
                let mut bytes = Vec::new();
                File::open(&input.path)
                    .map_err(|_| "Selected local image is no longer accessible.".to_string())?
                    .take(MAX_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(io_error)?;
                total += bytes.len() as u64;
                if bytes.len() as u64 > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
                    return Err("Selected images exceed the transfer size limit.".into());
                }
                let (extension, mime) = validate_image(&bytes).map_err(|_| {
                    "Selected image must be a valid JPEG, PNG or WebP within 120 MP.".to_string()
                })?;
                let path = dir
                    .join("outgoing")
                    .join(format!("{}.{}", input.id, extension));
                write_new(&path, &bytes)?;
                exports.push(Export {
                    public: Outgoing {
                        id: input.id,
                        name: display_name(&input.name, extension),
                        size: bytes.len() as u64,
                    },
                    path,
                    mime,
                });
            }
            let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
                .map_err(|_| "Cannot start Wi-Fi transfer listener.".to_string())?;
            listener.set_nonblocking(true).map_err(io_error)?;
            let port = listener.local_addr().map_err(io_error)?.port();
            let hosts: Vec<String> = addresses.iter().map(|ip| format!("{ip}:{port}")).collect();
            let urls = hosts
                .iter()
                .map(|host| format!("http://{host}/{token}/"))
                .collect();
            let shared = Arc::new(Shared {
                token,
                urls,
                hosts,
                expires_at: epoch_ms() + ttl.as_millis() as u64,
                deadline: Instant::now() + ttl,
                stopped: AtomicBool::new(false),
                socket: Mutex::new(None),
                received: Mutex::new(Received::default()),
                outgoing: exports,
                dir: dir.clone(),
            });
            let server = shared.clone();
            let worker = thread::Builder::new()
                .name("wallta-lan".into())
                .spawn(move || serve(listener, server))
                .map_err(io_error)?;
            let status = shared.status();
            *current = Some(Session {
                shared,
                worker: Some(worker),
            });
            Ok(status)
        })();
        if prepared.is_err() {
            let _ = fs::remove_dir_all(dir);
        }
        prepared
    }
    pub fn status(&self) -> LanStatus {
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if session.as_ref().is_some_and(|s| !s.shared.alive()) {
            *session = None;
        }
        session
            .as_ref()
            .map(|s| s.shared.status())
            .unwrap_or_default()
    }
    pub fn stop(&self) {
        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    pub fn accept_with<T>(
        &self,
        persist: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut slot = self.session.lock().map_err(lock_error)?;
        let mut session = slot.take().ok_or("Start a Wi-Fi transfer session first.")?;
        if !session.shared.alive() {
            return Err("Wi-Fi transfer expired. Start a new session.".into());
        }
        session.stop(); // Join all IO before moving files; expiry cleanup cannot race the move.
        if session.shared.received.lock().map_err(lock_error)?.count == 0 {
            return Err("No received images to add.".into());
        }
        let received_root = self.root.join("received");
        fs::create_dir_all(&received_root).map_err(io_error)?;
        let destination = received_root.join(random_id());
        fs::rename(session.shared.dir.join("incoming"), &destination).map_err(io_error)?;
        match persist(&destination) {
            Ok(value) => Ok(value),
            Err(error) => {
                let _ = fs::remove_dir_all(destination);
                Err(error)
            }
        }
    }
}
impl Drop for LanManager {
    fn drop(&mut self) {
        self.stop();
    }
}
fn io_error(error: std::io::Error) -> String {
    format!("Wi-Fi transfer file/network error: {error}")
}
fn lock_error<T>(_: std::sync::PoisonError<T>) -> String {
    "Wi-Fi transfer lock failed.".into()
}
fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn random_id() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn valid_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}
fn display_name(name: &str, extension: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || " -_.".contains(*c))
        .take(100)
        .collect();
    let stem = clean.trim_matches([' ', '.']);
    if stem.is_empty() {
        format!("image.{extension}")
    } else {
        format!("{stem}.{extension}")
    }
}
fn validate_image(bytes: &[u8]) -> Result<(&'static str, &'static str), u16> {
    let format = image::guess_format(bytes).map_err(|_| 415u16)?;
    let pair = match format {
        image::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        image::ImageFormat::Png => ("png", "image/png"),
        image::ImageFormat::WebP => ("webp", "image/webp"),
        _ => return Err(415),
    };
    let dimensions = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| 415u16)?;
    if dimensions.0 == 0
        || dimensions.1 == 0
        || dimensions.0 as u64 * dimensions.1 as u64 > MAX_PIXELS
    {
        return Err(413);
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| 415u16)?;
    Ok(pair)
}
fn serve(listener: TcpListener, shared: Arc<Shared>) {
    while shared.alive() {
        match listener.accept() {
            Ok((mut socket, _)) => {
                let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
                let _ = socket.set_write_timeout(Some(Duration::from_secs(1)));
                *shared.socket.lock().unwrap_or_else(|e| e.into_inner()) = socket.try_clone().ok();
                if shared.alive() {
                    handle(&mut socket, &shared);
                }
                *shared.socket.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(_) => break,
        }
    }
    // A native stop/accept joins this thread and owns cleanup. Automatic expiry removes immediately.
    if Instant::now() >= shared.deadline {
        shared.cancel();
        let _ = fs::remove_dir_all(&shared.dir);
    }
}
struct Request {
    method: String,
    target: String,
    host: String,
    origin: Option<String>,
    length: Option<u64>,
}
fn read_request(socket: &mut TcpStream, shared: &Shared, started: Instant) -> Result<Request, u16> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        if !shared.alive() || started.elapsed() > REQUEST_TIMEOUT {
            return Err(408);
        }
        let mut byte = [0];
        socket.read_exact(&mut byte).map_err(|_| 400u16)?;
        bytes.push(byte[0]);
        if bytes.len() > HEADER_LIMIT {
            return Err(431);
        }
    }
    let header = std::str::from_utf8(&bytes).map_err(|_| 400u16)?;
    let mut lines = header.split("\r\n");
    let parts: Vec<&str> = lines.next().ok_or(400u16)?.split(' ').collect();
    if parts.len() != 3
        || parts[2] != "HTTP/1.1"
        || !parts[1].starts_with('/')
        || parts[1].contains('#')
    {
        return Err(400);
    }
    let mut host = None;
    let mut origin = None;
    let mut length = None;
    for line in lines.filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once(':').ok_or(400u16)?;
        if key.is_empty()
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || value.bytes().any(|c| c < 32 && c != b'\t' || c == 127)
        {
            return Err(400);
        }
        let value = value.trim().to_owned();
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                if host.replace(value).is_some() {
                    return Err(400);
                }
            }
            "origin" => {
                if origin.replace(value).is_some() {
                    return Err(400);
                }
            }
            "content-length" => {
                if length.is_some()
                    || value.is_empty()
                    || !value.bytes().all(|c| c.is_ascii_digit())
                {
                    return Err(400);
                }
                length = Some(value.parse::<u64>().map_err(|_| 400u16)?);
            }
            "transfer-encoding" | "expect" => return Err(400),
            _ => (),
        }
    }
    Ok(Request {
        method: parts[0].into(),
        target: parts[1].into(),
        host: host.ok_or(400u16)?,
        origin,
        length,
    })
}
fn handle(socket: &mut TcpStream, shared: &Shared) {
    let started = Instant::now();
    let result = (|| -> Result<(), u16> {
        let request = read_request(socket, shared, started)?;
        if !shared.hosts.contains(&request.host) {
            return Err(403);
        }
        let prefix = format!("/{}/", shared.token);
        let route = request.target.strip_prefix(&prefix).ok_or(404u16)?;
        if !shared.alive() {
            return Err(410);
        }
        match (request.method.as_str(), route) {
            ("GET", "") if request.length.unwrap_or(0) == 0 => {
                respond(
                    socket,
                    shared,
                    started,
                    200,
                    "text/html; charset=utf-8",
                    &[],
                    include_str!("../../shared/lan/index.html").as_bytes(),
                );
            }
            ("GET", "status") if request.length.unwrap_or(0) == 0 => {
                let json = serde_json::to_vec(&shared.status()).map_err(|_| 500u16)?;
                respond(socket, shared, started, 200, "application/json", &[], &json);
            }
            ("GET", path) if path.starts_with("files/") && request.length.unwrap_or(0) == 0 => {
                let id = &path[6..];
                if !safe_id(id) {
                    return Err(404);
                }
                let export = shared
                    .outgoing
                    .iter()
                    .find(|e| e.public.id == id)
                    .ok_or(404u16)?;
                let bytes = fs::read(&export.path).map_err(|_| 404u16)?;
                respond(
                    socket,
                    shared,
                    started,
                    200,
                    export.mime,
                    &[(
                        "Content-Disposition",
                        format!("attachment; filename=\"{}\"", export.public.name),
                    )],
                    &bytes,
                );
            }
            ("POST", path) if path == "upload" || path.starts_with("upload?") => {
                if request.origin.as_deref() != Some(format!("http://{}", request.host).as_str()) {
                    return Err(403);
                }
                let length = request.length.ok_or(400u16)?;
                if length == 0 || length > MAX_FILE_BYTES {
                    return Err(413);
                }
                {
                    let received = shared.received.lock().map_err(|_| 500u16)?;
                    if received.count >= MAX_FILES || received.bytes + length > MAX_TOTAL_BYTES {
                        return Err(413);
                    }
                }
                let name = if let Some(query) = path.strip_prefix("upload?") {
                    let pairs: Vec<_> = url::form_urlencoded::parse(query.as_bytes()).collect();
                    if pairs.len() != 1 || pairs[0].0 != "name" {
                        return Err(400);
                    }
                    pairs[0].1.to_string()
                } else {
                    "image".into()
                };
                let mut bytes = Vec::with_capacity(length as usize);
                let mut buffer = [0u8; 64 * 1024];
                while (bytes.len() as u64) < length {
                    if !shared.alive() || started.elapsed() > REQUEST_TIMEOUT {
                        return Err(408);
                    }
                    let wanted = (length - bytes.len() as u64).min(buffer.len() as u64) as usize;
                    let count = socket.read(&mut buffer[..wanted]).map_err(|_| 400u16)?;
                    if count == 0 {
                        return Err(400);
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let (extension, _) = validate_image(&bytes)?;
                if !shared.alive() {
                    return Err(410);
                }
                let target = shared.dir.join("incoming").join(format!(
                    "{}-{}",
                    random_id(),
                    display_name(&name, extension)
                ));
                if write_new(&target, &bytes).is_err() {
                    let _ = fs::remove_file(&target);
                    return Err(500);
                }
                let mut received = shared.received.lock().map_err(|_| 500u16)?;
                received.count += 1;
                received.bytes += length;
                drop(received);
                respond(
                    socket,
                    shared,
                    started,
                    200,
                    "application/json",
                    &[],
                    br#"{"ok":true}"#,
                );
            }
            _ => return Err(404),
        }
        Ok(())
    })();
    if let Err(code) = result {
        respond(
            socket,
            shared,
            started,
            code,
            "application/json",
            &[],
            br#"{"ok":false,"error":"Request rejected"}"#,
        );
    }
    let _ = socket.shutdown(Shutdown::Both);
}
fn respond(
    socket: &mut TcpStream,
    shared: &Shared,
    started: Instant,
    code: u16,
    mime: &str,
    extra: &[(&str, String)],
    body: &[u8],
) {
    let mut headers = format!("HTTP/1.1 {code} {}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nPragma: no-cache\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\n", if code == 200 { "OK" } else { "Rejected" }, body.len());
    for (name, value) in extra {
        headers.push_str(&format!("{name}: {value}\r\n"));
    }
    headers.push_str("\r\n");
    for mut bytes in [headers.as_bytes(), body] {
        while !bytes.is_empty() {
            if !shared.alive() || started.elapsed() > REQUEST_TIMEOUT {
                return;
            }
            match socket.write(&bytes[..bytes.len().min(64 * 1024)]) {
                Ok(0) | Err(_) => return,
                Ok(written) => bytes = &bytes[written..],
            }
        }
    }
}

#[cfg(test)]
#[path = "lan_tests.rs"]
mod tests;
