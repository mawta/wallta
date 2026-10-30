use crate::models::{MediaItem, Source};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Read, path::Path, time::Duration};
use url::Url;

pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_LIST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ITEMS: usize = 10000;

pub fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("Too many redirects");
            }
            if attempt.url().scheme() != "https"
                && attempt.previous().iter().any(|u| u.scheme() == "https")
            {
                return attempt.error("HTTPS downgrade refused");
            }
            attempt.follow()
        }))
        .user_agent("Wallta/0.1")
        .build()
        .map_err(|_| "Could not initialize network client.".into())
}
pub fn network_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Enter a valid absolute HTTP(S) URL.".to_string())?;
    if !["https", "http"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an HTTP(S) URL without embedded credentials; enter credentials in their own fields.".into());
    }
    Ok(url)
}
pub fn item(url: String, name: Option<String>, source: &str) -> MediaItem {
    let id = format!(
        "{:x}",
        Sha256::digest(format!("{source}\0{url}").as_bytes())
    );
    let name = name.unwrap_or_else(|| {
        url.rsplit('/')
            .next()
            .unwrap_or("Image")
            .split('?')
            .next()
            .unwrap_or("Image")
            .to_string()
    });
    MediaItem {
        id,
        name,
        url,
        source_id: source.into(),
    }
}
pub fn is_image(path: &str) -> bool {
    let lower = path.split('?').next().unwrap_or(path).to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".webp", ".bmp", ".gif"]
        .iter()
        .any(|e| lower.ends_with(e))
}
fn is_local_image(path: &Path) -> bool {
    // A canonical Windows path starts with \\?\; it is not a URL query.
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["jpg", "jpeg", "png", "webp", "bmp", "gif"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}
pub fn bounded_response(response: Response, limit: u64) -> Result<Vec<u8>, String> {
    let response =
        response
            .error_for_status()
            .map_err(|e| match e.status().map(|s| s.as_u16()) {
                Some(401 | 403) => {
                    "Access denied. Check the source credentials and sharing permissions."
                        .to_string()
                }
                Some(code) => format!("Source returned HTTP {code}."),
                None => "Network request failed. Check your connection.".into(),
            })?;
    if response.content_length().is_some_and(|n| n > limit) {
        return Err("Source response exceeds the size limit.".into());
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the source response.".to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Source response exceeds the size limit.".into());
    }
    Ok(bytes)
}
fn json_response(request: RequestBuilder) -> Result<Value, String> {
    let bytes = bounded_response(
        request.send().map_err(|_| {
            "Network request failed. Check the source URL and connection.".to_string()
        })?,
        MAX_LIST_BYTES,
    )?;
    serde_json::from_slice(&bytes).map_err(|_| "Source did not return valid JSON.".into())
}
pub fn parse_manifest(text: &str, base: &str, source: &str) -> Result<Vec<MediaItem>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| "The manifest is not valid JSON.".to_string())?;
    let entries = value
        .as_array()
        .or_else(|| value.get("images").and_then(Value::as_array))
        .ok_or("Manifest must be an array or an object with an images array.")?;
    if entries.len() > MAX_ITEMS {
        return Err("Manifest contains more than 10000 images.".into());
    }
    let base = network_url(base)?;
    entries
        .iter()
        .map(|entry| {
            let raw = entry
                .as_str()
                .or_else(|| entry.get("url").and_then(Value::as_str))
                .ok_or("Every manifest entry must be a URL or an object with a URL.")?;
            let url = base
                .join(raw)
                .map_err(|_| "Manifest contains an invalid URL.".to_string())?;
            let url = network_url(url.as_str())?;
            Ok(item(
                url.into(),
                entry.get("name").and_then(Value::as_str).map(str::to_owned),
                source,
            ))
        })
        .collect()
}

fn webdav_folder(mut url: Url) -> Url {
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    url.set_query(None);
    url.set_fragment(None);
    url
}
fn webdav_roots(source: &Source) -> Result<Vec<Url>, String> {
    let mut roots: Vec<Url> = Vec::new();
    for location in source.effective_folders() {
        let url = network_url(location)?;
        if (source.password.is_some() || source.username.is_some()) && url.scheme() != "https" {
            return Err("WebDAV credentials require HTTPS.".into());
        }
        let root = webdav_folder(url);
        if !within_webdav_root(&root, &root) {
            return Err("WebDAV folders cannot contain encoded path separators.".into());
        }
        if roots
            .first()
            .is_some_and(|first| first.origin() != root.origin())
        {
            return Err("WebDAV folders in one source must use the same origin.".into());
        }
        roots.push(root);
    }
    Ok(roots)
}

