use crate::{
    models::Source,
    upscale::{UpscaleManager, UpscaleMode, UpscaleStatus},
};
use std::sync::Arc;

#[tauri::command]
pub fn upscale_start(
    ids: Vec<String>,
    mode: UpscaleMode,
    upscale: tauri::State<'_, Arc<UpscaleManager>>,
) -> Result<UpscaleStatus, String> {
    upscale.inner().start(ids, mode)
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
