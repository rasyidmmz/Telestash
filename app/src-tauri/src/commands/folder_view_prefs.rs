//! Per-folder sort preferences so FileExplorer restores sort after remount/folder switch.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbConnection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderViewPrefs {
    pub sort_field: String,
    pub sort_direction: String,
}

impl Default for FolderViewPrefs {
    fn default() -> Self {
        Self {
            sort_field: "name".to_string(),
            sort_direction: "asc".to_string(),
        }
    }
}

fn folder_key(folder_id: Option<i64>) -> String {
    match folder_id {
        Some(id) => id.to_string(),
        None => "home".to_string(),
    }
}

fn sanitize_field(field: &str) -> String {
    match field {
        "size" | "date" => field.to_string(),
        _ => "name".to_string(),
    }
}

fn sanitize_direction(dir: &str) -> String {
    match dir {
        "desc" => "desc".to_string(),
        _ => "asc".to_string(),
    }
}

#[tauri::command]
pub fn cmd_get_folder_view_prefs(
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
) -> Result<FolderViewPrefs, String> {
    let key = folder_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT sort_field, sort_direction FROM folder_view_prefs WHERE folder_key = ?")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let sort_field = stmt
            .read::<String, _>("sort_field")
            .map(|v| sanitize_field(&v))
            .unwrap_or_else(|_| "name".to_string());
        let sort_direction = stmt
            .read::<String, _>("sort_direction")
            .map(|v| sanitize_direction(&v))
            .unwrap_or_else(|_| "asc".to_string());
        Ok(FolderViewPrefs {
            sort_field,
            sort_direction,
        })
    } else {
        Ok(FolderViewPrefs::default())
    }
}

#[tauri::command]
pub fn cmd_set_folder_view_prefs(
    folder_id: Option<i64>,
    sort_field: String,
    sort_direction: String,
    db_pool: State<'_, DbConnection>,
) -> Result<(), String> {
    let key = folder_key(folder_id);
    let field = sanitize_field(&sort_field);
    let dir = sanitize_direction(&sort_direction);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "INSERT INTO folder_view_prefs (folder_key, sort_field, sort_direction) VALUES (?1, ?2, ?3)
             ON CONFLICT(folder_key) DO UPDATE SET sort_field = excluded.sort_field, sort_direction = excluded.sort_direction;",
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, field.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((3, dir.as_str())).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_key_uses_home_sentinel() {
        assert_eq!(super::folder_key(None), "home");
        assert_eq!(super::folder_key(Some(-100123)), "-100123");
    }

    #[test]
    fn sanitizes_sort_values() {
        assert_eq!(super::sanitize_field("size"), "size");
        assert_eq!(super::sanitize_field("date"), "date");
        assert_eq!(super::sanitize_field("evil;drop"), "name");
        assert_eq!(super::sanitize_direction("desc"), "desc");
        assert_eq!(super::sanitize_direction("sideways"), "asc");
    }
}
