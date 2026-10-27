use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub location: String,
    #[serde(default)] pub recursive: bool,
    #[serde(default = "yes")] pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub access_token: Option<String>,
}
fn yes() -> bool { true }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem { pub id: String, pub name: String, pub url: String, pub source_id: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub interval_seconds: u64, pub order: String, pub fit: String,
    pub blur_background: bool, pub autostart: bool, pub wifi_only: bool,
    pub cache_limit_mb: u64, pub schedule_enabled: bool, pub days: Vec<u32>,
    pub start_time: String, pub end_time: String, pub show_clock: bool, pub slideshow_seconds: u64,
}
impl Default for Settings {
    fn default() -> Self { Self { interval_seconds: 900, order: "shuffle".into(), fit: "fill".into(), blur_background: false, autostart: false, wifi_only: false, cache_limit_mb: 256, schedule_enabled: false, days: (0..7).collect(), start_time: "07:00".into(), end_time: "23:00".into(), show_clock: false, slideshow_seconds: 30 } }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(10..=86400 * 30).contains(&self.interval_seconds) { return Err("Windows interval must be at least 10 seconds and at most 30 days.".into()); }
        if !["shuffle", "sequential"].contains(&self.order.as_str()) { return Err("Unknown rotation order.".into()); }
        if !["fill", "fit", "stretch"].contains(&self.fit.as_str()) { return Err("Unknown image fit.".into()); }
        if !(32..=10240).contains(&self.cache_limit_mb) { return Err("Cache must be between 32 and 10240 MB.".into()); }
        if self.days.iter().any(|d| *d > 6) { return Err("Schedule days must be 0–6.".into()); }
        for t in [&self.start_time, &self.end_time] { if chrono::NaiveTime::parse_from_str(t, "%H:%M").is_err() { return Err("Schedule times must use HH:mm.".into()); } }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppState {
    pub version: u32, pub sources: Vec<Source>, pub settings: Settings, pub running: bool,
    pub current: Option<MediaItem>, pub last_changed: Option<String>, pub error: Option<String>,
}
impl Default for AppState { fn default() -> Self { Self { version: 1, sources: vec![], settings: Settings::default(), running: false, current: None, last_changed: None, error: None } } }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities { pub platform: &'static str, pub wallpaper: bool, pub screensaver: bool, pub min_interval_seconds: u64, pub notes: Vec<&'static str> }
