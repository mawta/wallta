use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFolder {
    pub location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub location: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<SourceFolder>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google: Option<GoogleConnection>,
}

impl Source {
    pub fn effective_folders(&self) -> Vec<&str> {
        if self.folders.is_empty() {
            vec![self.location.as_str()]
        } else {
            self.folders
                .iter()
                .map(|folder| folder.location.as_str())
                .collect()
        }
    }

    pub fn validate_folders(&mut self) -> Result<(), String> {
        if self.folders.is_empty() {
            return Ok(());
        }
        if !["local", "drive", "webdav"].contains(&self.kind.as_str()) {
            return Err(
                "This source uses a single location and does not support folder selections.".into(),
            );
        }
        if self.kind == "local"
            && self
                .google
                .as_ref()
                .is_some_and(|google| google.provider == "photos")
        {
            return Err("Google Photos imports are a managed collection. Reselect photos to change its contents.".into());
        }
        if self.folders.len() > 50 {
            return Err("A source supports at most 50 folders.".into());
        }
        let mut seen = std::collections::HashSet::new();
        let mut webdav_origin = None;
        for folder in &mut self.folders {
            folder.location = folder.location.trim().to_owned();
            if folder.location.is_empty() || folder.location.len() > 8192 {
                return Err("Every selected folder needs a location of at most 8192 bytes.".into());
            }
            let key = match self.kind.as_str() {
                "drive" => {
                    let id = if folder.location.starts_with("http") {
                        let url = url::Url::parse(&folder.location)
                            .map_err(|_| "Use a Google Drive folder link or folder ID.")?;
                        if !["http", "https"].contains(&url.scheme())
                            || url.host_str() != Some("drive.google.com")
                            || !url.username().is_empty()
                            || url.password().is_some()
                        {
                            return Err("Use a Google Drive folder link or folder ID.".into());
                        }
                        let segments: Vec<_> = url.path_segments().into_iter().flatten().collect();
                        segments
                            .windows(2)
                            .find(|pair| pair[0] == "folders")
                            .map(|pair| pair[1].to_owned())
                            .ok_or("Use a Google Drive folder link or folder ID.")?
                    } else {
                        folder.location.clone()
                    };
                    if id.is_empty()
                        || id.len() > 256
                        || !id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                    {
                        return Err("Invalid Google Drive folder ID.".into());
                    }
                    folder.location = id.clone();
                    id
                }
                "webdav" => {
                    let mut url = url::Url::parse(&folder.location)
                        .map_err(|_| "Use a valid HTTP(S) WebDAV folder URL.")?;
                    if !["http", "https"].contains(&url.scheme())
                        || url.host_str().is_none()
                        || !url.username().is_empty()
                        || url.password().is_some()
                    {
                        return Err(
                            "Use an HTTP(S) WebDAV folder URL without embedded credentials.".into(),
                        );
                    }
                    let path = url.path().to_ascii_lowercase();
                    if path.contains("%2f") || path.contains("%5c") {
                        return Err("WebDAV folders cannot contain encoded path separators.".into());
                    }
                    let origin = url.origin();
                    if webdav_origin.as_ref().is_some_and(|first| first != &origin) {
                        return Err("WebDAV folders in one source must use the same origin.".into());
                    }
                    webdav_origin = Some(origin);
                    url.set_query(None);
                    url.set_fragment(None);
                    if !url.path().ends_with('/') {
                        url.set_path(&format!("{}/", url.path()));
                    }
                    folder.location = url.to_string();
                    folder.location.clone()
                }
                // Windows folder names are case insensitive and accept either separator.
                "local" if cfg!(windows) => folder
                    .location
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_lowercase(),
                _ => folder.location.clone(),
            };
            if !seen.insert(key) {
                return Err("Selected folders must be unique.".into());
            }
            folder.name = folder
                .name
                .take()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty());
        }
        self.location = self.folders[0].location.clone();
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleConnection {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub connected: bool,
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: String,
    pub name: String,
    pub url: String,
    pub source_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub interval_seconds: u64,
    pub order: String,
    pub fit: String,
    pub blur_background: bool,
    pub autostart: bool,
    pub wifi_only: bool,
    pub cache_limit_mb: u64,
    pub schedule_enabled: bool,
    pub days: Vec<u32>,
    pub start_time: String,
    pub end_time: String,
    pub show_clock: bool,
    pub slideshow_seconds: u64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_seconds: 900,
            order: "shuffle".into(),
            fit: "fill".into(),
            blur_background: false,
            autostart: false,
            wifi_only: false,
            cache_limit_mb: 256,
            schedule_enabled: false,
            days: (0..7).collect(),
            start_time: "07:00".into(),
            end_time: "23:00".into(),
            show_clock: false,
            slideshow_seconds: 30,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(10..=86400 * 30).contains(&self.interval_seconds) {
            return Err("Windows interval must be at least 10 seconds and at most 30 days.".into());
        }
        if !["shuffle", "sequential"].contains(&self.order.as_str()) {
            return Err("Unknown rotation order.".into());
        }
        if !["fill", "fit", "stretch"].contains(&self.fit.as_str()) {
            return Err("Unknown image fit.".into());
        }
        if !(32..=10240).contains(&self.cache_limit_mb) {
            return Err("Cache must be between 32 and 10240 MB.".into());
        }
        if !(5..=86400).contains(&self.slideshow_seconds) {
            return Err("Slideshow interval must be between 5 seconds and 24 hours.".into());
        }
        if self.days.iter().any(|d| *d > 6) {
            return Err("Schedule days must be 0–6.".into());
        }
        for t in [&self.start_time, &self.end_time] {
            if chrono::NaiveTime::parse_from_str(t, "%H:%M").is_err() {
                return Err("Schedule times must use HH:mm.".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppState {
    pub version: u32,
    pub sources: Vec<Source>,
    pub settings: Settings,
    pub running: bool,
    pub current: Option<MediaItem>,
    pub last_changed: Option<String>,
    pub error: Option<String>,
}
impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 1,
            sources: vec![],
            settings: Settings::default(),
            running: false,
            current: None,
            last_changed: None,
            error: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub platform: &'static str,
    pub wallpaper: bool,
    pub screensaver: bool,
    pub min_interval_seconds: u64,
    pub notes: Vec<&'static str>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_folder_array_survives_state_round_trip() {
        let folders = json!([{"location":"one","name":"First"},{"location":"two"}]);
        let source: Source = serde_json::from_value(json!({
            "id":"legacy", "name":"Local", "kind":"local", "location":"old", "folders":folders
        }))
        .unwrap();
        assert_eq!(serde_json::to_value(source).unwrap()["folders"], folders);
    }

    #[test]
    fn legacy_and_empty_folders_use_the_existing_location() {
        for folders in [serde_json::Value::Null, json!([])] {
            let mut value = json!({"id":"old","name":"Old","kind":"local","location":"original"});
            if !folders.is_null() {
                value["folders"] = folders;
            }
            let mut source: Source = serde_json::from_value(value).unwrap();
            source.validate_folders().unwrap();
            assert_eq!(source.effective_folders(), vec!["original"]);
            assert!(serde_json::to_value(source)
                .unwrap()
                .get("folders")
                .is_none());
        }
    }

    #[test]
    fn explicit_folders_are_authoritative_and_preserve_source_identity() {
        let mut source: Source = serde_json::from_value(json!({
            "id":"saved-account","name":"Drive","kind":"drive","location":"old",
            "accessToken":"secret","folders":[{"location":" first ","name":" First "},{"location":"second"}]
        })).unwrap();
        source.validate_folders().unwrap();
        assert_eq!(source.location, "first");
        assert_eq!(source.effective_folders(), vec!["first", "second"]);
        assert_eq!(source.folders[0].name.as_deref(), Some("First"));
        assert_eq!(source.id, "saved-account");
        assert_eq!(source.access_token.as_deref(), Some("secret"));
    }

    #[test]
    fn folder_locations_are_normalized_and_reject_invalid_remote_roots() {
        let mut drive: Source = serde_json::from_value(json!({
            "id":"s","name":"Drive","kind":"drive","location":"old",
            "folders":[{"location":"https://drive.google.com/drive/folders/first?usp=sharing"},{"location":"second"}]
        })).unwrap();
        drive.validate_folders().unwrap();
        assert_eq!(drive.effective_folders(), vec!["first", "second"]);
        assert_eq!(drive.location, "first");
        for (kind, location) in [
            ("drive", "bad folder id"),
            ("drive", "https://untrusted.test/folders/first"),
            ("webdav", "file:///photos/"),
            ("webdav", "https://user:secret@dav.test/photos/"),
            ("webdav", "https://dav.test/photos/%2fprivate/"),
        ] {
            let mut source: Source = serde_json::from_value(json!({
                "id":"s","name":"Source","kind":kind,"location":"old","folders":[{"location":location}]
            })).unwrap();
            assert!(source.validate_folders().is_err(), "{location}");
        }
        let mut dav: Source = serde_json::from_value(json!({
            "id":"s","name":"DAV","kind":"webdav","location":"old",
            "folders":[{"location":"https://DAV.test:443/a?query=ignored#fragment"}]
        }))
        .unwrap();
        dav.validate_folders().unwrap();
        assert_eq!(dav.location, "https://dav.test/a/");
    }

    #[test]
    fn invalid_folder_arrays_are_rejected() {
        for folders in [
            json!([{"location":" "}]),
            json!([{"location":"a"},{"location":" a "}]),
            json!((0..51)
                .map(|id| json!({"location":format!("folder-{id}")}))
                .collect::<Vec<_>>()),
        ] {
            let mut source: Source = serde_json::from_value(json!({
                "id":"s","name":"Local","kind":"local","location":"old","folders":folders
            }))
            .unwrap();
            assert!(source.validate_folders().is_err());
        }
        for kind in ["photos", "url", "manifest"] {
            let mut source: Source = serde_json::from_value(json!({
                "id":"s","name":"Source","kind":kind,"location":"old","folders":[{"location":"new"}]
            }))
            .unwrap();
            assert!(source.validate_folders().is_err());
        }
    }

    #[test]
    fn folder_aliases_cannot_repeat_and_webdav_stays_on_one_account_origin() {
        for (kind, folders) in [
            (
                "drive",
                json!([{"location":"folder-id"},{"location":"https://drive.google.com/drive/folders/folder-id"}]),
            ),
            (
                "webdav",
                json!([{"location":"https://dav.test/a/"},{"location":"https://other.test/b/"}]),
            ),
            (
                "webdav",
                json!([{"location":"https://dav.test/a/"},{"location":"https://dav.test/a?ignored=query"}]),
            ),
        ] {
            let mut source: Source = serde_json::from_value(json!({
                "id":"s","name":"Source","kind":kind,"location":"old","folders":folders
            }))
            .unwrap();
            assert!(source.validate_folders().is_err());
        }
    }

    #[test]
    fn legacy_sources_without_google_metadata_still_load() {
        let source: Source = serde_json::from_value(json!({
            "id":"legacy", "name":"Drive", "kind":"drive", "location":"folder"
        }))
        .unwrap();
        assert!(serde_json::to_value(source)
            .unwrap()
            .get("google")
            .is_none());
    }

    #[test]
    fn google_connection_survives_state_round_trip() {
        let metadata = json!({"provider":"drive", "account":"wallta@example.com", "selectionName":"Wallpapers", "connected":true, "connectionId":"grant-a"});
        let state: AppState = serde_json::from_value(json!({"sources":[{
            "id":"drive", "name":"Drive", "kind":"drive", "location":"folder", "google":metadata
        }]}))
        .unwrap();
        assert_eq!(
            serde_json::to_value(state).unwrap()["sources"][0]["google"],
            metadata
        );
    }

    #[test]
    fn google_metadata_without_connection_id_migrates() {
        let source: Source = serde_json::from_value(json!({
            "id":"legacy", "name":"Drive", "kind":"drive", "location":"folder",
            "google":{"provider":"drive", "connected":true}
        }))
        .unwrap();
        assert!(source.google.unwrap().connection_id.is_none());
    }
}
