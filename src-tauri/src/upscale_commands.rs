use crate::{
    models::Source,
    upscale::{ModelInfo, StorageInfo, UpscaleManager, UpscaleMode, UpscaleStatus},
};
use std::sync::Arc;

#[tauri::command]
pub fn upscale_start(
    ids: Vec<String>,
    mode: UpscaleMode,
    model: Option<String>,
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<UpscaleStatus, String> {
    upscale
        .inner()
        .start_with_model(ids, mode, model.as_deref())
}
#[tauri::command]
pub fn upscale_start_all(
    source_id: Option<String>,
    mode: UpscaleMode,
    model: Option<String>,
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<UpscaleStatus, String> {
    upscale.inner().start_all(source_id, mode, model.as_deref())
}
#[tauri::command]
pub async fn upscale_models(
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<Vec<ModelInfo>, String> {
    let manager = upscale.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.models())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn upscale_storage(
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<StorageInfo, String> {
    let manager = upscale.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.storage())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn upscale_clear_cache(
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<StorageInfo, String> {
    let manager = upscale.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.clear_cache())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn upscale_delete_collection(
    collection_id: String,
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<StorageInfo, String> {
    let manager = upscale.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.delete_collection(&collection_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn upscale_status(
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<UpscaleStatus, String> {
    upscale.status()
}
#[tauri::command]
pub fn upscale_cancel(
    job_id: Option<String>,
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<UpscaleStatus, String> {
    upscale.cancel(job_id.as_deref())
}
#[tauri::command]
pub async fn upscale_save(
    job_id: String,
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<Source, String> {
    let upscale = upscale.inner().clone();
    tauri::async_runtime::spawn_blocking(move || upscale.save(&job_id))
        .await
        .map_err(|e| e.to_string())?
}
