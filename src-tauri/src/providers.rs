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

pub fn parse_webdav(xml: &str, base: &str, source: &str) -> Result<Vec<MediaItem>, String> {
    use quick_xml::{events::Event, Reader};
    let base = network_url(base)?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut href = String::new();
    let mut mime = String::new();
    let mut field = Vec::new();
    let mut collection = false;
    let mut result = Vec::new();
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
                if e.local_name().as_ref() == b"response" && !collection {
                    if let Ok(url) = base.join(&href) {
                        if network_url(url.as_str()).is_ok()
                            && url.origin() == base.origin()
                            && url.path().starts_with(base.path())
                            && !url.path().ends_with('/')
                            && (is_image(url.path()) || mime.starts_with("image/"))
                        {
                            result.push(item(url.into(), None, source));
                        }
                    }
                    if result.len() >= MAX_ITEMS {
                        break;
                    }
                }
                field.clear();
            }
            Event::DocType(_) => return Err("WebDAV XML document types are not supported.".into()),
            Event::Eof => break,
            _ => (),
        }
    }
    Ok(result)
}

fn local_list(source: &Source) -> Result<Vec<MediaItem>, String> {
    fn scan(path: &Path, source: &Source, result: &mut Vec<MediaItem>) -> Result<(), String> {
        for entry in fs::read_dir(path).map_err(|_| {
            "Cannot read the local folder. Pick it again or check permissions.".to_string()
        })? {
            let entry = entry.map_err(|_| "Cannot read a folder entry.".to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            // Do not follow symlinks/junctions outside the granted folder.
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && source.recursive {
                scan(&entry.path(), source, result)?;
            } else if kind.is_file() && is_image(&entry.path().to_string_lossy()) {
                result.push(item(
                    entry.path().to_string_lossy().into_owned(),
                    Some(entry.file_name().to_string_lossy().into_owned()),
                    &source.id,
                ));
            }
            if result.len() >= MAX_ITEMS {
                return Ok(());
            }
        }
        Ok(())
    }
    let mut result = vec![];
    scan(Path::new(&source.location), source, &mut result)?;
    result.sort_by(|a, b| a.url.cmp(&b.url));
    Ok(result)
}

pub fn list_source(client: &Client, source: &Source) -> Result<Vec<MediaItem>, String> {
    match source.kind.as_str() {
        "local" => local_list(source),
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
        "webdav" => {
            let mut url = network_url(&source.location)?;
            if (source.password.is_some() || source.username.is_some()) && url.scheme() != "https" {
                return Err("WebDAV credentials require HTTPS.".into());
            }
            if !url.path().ends_with('/') {
                url.set_path(&format!("{}/", url.path()));
            }
            let request = client.request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), url.clone()).header("Depth", "1").header("Content-Type", "application/xml").body("<?xml version=\"1.0\"?><d:propfind xmlns:d=\"DAV:\"><d:prop><d:resourcetype/><d:getcontenttype/></d:prop></d:propfind>");
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
            parse_webdav(
                std::str::from_utf8(&bytes).map_err(|_| "WebDAV XML must be UTF-8.".to_string())?,
                url.as_str(),
                &source.id,
            )
        }
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
    walk_drive(source, &id, |folder, page| {
        let filter = if source.recursive {
            "(mimeType contains 'image/' or mimeType = 'application/vnd.google-apps.folder')"
        } else {
            "mimeType contains 'image/'"
        };
        let query = format!("'{folder}' in parents and trashed = false and {filter}");
        let mut request = client
            .get("https://www.googleapis.com/drive/v3/files")
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
    let mut folders = VecDeque::from([(root.to_string(), 0usize)]);
    let mut visited = HashSet::from([root.to_string()]);
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
                            result.push(item(
                                format!("{base}=w3840-h2160"),
                                media.get("id").and_then(Value::as_str).map(str::to_owned),
                                &source.id,
                            ));
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
    let url = network_url(&media.url)?;
    let mut request = client.get(url.clone());
    if source.kind == "webdav" && url.origin() == network_url(&source.location)?.origin() {
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