fn within_webdav_root(url: &Url, root: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    network_url(url.as_str()).is_ok()
        && url.origin() == root.origin()
        && url.path().starts_with(root.path())
        // An encoded separator must not turn an apparently safe path into a
        // different hierarchy after a WebDAV server decodes it.
        && !path.contains("%2f") && !path.contains("%5c")
}
fn webdav_client(roots: &[Url]) -> Result<Client, String> {
    let roots = roots.to_vec();
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5
                || !roots
                    .iter()
                    .any(|root| within_webdav_root(attempt.url(), root))
            {
                return attempt.error("WebDAV redirects must stay inside the configured folder");
            }
            attempt.follow()
        }))
        .user_agent("Wallta/0.1")
        .build()
        .map_err(|_| "Could not initialize WebDAV client.".into())
}

struct WebdavPage {
    images: Vec<MediaItem>,
    folders: Vec<Url>,
}
pub fn parse_webdav(xml: &str, base: &str, source: &str) -> Result<Vec<MediaItem>, String> {
    let base = webdav_folder(network_url(base)?);
    Ok(parse_webdav_page(xml, &base, &base, source, false)?.images)
}
fn parse_webdav_page(
    xml: &str,
    base: &Url,
    root: &Url,
    source: &str,
    recursive: bool,
) -> Result<WebdavPage, String> {
    use quick_xml::{events::Event, Reader};
    if xml.len() as u64 > MAX_LIST_BYTES {
        return Err("Source response exceeds the size limit.".into());
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut href = String::new();
    let mut mime = String::new();
    let mut field = Vec::new();
    let mut collection = false;
    let mut images = Vec::new();
    let mut folders = Vec::new();
    let mut image_ids = HashSet::new();
    let mut folder_ids = HashSet::new();
    loop {
        match reader
            .read_event()
            .map_err(|_| "WebDAV response contains malformed XML.".to_string())?
        {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_vec();
                if name == b"response" {
                    href.clear();
                    mime.clear();
                    collection = false;
                }
                if name == b"collection" {
                    collection = true;
                }
                field = name;
            }
            Event::Empty(e) if e.local_name().as_ref() == b"collection" => collection = true,
            Event::Text(e) => {
                let text = e
                    .unescape()
                    .map_err(|_| "WebDAV XML contains invalid text.".to_string())?;
                if field == b"href" {
                    href.push_str(&text);
                }
                if field == b"getcontenttype" {
                    mime.push_str(&text);
                }
            }
            Event::End(e) => {
                if e.local_name().as_ref() == b"response" && !href.is_empty() {
                    if let Ok(mut url) = base.join(&href) {
                        url.set_fragment(None);
                        if collection {
                            url = webdav_folder(url);
                        }
                        if within_webdav_root(&url, root) {
                            if collection && recursive && folder_ids.insert(url.as_str().to_owned())
                            {
                                if folders.len() >= 1000 {
                                    return Err("WebDAV recursion is limited to 16 levels and 1000 folders. Choose a smaller folder.".into());
                                }
                                folders.push(url);
                            } else if !collection
                                && !url.path().ends_with('/')
                                && (is_image(url.path()) || mime.starts_with("image/"))
                                && images.len() < MAX_ITEMS
                            {
                                let media = item(url.into(), None, source);
                                if image_ids.insert(media.id.clone()) {
                                    images.push(media);
                                }
                            }
                        }
                    }
                }
                field.clear();
            }
            Event::DocType(_) => return Err("WebDAV XML document types are not supported.".into()),
            Event::Eof => break,
            _ => (),
        }
    }
    Ok(WebdavPage { images, folders })
}

