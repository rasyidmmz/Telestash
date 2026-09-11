//! Per-file favorites. Identity is (folder_key, message_id) per ADR 0003.
//! Split media favorites the manifest message, not individual parts.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbConnection;
use crate::models::FileMetadata;
use crate::TelegramState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteRef {
    pub folder_id: Option<i64>,
    pub message_id: i32,
}

pub(crate) fn folder_key(folder_id: Option<i64>) -> String {
    match folder_id {
        Some(id) => id.to_string(),
        None => "home".to_string(),
    }
}

fn parse_folder_key(key: &str) -> Option<i64> {
    if key == "home" {
        None
    } else {
        key.parse::<i64>().ok()
    }
}

/// Toggle a favorite. Returns the new state (true = now favorited).
#[tauri::command]
pub fn cmd_toggle_file_favorite(
    folder_id: Option<i64>,
    message_id: i32,
    db_pool: State<'_, DbConnection>,
) -> Result<bool, String> {
    let key = folder_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;

    let mut probe = conn
        .prepare("SELECT 1 FROM file_favorites WHERE folder_key = ?1 AND message_id = ?2")
        .map_err(|e| e.to_string())?;
    probe.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    probe.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
    let exists = matches!(probe.next().map_err(|e| e.to_string())?, sqlite::State::Row);
    drop(probe);

    if exists {
        let mut del = conn
            .prepare("DELETE FROM file_favorites WHERE folder_key = ?1 AND message_id = ?2")
            .map_err(|e| e.to_string())?;
        del.bind((1, key.as_str())).map_err(|e| e.to_string())?;
        del.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
        del.next().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        let now = chrono::Utc::now().timestamp();
        let mut ins = conn
            .prepare(
                "INSERT INTO file_favorites (folder_key, message_id, created_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(folder_key, message_id) DO NOTHING;",
            )
            .map_err(|e| e.to_string())?;
        ins.bind((1, key.as_str())).map_err(|e| e.to_string())?;
        ins.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
        ins.bind((3, now)).map_err(|e| e.to_string())?;
        ins.next().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// Message ids favorited in one folder (for filter ★ in that folder).
#[tauri::command]
pub fn cmd_list_folder_favorites(
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
) -> Result<Vec<i32>, String> {
    let key = folder_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT message_id FROM file_favorites WHERE folder_key = ?1 ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let id = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())?;
        ids.push(id as i32);
    }
    Ok(ids)
}

/// All favorite refs across folders (for the virtual "All Favorites" view).
#[tauri::command]
pub fn cmd_list_all_favorites(db_pool: State<'_, DbConnection>) -> Result<Vec<FavoriteRef>, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT folder_key, message_id FROM file_favorites ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let mut refs = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let key = stmt.read::<String, _>("folder_key").map_err(|e| e.to_string())?;
        let message_id = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())? as i32;
        refs.push(FavoriteRef {
            folder_id: parse_folder_key(&key),
            message_id,
        });
    }
    Ok(refs)
}

/// Resolve every favorite to FileMetadata for the "All Favorites" grid.
#[tauri::command]
pub async fn cmd_get_all_favorite_files(
    db_pool: State<'_, DbConnection>,
    state: State<'_, TelegramState>,
) -> Result<Vec<FileMetadata>, String> {
    let refs = {
        let conn = db_pool.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT folder_key, message_id FROM file_favorites ORDER BY created_at DESC")
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
            let key = stmt.read::<String, _>("folder_key").map_err(|e| e.to_string())?;
            let message_id = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())? as i32;
            out.push((parse_folder_key(&key), message_id));
        }
        out
    };

    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        return Ok(Vec::new());
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    // Group by folder to reuse peer resolution.
    let mut by_folder: std::collections::HashMap<String, Vec<i32>> = std::collections::HashMap::new();
    for (folder_id, message_id) in refs {
        by_folder
            .entry(folder_key(folder_id))
            .or_default()
            .push(message_id);
    }

    let mut files = Vec::new();
    for (key, ids) in by_folder {
        let folder_id = parse_folder_key(&key);
        let peer = match crate::commands::utils::resolve_peer(&client, folder_id, &state.peer_cache).await
        {
            Ok(p) => p,
            Err(_) => continue,
        };
        let messages = match client.get_messages_by_id(peer, &ids).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        for msg in messages.into_iter().flatten() {
            let Some(media) = msg.media() else { continue };
            let caption = msg.text();
            if crate::models::is_split_part_caption(caption) {
                continue;
            }
            if let grammers_client::media::Media::Document(d) = &media {
                if crate::models::is_split_part_filename(d.name().unwrap_or_default()) {
                    continue;
                }
            }
            if let Some(manifest) =
                crate::commands::fs::split_manifest_from_media(&client, &media, caption).await
            {
                files.push(FileMetadata {
                    id: msg.id() as i64,
                    folder_id,
                    name: manifest.filename,
                    size: manifest.size,
                    mime_type: Some(manifest.mime_type),
                    file_ext: manifest.file_ext,
                    created_at: msg.date().to_string(),
                    icon_type: "file".into(),
                });
                continue;
            }
            let (name, size, mime, ext) = match &media {
                grammers_client::media::Media::Document(d) => {
                    let doc_name = d.name().unwrap_or_default().to_string();
                    let display_name = if caption.is_empty() {
                        doc_name.clone()
                    } else {
                        caption.to_string()
                    };
                    let s = d.size().unwrap_or(0);
                    let m = d.mime_type().map(|s| s.to_string());
                    let e = std::path::Path::new(&doc_name)
                        .extension()
                        .and_then(|os| os.to_str())
                        .map(|s| s.to_string());
                    (display_name, s as u64, m, e)
                }
                grammers_client::media::Media::Photo(_) => (
                    "Photo.jpg".to_string(),
                    0u64,
                    Some("image/jpeg".into()),
                    Some("jpg".into()),
                ),
                _ => ("Unknown".to_string(), 0, None, None),
            };
            files.push(FileMetadata {
                id: msg.id() as i64,
                folder_id,
                name,
                size,
                mime_type: mime,
                file_ext: ext,
                created_at: msg.date().to_string(),
                icon_type: "file".into(),
            });
        }
    }
    Ok(files)
}

/// Drop favorite rows when a file is deleted (called from purge path).
pub(crate) fn purge_favorite_for_file(
    conn: &sqlite::Connection,
    folder_id: Option<i64>,
    message_id: i32,
) -> Result<(), String> {
    let key = folder_key(folder_id);
    let mut stmt = conn
        .prepare("DELETE FROM file_favorites WHERE folder_key = ?1 AND message_id = ?2")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_key_roundtrip() {
        assert_eq!(super::folder_key(None), "home");
        assert_eq!(super::parse_folder_key("home"), None);
        assert_eq!(super::folder_key(Some(42)), "42");
        assert_eq!(super::parse_folder_key("42"), Some(42));
    }
}
