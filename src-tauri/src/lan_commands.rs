use crate::{
    commands::Engine,
    lan::{LanManager, LanStatus},
    models::AppState,
};
use std::sync::Arc;

#[tauri::command]
pub async fn lan_start(
    media_ids: Vec<String>,
    engine: tauri::State<'_, Arc<Engine>>,
    lan: tauri::State<'_, Arc<LanManager>>,
) -> Result<LanStatus, String> {
    let engine = engine.inner().clone();
    let lan = lan.inner().clone();
    tauri::async_runtime::spawn_blocking(move || lan.start_with(|| engine.lan_exports(&media_ids)))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn lan_status(lan: tauri::State<'_, Arc<LanManager>>) -> LanStatus {
    lan.status()
}
#[tauri::command]
pub fn lan_stop(lan: tauri::State<'_, Arc<LanManager>>) -> LanStatus {
    lan.stop();
    lan.status()
}
#[tauri::command]
pub async fn lan_accept(
    engine: tauri::State<'_, Arc<Engine>>,
    lan: tauri::State<'_, Arc<LanManager>>,
) -> Result<AppState, String> {
    let engine = engine.inner().clone();
    let lan = lan.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        lan.accept_with(|path| engine.accept_lan_source(path))
    })
    .await
    .map_err(|e| e.to_string())?
}