fn walk_webdav(
    source: &Source,
    mut fetch: impl FnMut(&Url, Duration) -> Result<String, String>,
) -> Result<Vec<MediaItem>, String> {
    use std::{collections::VecDeque, time::Instant};
    let roots = webdav_roots(source)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut visited = HashSet::new();
    let mut pending = VecDeque::new();
    for root in roots {
        if visited.insert(root.as_str().to_owned()) {
            pending.push_back((root.clone(), root, 0usize));
        }
    }
    let mut image_ids = HashSet::new();
    let mut result = Vec::new();
    while let Some((folder, root, depth)) = pending.pop_front() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or("WebDAV listing exceeded its 60 second limit. Use a smaller folder.")?;
        let xml = fetch(&folder, remaining.min(Duration::from_secs(30)))?;
        let page = parse_webdav_page(&xml, &folder, &root, &source.id, source.recursive)?;
        if Instant::now() >= deadline {
            return Err(
                "WebDAV listing exceeded its 60 second limit. Use a smaller folder.".into(),
            );
        }
        for image in page.images {
            if image_ids.insert(image.id.clone()) {
                result.push(image);
                if result.len() >= MAX_ITEMS {
                    return Ok(result);
                }
            }
        }
        for child in page.folders {
            if visited.contains(child.as_str()) {
                continue;
            }
            if depth >= 16 || visited.len() >= 1000 {
                return Err("WebDAV recursion is limited to 16 levels and 1000 folders. Choose a smaller folder.".into());
            }
            visited.insert(child.as_str().to_owned());
            pending.push_back((child, root.clone(), depth + 1));
        }
    }
    Ok(result)
}
fn list_webdav(source: &Source) -> Result<Vec<MediaItem>, String> {
    let client = webdav_client(&webdav_roots(source)?)?;
    walk_webdav(source, |folder, timeout| {
        let request = client.request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), folder.clone())
            .timeout(timeout)
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body("<?xml version=\"1.0\"?><d:propfind xmlns:d=\"DAV:\"><d:prop><d:resourcetype/><d:getcontenttype/></d:prop></d:propfind>");
        // All queued URLs and redirect targets stay within explicitly selected roots.
        let request = if let Some(user) = &source.username {
            request.basic_auth(user, source.password.as_ref())
        } else {
            request
        };
        let bytes = bounded_response(
            request
                .send()
                .map_err(|_| "Could not connect to WebDAV.".to_string())?,
            MAX_LIST_BYTES,
        )?;
        String::from_utf8(bytes).map_err(|_| "WebDAV XML must be UTF-8.".into())
    })
}

fn local_list(source: &Source) -> Result<Vec<MediaItem>, String> {
    local_list_excluding(source, &[])
}

fn local_list_excluding(
    source: &Source,
    excluded: &[std::path::PathBuf],
) -> Result<Vec<MediaItem>, String> {
    use std::{collections::VecDeque, time::Instant};
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut pending = VecDeque::new();
    let mut visited = HashSet::new();
    for location in source.effective_folders() {
        let path = Path::new(location);
        let canonical = path.canonicalize().map_err(|_| {
            "Cannot read the local folder. Pick it again or check permissions.".to_string()
        })?;
        if !excluded.iter().any(|root| canonical.starts_with(root)) && visited.insert(canonical) {
            pending.push_back((path.to_path_buf(), 0usize));
        }
    }
    let mut result = vec![];
    let mut files = HashSet::new();
    let mut entries = 0usize;
    'scan: while let Some((path, depth)) = pending.pop_front() {
        for entry in fs::read_dir(&path).map_err(|_| {
            "Cannot read the local folder. Pick it again or check permissions.".to_string()
        })? {
            entries += 1;
            if Instant::now() >= deadline || entries > 100_000 {
                return Err("Local listing exceeded its 60 second / 100000 entry limit. Use smaller folders.".into());
            }
            let entry = entry.map_err(|_| "Cannot read a folder entry.".to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            // Do not follow symlinks/junctions outside the granted folder.
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && source.recursive {
                let canonical = entry.path().canonicalize().map_err(|e| e.to_string())?;
                if excluded.iter().any(|root| canonical.starts_with(root)) {
                    continue;
                }
                if !visited.contains(&canonical) {
                    if depth >= 16 || visited.len() >= 1000 {
                        return Err("Local recursion is limited to 16 levels and 1000 folders. Choose smaller folders.".into());
                    }
                    visited.insert(canonical);
                    pending.push_back((entry.path(), depth + 1));
                }
            } else if kind.is_file()
                && is_local_image(&entry.path())
                && files.insert(entry.path().canonicalize().map_err(|e| e.to_string())?)
            {
                result.push(item(
                    entry
                        .path()
                        .canonicalize()
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .into_owned(),
                    Some(entry.file_name().to_string_lossy().into_owned()),
                    &source.id,
                ));
            }
            if result.len() >= MAX_ITEMS {
                break 'scan;
            }
        }
    }
    result.sort_by(|a, b| a.url.cmp(&b.url));
    Ok(result)
}

