//! Persistent watch history stored in SQLite (`watch_history` table).
//!
//! The frontend keeps its own in-memory view; these commands are the durable
//! store so history survives webview storage clears. One row per file
//! (file_id is the primary key, matching the frontend's upsert-by-file-id
//! semantics); write-heavy calls are fire-and-forget from the UI.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbConnection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchHistoryRow {
    pub file_id: i64,
    pub file_name: String,
    pub folder_id: Option<i64>,
    pub file_size: i64,
    /// Unix epoch millis.
    pub timestamp: i64,
    pub status: String,
    pub quality_tag: Option<String>,
    pub last_position_secs: Option<f64>,
    pub total_duration_secs: Option<f64>,
}

const HISTORY_CAP: i64 = 200;

fn bind_entry(stmt: &mut sqlite::Statement, entry: &WatchHistoryRow) -> Result<(), String> {
    stmt.bind((1, entry.file_id)).map_err(|e| e.to_string())?;
    stmt.bind((2, entry.file_name.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((3, entry.folder_id.unwrap_or(-1))).map_err(|e| e.to_string())?;
    stmt.bind((4, entry.file_size)).map_err(|e| e.to_string())?;
    stmt.bind((5, entry.timestamp)).map_err(|e| e.to_string())?;
    stmt.bind((6, entry.status.as_str())).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<&str>)>((7, entry.quality_tag.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind((8, entry.last_position_secs.unwrap_or(0.0))).map_err(|e| e.to_string())?;
    stmt.bind((9, entry.total_duration_secs.unwrap_or(0.0))).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cmd_watch_history_upsert(entry: WatchHistoryRow, db_pool: State<'_, DbConnection>) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "INSERT INTO watch_history
                (file_id, file_name, folder_id, file_size, timestamp, status, quality_tag, last_position_secs, total_duration_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(file_id) DO UPDATE SET
                file_name = excluded.file_name,
                folder_id = excluded.folder_id,
                file_size = excluded.file_size,
                timestamp = excluded.timestamp,
                status = excluded.status,
                quality_tag = COALESCE(excluded.quality_tag, watch_history.quality_tag),
                last_position_secs = COALESCE(excluded.last_position_secs, watch_history.last_position_secs),
                total_duration_secs = COALESCE(excluded.total_duration_secs, watch_history.total_duration_secs);",
        )
        .map_err(|e| e.to_string())?;
    bind_entry(&mut stmt, &entry)?;
    stmt.next().map_err(|e| e.to_string())?;

    // Keep the table bounded the same way the old localStorage store did.
    conn.execute(&format!(
        "DELETE FROM watch_history WHERE file_id NOT IN (
            SELECT file_id FROM watch_history ORDER BY timestamp DESC LIMIT {HISTORY_CAP}
        );"
    ))
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cmd_watch_history_list(db_pool: State<'_, DbConnection>) -> Result<Vec<WatchHistoryRow>, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT file_id, file_name, folder_id, file_size, timestamp, status,
                    quality_tag, last_position_secs, total_duration_secs
             FROM watch_history ORDER BY timestamp DESC LIMIT {HISTORY_CAP};"
        ))
        .map_err(|e| e.to_string())?;

    let mut rows = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let folder_id_raw = stmt.read::<i64, _>("folder_id").map_err(|e| e.to_string())?;
        rows.push(WatchHistoryRow {
            file_id: stmt.read::<i64, _>("file_id").map_err(|e| e.to_string())?,
            file_name: stmt.read::<String, _>("file_name").map_err(|e| e.to_string())?,
            // -1 encodes "no folder" (Saved Messages); convert back to None.
            folder_id: if folder_id_raw < 0 { None } else { Some(folder_id_raw) },
            file_size: stmt.read::<i64, _>("file_size").map_err(|e| e.to_string())?,
            timestamp: stmt.read::<i64, _>("timestamp").map_err(|e| e.to_string())?,
            status: stmt.read::<String, _>("status").map_err(|e| e.to_string())?,
            quality_tag: stmt.read::<Option<String>, _>("quality_tag").ok().flatten(),
            last_position_secs: stmt.read::<Option<f64>, _>("last_position_secs").ok().flatten(),
            total_duration_secs: stmt.read::<Option<f64>, _>("total_duration_secs").ok().flatten(),
        });
    }
    Ok(rows)
}

#[tauri::command]
pub fn cmd_watch_history_remove(file_id: i64, db_pool: State<'_, DbConnection>) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("DELETE FROM watch_history WHERE file_id = ?1")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, file_id)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cmd_watch_history_clear(db_pool: State<'_, DbConnection>) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM watch_history").map_err(|e| e.to_string())?;
    Ok(())
}

/// Bulk insert used by the one-time localStorage → SQLite migration.
/// Rows already present (same file_id) are skipped so re-running is harmless.
#[tauri::command]
pub fn cmd_watch_history_import(
    entries: Vec<WatchHistoryRow>,
    db_pool: State<'_, DbConnection>,
) -> Result<u32, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut imported: u32 = 0;
    for entry in &entries {
        let mut probe = conn
            .prepare("SELECT 1 FROM watch_history WHERE file_id = ?1")
            .map_err(|e| e.to_string())?;
        probe.bind((1, entry.file_id)).map_err(|e| e.to_string())?;
        let exists = matches!(probe.next().map_err(|e| e.to_string())?, sqlite::State::Row);
        if exists {
            continue;
        }
        let mut stmt = conn
            .prepare(
                "INSERT INTO watch_history
                    (file_id, file_name, folder_id, file_size, timestamp, status, quality_tag, last_position_secs, total_duration_secs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);",
            )
            .map_err(|e| e.to_string())?;
        bind_entry(&mut stmt, entry)?;
        stmt.next().map_err(|e| e.to_string())?;
        imported += 1;
    }
    Ok(imported)
}
