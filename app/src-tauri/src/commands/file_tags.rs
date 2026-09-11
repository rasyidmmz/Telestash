//! Manual per-file tags. Identity (folder_key, message_id) per ADR 0003.
//! Distinct from watch_history.quality_tag (media quality badge).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

use crate::db::DbConnection;
use crate::commands::favorites::folder_key;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTag {
    pub id: i64,
    pub name: String,
}

fn sanitize_tag_name(name: &str) -> String {
    name.trim().chars().take(40).collect::<String>().trim().to_string()
}

#[tauri::command]
pub fn cmd_list_tags(db_pool: State<'_, DbConnection>) -> Result<Vec<FileTag>, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name FROM file_tags ORDER BY name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let mut tags = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        tags.push(FileTag {
            id: stmt.read::<i64, _>("id").map_err(|e| e.to_string())?,
            name: stmt.read::<String, _>("name").map_err(|e| e.to_string())?,
        });
    }
    Ok(tags)
}

#[tauri::command]
pub fn cmd_create_tag(name: String, db_pool: State<'_, DbConnection>) -> Result<FileTag, String> {
    let name = sanitize_tag_name(&name);
    if name.is_empty() {
        return Err("Tag name is empty".to_string());
    }
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("INSERT INTO file_tags (name) VALUES (?1) ON CONFLICT(name) DO NOTHING;")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, name.as_str())).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    drop(stmt);
    let mut stmt = conn
        .prepare("SELECT id, name FROM file_tags WHERE name = ?1")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, name.as_str())).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        Ok(FileTag {
            id: stmt.read::<i64, _>("id").map_err(|e| e.to_string())?,
            name: stmt.read::<String, _>("name").map_err(|e| e.to_string())?,
        })
    } else {
        Err("Failed to create tag".to_string())
    }
}

#[tauri::command]
pub fn cmd_delete_tag(tag_id: i64, db_pool: State<'_, DbConnection>) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("DELETE FROM file_tags WHERE id = ?1")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, tag_id)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

/// Set the full tag list for one file (replaces links).
#[tauri::command]
pub fn cmd_set_file_tags(
    folder_id: Option<i64>,
    message_id: i32,
    tag_ids: Vec<i64>,
    db_pool: State<'_, DbConnection>,
) -> Result<(), String> {
    let key = folder_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;

    let mut del = conn
        .prepare("DELETE FROM file_tag_links WHERE folder_key = ?1 AND message_id = ?2")
        .map_err(|e| e.to_string())?;
    del.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    del.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
    del.next().map_err(|e| e.to_string())?;
    drop(del);

    for tag_id in tag_ids {
        let mut ins = conn
            .prepare(
                "INSERT OR IGNORE INTO file_tag_links (folder_key, message_id, tag_id) VALUES (?1, ?2, ?3);",
            )
            .map_err(|e| e.to_string())?;
        ins.bind((1, key.as_str())).map_err(|e| e.to_string())?;
        ins.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
        ins.bind((3, tag_id)).map_err(|e| e.to_string())?;
        ins.next().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// message_id → tag names for one folder (for chips/badges).
#[tauri::command]
pub fn cmd_get_folder_tag_map(
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
) -> Result<HashMap<i64, Vec<String>>, String> {
    let key = folder_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT l.message_id, t.name FROM file_tag_links l
             JOIN file_tags t ON t.id = l.tag_id
             WHERE l.folder_key = ?1
             ORDER BY t.name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let mid = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())?;
        let name = stmt.read::<String, _>("name").map_err(|e| e.to_string())?;
        map.entry(mid).or_default().push(name);
    }
    Ok(map)
}

/// Tag ids assigned to one file.
#[tauri::command]
pub fn cmd_get_file_tag_ids(
    folder_id: Option<i64>,
    message_id: i32,
    db_pool: State<'_, DbConnection>,
) -> Result<Vec<i64>, String> {
    let key = folder_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT tag_id FROM file_tag_links WHERE folder_key = ?1 AND message_id = ?2")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        ids.push(stmt.read::<i64, _>("tag_id").map_err(|e| e.to_string())?);
    }
    Ok(ids)
}

pub(crate) fn purge_tags_for_file(
    conn: &sqlite::Connection,
    folder_id: Option<i64>,
    message_id: i32,
) -> Result<(), String> {
    let key = folder_key(folder_id);
    let mut stmt = conn
        .prepare("DELETE FROM file_tag_links WHERE folder_key = ?1 AND message_id = ?2")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, message_id as i64)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn sanitizes_tag_names() {
        assert_eq!(super::sanitize_tag_name("  Action  "), "Action");
        assert_eq!(super::sanitize_tag_name(""), "");
        let long = "x".repeat(100);
        assert_eq!(super::sanitize_tag_name(&long).len(), 40);
    }
}