pub fn list_source(client: &Client, source: &Source) -> Result<Vec<MediaItem>, String> {
    list_source_excluding(client, source, &[])
}

pub(crate) fn list_source_excluding(
    client: &Client,
    source: &Source,
    excluded: &[std::path::PathBuf],
) -> Result<Vec<MediaItem>, String> {
    let mut validated = source.clone();
    validated.validate_folders()?;
    let resolved = crate::google::authorize_source(&validated)?;
    let source = &resolved;
    match source.kind.as_str() {
        "local" => local_list_excluding(source, excluded),
        "url" => Ok(vec![item(
            network_url(&source.location)?.into(),
            None,
            &source.id,
        )]),
        "manifest" => {
            let bytes = bounded_response(
                client
                    .get(network_url(&source.location)?)
                    .send()
                    .map_err(|_| "Could not fetch manifest.".to_string())?,
                MAX_LIST_BYTES,
            )?;
            let text =
                std::str::from_utf8(&bytes).map_err(|_| "Manifest must be UTF-8.".to_string())?;
            parse_manifest(text, &source.location, &source.id)
        }
        "webdav" => list_webdav(source),
        "drive" => list_drive(client, source),
        "photos" => list_photos(client, source),
        _ => Err("Unknown source type.".into()),
    }
}

fn drive_folder(location: &str) -> Result<String, String> {
    let id = if location.starts_with("http") {
        let url = network_url(location)?;
        url.path_segments()
            .and_then(|s| {
                let parts: Vec<_> = s.collect();
                parts
                    .windows(2)
                    .find(|p| p[0] == "folders")
                    .map(|p| p[1].to_owned())
            })
            .ok_or("Use a Google Drive folder link or folder ID.")?
    } else {
        location.into()
    };
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid Google Drive folder ID.".into());
    }
    Ok(id)
}
fn list_drive(client: &Client, source: &Source) -> Result<Vec<MediaItem>, String> {
    if source.api_key.is_none() && source.access_token.is_none() {
        return Err("Google Drive needs an API key for a public folder, or an OAuth access token for private files. Configure credentials first.".into());
    }
    let id = drive_folder(&source.location)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    walk_drive(source, &id, |folder, page| {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or("Drive listing exceeded its 60 second limit. Use smaller folders.")?;
        let filter = if source.recursive {
            "(mimeType contains 'image/' or mimeType = 'application/vnd.google-apps.folder')"
        } else {
            "mimeType contains 'image/'"
        };
        let query = format!("'{folder}' in parents and trashed = false and {filter}");
        let mut request = client
            .get("https://www.googleapis.com/drive/v3/files")
            .timeout(remaining.min(Duration::from_secs(30)))
            .query(&[
                ("q", query.as_str()),
                ("pageSize", "1000"),
                ("fields", "nextPageToken,files(id,name,mimeType)"),
                ("pageToken", page),
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
            ]);
        if let Some(key) = &source.api_key {
            request = request.query(&[("key", key)]);
        }
        if let Some(token) = &source.access_token {
            request = request.bearer_auth(token);
        }
        json_response(request)
    })
}

