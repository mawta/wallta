use crate::{commands::Engine, google};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const PAGE_SIZE: usize = 100;

#[derive(Serialize)]
pub struct DriveFolder {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFolderPage {
    folders: Vec<DriveFolder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_page_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<String>,
}

#[derive(Serialize)]
pub struct DriveSelection {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<String>,
}

fn validate_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("Google Drive folder ID is invalid.".into());
    }
    Ok(())
}

fn validate_page_token(value: &str) -> Result<(), String> {
    if value.len() > 4096 || !value.bytes().all(|c| c.is_ascii_graphic()) {
        return Err("Google Drive page token is invalid.".into());
    }
    Ok(())
}

fn folder_query(parent: &str, shared: bool) -> Result<String, String> {
    validate_id(parent)?;
    let base = format!("mimeType = '{FOLDER_MIME}' and trashed = false");
    Ok(if shared && parent == "root" {
        format!("{base} and sharedWithMe = true")
    } else {
        format!("{base} and '{parent}' in parents")
    })
}

fn parse_folder(value: &Value) -> Result<DriveFolder, String> {
    let id = value["id"]
        .as_str()
        .ok_or("Google returned a folder without its ID.")?;
    validate_id(id)?;
    let name = value["name"]
        .as_str()
        .filter(|name| !name.is_empty() && name.len() <= 4096)
        .ok_or("Google returned an invalid folder name.")?;
    Ok(DriveFolder {
        id: id.into(),
        name: name.into(),
    })
}

fn parse_folders(value: &Value, account: Option<String>) -> Result<DriveFolderPage, String> {
    let files = value["files"]
        .as_array()
        .ok_or("Google returned an invalid folder list.")?;
    if files.len() > PAGE_SIZE {
        return Err("Google returned too many folders in one page.".into());
    }
    let next_page_token = match value.get("nextPageToken") {
        None | Some(Value::Null) => None,
        Some(Value::String(token)) => {
            validate_page_token(token)?;
            (!token.is_empty()).then(|| token.clone())
        }
        _ => return Err("Google returned invalid folder pagination.".into()),
    };
    Ok(DriveFolderPage {
        folders: files.iter().map(parse_folder).collect::<Result<_, _>>()?,
        next_page_token,
        account,
    })
}

fn parse_selection(value: &Value, account: Option<String>) -> Result<DriveSelection, String> {
    if value["mimeType"].as_str() != Some(FOLDER_MIME) || value["trashed"].as_bool() != Some(false)
    {
        return Err("Chọn một thư mục Google Drive còn khả dụng.".into());
    }
    let folder = parse_folder(value)?;
    Ok(DriveSelection {
        id: folder.id,
        name: folder.name,
        account,
    })
}

#[tauri::command]
pub async fn google_drive_folders(
    source_id: String,
    parent_id: String,
    page_token: Option<String>,
    shared: Option<bool>,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<DriveFolderPage, String> {
    let query = folder_query(&parent_id, shared.unwrap_or(false))?;
    let page_token = page_token.unwrap_or_default();
    validate_page_token(&page_token)?;
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let access = google::drive_access(&engine, &source_id)?;
        access.ensure_active()?;
        let response = google::read_json(
            engine
                .client
                .get("https://www.googleapis.com/drive/v3/files")
                .bearer_auth(&access.access_token)
                .query(&[
                    ("q", query.as_str()),
                    ("fields", "nextPageToken,files(id,name)"),
                    ("pageSize", "100"),
                    ("pageToken", page_token.as_str()),
                    ("orderBy", "name_natural"),
                    ("supportsAllDrives", "true"),
                    ("includeItemsFromAllDrives", "true"),
                ]),
        )?;
        access.ensure_active()?;
        parse_folders(&response, access.account)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn google_drive_select(
    source_id: String,
    folder_id: String,
    engine: tauri::State<'_, Arc<Engine>>,
) -> Result<DriveSelection, String> {
    validate_id(&folder_id)?;
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let access = google::drive_access(&engine, &source_id)?;
        access.ensure_active()?;
        let response = google::read_json(
            engine
                .client
                .get(format!(
                    "https://www.googleapis.com/drive/v3/files/{folder_id}"
                ))
                .bearer_auth(&access.access_token)
                .query(&[
                    ("fields", "id,name,mimeType,trashed"),
                    ("supportsAllDrives", "true"),
                ]),
        )?;
        let selection = parse_selection(&response, access.account.clone())?;
        access.selected(&selection.id, &selection.name)?;
        Ok(selection)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn folder_query_separates_shared_root_from_folder_children() {
        let base = "mimeType = 'application/vnd.google-apps.folder' and trashed = false";
        assert_eq!(
            folder_query("root", false).unwrap(),
            format!("{base} and 'root' in parents")
        );
        assert_eq!(
            folder_query("root", true).unwrap(),
            format!("{base} and sharedWithMe = true")
        );
        assert_eq!(
            folder_query("shared_folder", false).unwrap(),
            format!("{base} and 'shared_folder' in parents")
        );
        for invalid in ["", "a' or true", "a/b", "a\\b", " a "] {
            assert!(folder_query(invalid, false).is_err(), "{invalid}");
        }
    }

    #[test]
    fn parses_empty_and_paginated_folder_results() {
        let empty = parse_folders(&json!({"files":[]}), None).unwrap();
        assert!(empty.folders.is_empty());
        let page = parse_folders(
            &json!({"files":[{"id":"abc_123", "name":"Ảnh nhà"}], "nextPageToken":"token/+=abc"}),
            Some("user@example.com".into()),
        )
        .unwrap();
        assert_eq!(page.folders[0].name, "Ảnh nhà");
        assert_eq!(page.next_page_token.as_deref(), Some("token/+=abc"));
        assert_eq!(page.account.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn rejects_malformed_pages_and_non_folder_selection() {
        for invalid in [
            json!({}),
            json!({"files":[{"id":"a/b", "name":"bad"}]}),
            json!({"files":[], "nextPageToken":"a\n"}),
        ] {
            assert!(parse_folders(&invalid, None).is_err());
        }
        assert!(parse_selection(
            &json!({"id":"a", "name":"image", "mimeType":"image/jpeg", "trashed":false}),
            None
        )
        .is_err());
        assert!(parse_selection(
            &json!({"id":"a", "name":"removed", "mimeType":FOLDER_MIME, "trashed":true}),
            None
        )
        .is_err());
        assert_eq!(
            parse_selection(
                &json!({"id":"a", "name":"Photos", "mimeType":FOLDER_MIME, "trashed":false}),
                None
            )
            .unwrap()
            .id,
            "a"
        );
    }
}