pub fn walk_drive(
    source: &Source,
    root: &str,
    mut fetch: impl FnMut(&str, &str) -> Result<Value, String>,
) -> Result<Vec<MediaItem>, String> {
    use std::collections::VecDeque;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let roots = if source.folders.is_empty() {
        vec![drive_folder(root)?]
    } else {
        source
            .effective_folders()
            .into_iter()
            .map(drive_folder)
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut visited = HashSet::new();
    let mut folders = VecDeque::new();
    for root in roots {
        if visited.insert(root.clone()) {
            folders.push_back((root, 0usize));
        }
    }
    let mut file_ids = HashSet::new();
    let mut result = Vec::new();
    let mut pages = 0;
    while let Some((folder, depth)) = folders.pop_front() {
        let mut token = String::new();
        let mut seen = HashSet::new();
        loop {
            if std::time::Instant::now() > deadline || pages >= 100 {
                return Err(
                    "Drive listing exceeded its 60 second / 100 page limit. Use a smaller folder."
                        .into(),
                );
            }
            pages += 1;
            let page = fetch(&folder, &token)?;
            if std::time::Instant::now() >= deadline {
                return Err(
                    "Drive listing exceeded its 60 second / 100 page limit. Use a smaller folder."
                        .into(),
                );
            }
            let files = page
                .get("files")
                .and_then(Value::as_array)
                .ok_or("Drive did not return a files list.")?;
            for file in files {
                let (Some(id), Some(name), Some(mime)) = (
                    file.get("id").and_then(Value::as_str),
                    file.get("name").and_then(Value::as_str),
                    file.get("mimeType").and_then(Value::as_str),
                ) else {
                    continue;
                };
                if id.is_empty()
                    || id.len() > 256
                    || !id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    continue;
                }
                if mime == "application/vnd.google-apps.folder"
                    && source.recursive
                    && !visited.contains(id)
                {
                    if depth >= 16 || visited.len() >= 1000 {
                        return Err("Drive recursion is limited to 16 levels and 1000 folders. Choose a smaller folder.".into());
                    }
                    visited.insert(id.to_string());
                    folders.push_back((id.to_string(), depth + 1));
                } else if mime.starts_with("image/") && file_ids.insert(id.to_string()) {
                    result.push(item(
                        format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media"),
                        Some(name.into()),
                        &source.id,
                    ));
                    if result.len() >= MAX_ITEMS {
                        return Ok(result);
                    }
                }
            }
            let Some(next) = page
                .get("nextPageToken")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            else {
                break;
            };
            if !seen.insert(next.to_owned()) {
                return Err("Drive returned a repeated page token.".into());
            }
            token = next.into();
        }
    }
    Ok(result)
}
fn list_photos(client: &Client, source: &Source) -> Result<Vec<MediaItem>, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let access = source.access_token.as_ref().ok_or("Google Photos Ambient requires approved Ambient API credentials, a paired device ID and an access token. It does not read ordinary album links.")?;
    if source.location.trim().is_empty() {
        return Err("Enter the paired Google Photos Ambient device ID.".into());
    }
    let mut result = vec![];
    let mut token = String::new();
    let mut seen = HashSet::new();
    loop {
        if std::time::Instant::now() > deadline || seen.len() >= 100 {
            return Err("Photos listing exceeded its time or pagination limit.".into());
        }
        let page = json_response(
            client
                .get("https://photosambient.googleapis.com/v1/mediaItems")
                .bearer_auth(access)
                .query(&[
                    ("deviceId", source.location.as_str()),
                    ("pageToken", token.as_str()),
                    ("pageSize", "100"),
                ]),
        )?;
        if let Some(items) = page.get("mediaItems").and_then(Value::as_array) {
            for media in items {
                if let Some(file) = media.get("mediaFile") {
                    if file
                        .get("mimeType")
                        .and_then(Value::as_str)
                        .is_some_and(|m| m.starts_with("image/"))
                    {
                        if let Some(base) = file.get("baseUrl").and_then(Value::as_str) {
                            network_url(base)?;
                            // Provider-owned stable identity survives temporary base URL refresh.
                            if let Some(id) = media
                                .get("id")
                                .and_then(Value::as_str)
                                .filter(|id| !id.is_empty())
                            {
                                result.push(photos_item(base, id, &source.id));
                            }
                        }
                    }
                }
            }
        }
        if result.len() >= MAX_ITEMS {
            result.truncate(MAX_ITEMS);
            break;
        }
        let Some(next) = page
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
        else {
            break;
        };
        if !seen.insert(next.to_owned()) {
            return Err("Photos returned a repeated page token.".into());
        }
        token = next.into();
    }
    Ok(result)
}

pub fn download(client: &Client, media: &MediaItem, source: &Source) -> Result<Vec<u8>, String> {
    let mut validated = source.clone();
    validated.validate_folders()?;
    let resolved = crate::google::authorize_source(&validated)?;
    let source = &resolved;
    let url = network_url(&media.url)?;
    // A dedicated redirect policy also prevents same-origin redirects from
    // forwarding WebDAV credentials outside the granted root path.
    let scoped_client;
    let client = if source.kind == "webdav" {
        let roots = webdav_roots(source)?;
        if !roots.iter().any(|root| within_webdav_root(&url, root)) {
            return Err("The WebDAV image is outside the configured folder.".into());
        }
        scoped_client = webdav_client(&roots)?;
        &scoped_client
    } else {
        client
    };
    let mut request = client.get(url.clone());
    if source.kind == "webdav" {
        if let Some(user) = &source.username {
            request = request.basic_auth(user, source.password.as_ref());
        }
    }
    if source.kind == "drive" && url.host_str() == Some("www.googleapis.com") {
        if let Some(key) = &source.api_key {
            request = request.query(&[("key", key)]);
        }
        if let Some(token) = &source.access_token {
            request = request.bearer_auth(token);
        }
    }
    // Ambient base URLs are returned by Google's API and are short-lived; no bearer token is sent to the image host.
    bounded_response(
        request.send().map_err(|_| {
            "Could not download image. The current wallpaper has been kept.".to_string()
        })?,
        MAX_IMAGE_BYTES,
    )
}

#[cfg(test)]
mod webdav_tests {
    use super::*;

    #[test]
    fn canonical_local_folders_list_upscaled_images() {
        let root = tempfile::tempdir().unwrap();
        let output = root
            .path()
            .join("upscaled/0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("001.jpg"), b"saved image").unwrap();
        fs::write(output.join("002.PNG"), b"saved image").unwrap();
        fs::write(output.join("collection.json"), b"{}").unwrap();
        let output = output.canonicalize().unwrap();
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"upscaled", "name":"Saved 4K", "kind":"local",
            "location":output, "recursive":false, "enabled":true
        }))
        .unwrap();
        let listed = local_list(&source).unwrap();
        assert_eq!(
            listed.len(),
            2,
            "canonical Windows paths must remain readable"
        );
        assert!(listed.iter().all(|item| Path::new(&item.url).is_file()));
        assert!(is_image("https://example.com/photo.JPG?token=abc"));
        assert!(!is_image(
            "https://example.com/collection.json?name=photo.jpg"
        ));
    }

    #[test]
    fn managed_subtree_is_excluded_before_local_recursion_limits() {
        let root = tempfile::tempdir().unwrap();
        let managed = root.path().join("upscaled");
        fs::create_dir_all(&managed).unwrap();
        for index in 0..1001 {
            fs::create_dir(managed.join(index.to_string())).unwrap();
        }
        let originals = root.path().join("originals");
        fs::create_dir(&originals).unwrap();
        fs::write(originals.join("photo.png"), b"original").unwrap();
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"originals", "name":"Originals", "kind":"local", "location":root.path(), "recursive":true
        })).unwrap();
        assert!(local_list(&source).is_err());
        let items = local_list_excluding(&source, &[managed.canonicalize().unwrap()]).unwrap();
        assert_eq!(items.len(), 1);
        assert!(Path::new(&items[0].url).starts_with(originals.canonicalize().unwrap()));
    }

    fn source() -> Source {
        serde_json::from_value(serde_json::json!({"id":"dav", "name":"DAV", "kind":"webdav", "location":"https://dav.test/photos", "recursive":true, "enabled":true})).unwrap()
    }
    fn collection(href: &str) -> String {
        format!("<d:response><d:href>{href}</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>")
    }
    fn image(href: &str) -> String {
        format!("<d:response><d:href>{href}</d:href></d:response>")
    }
    fn page(entries: String) -> String {
        format!(r#"<d:multistatus xmlns:d="DAV:">{entries}</d:multistatus>"#)
    }

    #[test]
    fn selected_webdav_roots_are_both_visited_and_overlap_is_deduplicated() {
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"dav","name":"DAV","kind":"webdav","location":"https://dav.test/photos/",
            "recursive":true,"folders":[{"location":"https://dav.test/photos/"},
                {"location":"https://dav.test/photos/child/"},{"location":"https://dav.test/other/"}]
        })).unwrap();
        let mut visits = vec![];
        let images = walk_webdav(&source, |folder, _| {
            visits.push(folder.path().to_owned());
            Ok(page(match folder.path() {
                "/photos/" => collection("child/") + &image("child/shared.jpg"),
                "/photos/child/" => image("shared.jpg"),
                "/other/" => image("second.jpg"),
                _ => panic!("Unselected folder"),
            }))
        })
        .unwrap();
        assert_eq!(visits.len(), 3);
        assert!(visits.contains(&"/other/".into()));
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn two_local_roots_and_nested_overlap_return_each_file_once() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("one");
        let nested = first.join("nested");
        let second = root.path().join("two");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("first.jpg"), b"a").unwrap();
        fs::write(nested.join("nested.jpg"), b"b").unwrap();
        fs::write(second.join("second.jpg"), b"c").unwrap();
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"local","name":"Local","kind":"local","location":first,"recursive":true,
            "folders":[{"location":first},{"location":nested},{"location":second}]
        }))
        .unwrap();
        let items = local_list(&source).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(
            items
                .iter()
                .map(|item| &item.id)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn multiple_drive_roots_share_pagination_budget_and_deduplicate_files() {
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"drive","name":"Drive","kind":"drive","location":"first",
            "folders":[{"location":"first"},{"location":"second"}]
        }))
        .unwrap();
        let mut visits = vec![];
        let images = walk_drive(&source, "first", |folder, _| {
            visits.push(folder.to_owned());
            Ok(serde_json::json!({"files":[{"id":"shared","name":"same.jpg","mimeType":"image/jpeg"},
                {"id":format!("image-{folder}"),"name":"own.jpg","mimeType":"image/jpeg"}]}))
        }).unwrap();
        assert_eq!(visits, vec!["first", "second"]);
        assert_eq!(images.len(), 3);
        let mut count = 0;
        let error = walk_drive(&source, "first", |folder, _| {
            count += 1;
            Ok(if folder == "first" && count == 50 {
                serde_json::json!({"files":[]})
            } else {
                serde_json::json!({"files":[],"nextPageToken":format!("{folder}-{count}")})
            })
        })
        .unwrap_err();
        assert!(error.contains("100 page"));
        assert_eq!(count, 100);
    }

    #[test]
    fn local_scan_rejects_unbounded_depth() {
        let root = tempfile::tempdir().unwrap();
        let mut directory = root.path().to_path_buf();
        for _ in 0..17 {
            directory.push("a");
        }
        fs::create_dir_all(directory).unwrap();
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"local","name":"Local","kind":"local","location":root.path(),"recursive":true
        }))
        .unwrap();
        assert!(local_list(&source).unwrap_err().contains("16 levels"));
    }

    #[test]
    fn secondary_webdav_root_with_http_credentials_is_rejected_before_fetch() {
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"dav","name":"DAV","kind":"webdav","location":"https://dav.test/photos/","username":"user",
            "folders":[{"location":"https://dav.test/photos/"},{"location":"http://dav.test/other/"}]
        })).unwrap();
        assert!(
            walk_webdav(&source, |_, _| panic!("Do not send credentials"))
                .unwrap_err()
                .contains("HTTPS")
        );
    }

    #[test]
    fn webdav_roots_reject_other_account_origin_before_fetch() {
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"dav","name":"DAV","kind":"webdav","location":"https://dav.test/one/",
            "folders":[{"location":"https://dav.test/one/"},{"location":"https://other.test/two/"}]
        }))
        .unwrap();
        let error = walk_webdav(&source, |_, _| {
            panic!("Do not request an unrelated account origin")
        })
        .unwrap_err();
        assert!(error.contains("same origin"));
    }

    #[test]
    fn selected_webdav_roots_share_a_global_folder_budget() {
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"dav","name":"DAV","kind":"webdav","location":"https://dav.test/one/","recursive":true,
            "folders":[{"location":"https://dav.test/one/"},{"location":"https://dav.test/two/"}]
        })).unwrap();
        let mut count = 0;
        let error = walk_webdav(&source, |_, _| {
            count += 1;
            Ok(page(
                (0..600)
                    .map(|id| collection(&format!("child{id}/")))
                    .collect(),
            ))
        })
        .unwrap_err();
        assert!(error.contains("1000 folders"));
        assert_eq!(count, 2);
    }

    #[test]
    fn download_refuses_images_outside_all_selected_webdav_roots_before_network() {
        let source: Source = serde_json::from_value(serde_json::json!({
            "id":"dav","name":"DAV","kind":"webdav","location":"https://dav.test/one/","username":"user","password":"secret",
            "folders":[{"location":"https://dav.test/one/"},{"location":"https://dav.test/two/"}]
        })).unwrap();
        let client = client().unwrap();
        for url in [
            "https://dav.test/private/a.jpg",
            "https://dav.test/two-other/a.jpg",
            "https://other.test/two/a.jpg",
            "https://dav.test/two/%2f..%2fprivate.jpg",
        ] {
            let error =
                download(&client, &item(url.into(), None, &source.id), &source).unwrap_err();
            assert!(error.contains("outside the configured folder"), "{error}");
        }
    }

    #[test]
    fn webdav_redirect_cannot_escape_the_selected_roots() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /private/a.jpg\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let roots = [
            Url::parse(&format!("http://{address}/one/")).unwrap(),
            Url::parse(&format!("http://{address}/two/")).unwrap(),
        ];
        let error = webdav_client(&roots)
            .unwrap()
            .get(roots[1].join("a.jpg").unwrap())
            .send()
            .unwrap_err();
        server.join().unwrap();
        assert!(error.is_redirect());
    }

    #[test]
    fn recursive_webdav_uses_breadth_first_order_and_deduplicates_images() {
        let mut visits = vec![];
        let images = walk_webdav(&source(), |folder, timeout| {
            assert!(timeout <= Duration::from_secs(30));
            visits.push(folder.path().to_owned());
            Ok(page(match folder.path() {
                "/photos/" => collection("a/") + &collection("b/") + &image("root.jpg"),
                "/photos/a/" => collection("deep/") + &image("/photos/root.jpg"),
                "/photos/b/" => image("b.jpg"),
                "/photos/a/deep/" => image("deep.jpg"),
                path => panic!("Unexpected folder: {path}"),
            }))
        })
        .unwrap();
        assert_eq!(
            visits,
            vec!["/photos/", "/photos/a/", "/photos/b/", "/photos/a/deep/"]
        );
        assert_eq!(images.len(), 3);
    }

    #[test]
    fn recursive_webdav_enforces_depth_limit_before_fetching_deeper_folder() {
        let mut count = 0;
        let result = walk_webdav(&source(), |_, _| {
            count += 1;
            Ok(page(collection("deeper/")))
        });
        assert!(result.unwrap_err().contains("16 levels"));
        assert_eq!(count, 17); // root plus the permitted 16 descendants
    }

    #[test]
    fn recursive_webdav_enforces_global_folder_budget_before_fetch() {
        let mut count = 0;
        let result = walk_webdav(&source(), |_, _| {
            count += 1;
            Ok(page(
                (0..1000)
                    .map(|id| collection(&format!("sub{id}/")))
                    .collect(),
            ))
        });
        assert!(result.unwrap_err().contains("1000 folders"));
        assert_eq!(count, 1);
    }

    #[test]
    fn recursive_webdav_caps_the_total_image_count() {
        let images = walk_webdav(&source(), |_, _| {
            Ok(page(
                (0..10001).map(|id| image(&format!("{id}.jpg"))).collect(),
            ))
        })
        .unwrap();
        assert_eq!(images.len(), MAX_ITEMS);
    }

    #[test]
    fn webdav_rejects_oversized_xml_and_encoded_path_escapes() {
        assert!(parse_webdav(
            &" ".repeat(MAX_LIST_BYTES as usize + 1),
            "https://dav.test/photos/",
            "dav"
        )
        .unwrap_err()
        .contains("size limit"));
        let images = parse_webdav(
            &page(
                image("/photos/%2f..%2fprivate.jpg")
                    + &image("/photos/../private.jpg")
                    + &image("https://other.test/photos/a.jpg")
                    + &image("/photos2/a.jpg")
                    + &image("/photos/ok.jpg"),
            ),
            "https://dav.test/photos/",
            "dav",
        )
        .unwrap();
        assert_eq!(images.len(), 1);
        assert!(images[0].url.ends_with("/photos/ok.jpg"));
    }

    #[test]
    fn webdav_never_fetches_http_with_source_credentials() {
        let mut source = source();
        source.location = "http://dav.test/photos".into();
        source.username = Some("user".into());
        assert!(walk_webdav(&source, |_, _| panic!(
            "Credentials must be rejected before any request"
        ))
        .unwrap_err()
        .contains("HTTPS"));
    }
}

#[cfg(test)]
mod preview_identity_tests {
    use super::*;
    #[test]
    fn local_ids_match_canonical_saved_output_paths() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("photo.jpg"), b"image").unwrap();
        let source: Source = serde_json::from_value(
            serde_json::json!({"id":"local","name":"Local","kind":"local","location":temp.path()}),
        )
        .unwrap();
        let media = list_source(&client().unwrap(), &source).unwrap().remove(0);
        let canonical = temp
            .path()
            .join("photo.jpg")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(media.id, item(canonical.clone(), None, &source.id).id);
        assert_eq!(media.url, canonical);
    }
}

fn photos_item(base: &str, media_id: &str, source_id: &str) -> MediaItem {
    let mut media = item(
        format!("photos:{media_id}"),
        Some(media_id.into()),
        source_id,
    );
    media.url = format!("{base}=w3840-h2160");
    media
}
#[cfg(test)]
mod photos_identity_tests {
    #[test]
    fn photo_identity_survives_temporary_url_refresh() {
        let first = super::photos_item("https://photos.test/one?secret=a", "native-id", "source");
        let next = super::photos_item("https://photos.test/two?secret=b", "native-id", "source");
        assert_eq!(first.id, next.id);
        assert_ne!(first.url, next.url);
    }
}
