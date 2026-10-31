//! Persistent watch history stored in SQLite (`watch_history` table).
//!
//! The frontend keeps its own in-memory view; these commands are the durable
//! store so history survives webview storage clears. Rows are identified by
//! folder and file together; write-heavy calls are fire-and-forget from the UI.

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

const WATCH_HISTORY_PROBE_SQL: &str = r#"
    SELECT timestamp FROM watch_history WHERE file_id = ?1 AND folder_key = ?2;
"#;
const WATCH_HISTORY_UPSERT_SQL: &str = r#"
    INSERT INTO watch_history
        (file_id, file_name, folder_id, file_size, timestamp, status, quality_tag,
         last_position_secs, total_duration_secs, folder_key, play_count)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1)
    ON CONFLICT(folder_key, file_id) DO UPDATE SET
        file_name = excluded.file_name, folder_id = excluded.folder_id,
        file_size = excluded.file_size, timestamp = excluded.timestamp, status = excluded.status,
        quality_tag = COALESCE(excluded.quality_tag, watch_history.quality_tag),
        last_position_secs = COALESCE(excluded.last_position_secs, watch_history.last_position_secs),
        total_duration_secs = COALESCE(excluded.total_duration_secs, watch_history.total_duration_secs),
        play_count = watch_history.play_count + ?11;
"#;
const WATCH_HISTORY_LIST_SQL: &str = r#"
    SELECT file_id, file_name, folder_id, file_size, timestamp, status, quality_tag,
           last_position_secs, total_duration_secs, folder_key
    FROM watch_history ORDER BY timestamp DESC LIMIT ?1;
"#;
const WATCH_HISTORY_DELETE_SQL: &str = r#"
    DELETE FROM watch_history WHERE file_id = ?1 AND folder_key = ?2;
"#;
const WATCH_HISTORY_IMPORT_SQL: &str = r#"
    INSERT OR IGNORE INTO watch_history
        (file_id, file_name, folder_id, file_size, timestamp, status, quality_tag,
         last_position_secs, total_duration_secs, folder_key)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
"#;
const WATCH_HISTORY_TRIM_SQL: &str = r#"
    DELETE FROM watch_history WHERE rowid NOT IN (
        SELECT rowid FROM watch_history ORDER BY timestamp DESC LIMIT ?1
    );
"#;

pub(crate) fn folder_key(folder_id: Option<i64>) -> String {
    folder_id.map(|id| id.to_string()).unwrap_or_else(|| "home".to_string())
}

fn trim_history(conn: &sqlite::Connection) -> Result<(), String> {
    let mut stmt = conn.prepare(WATCH_HISTORY_TRIM_SQL).map_err(|e| e.to_string())?;
    stmt.bind((1, HISTORY_CAP)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

/// A repeat play of the same file within this window counts as one play, so the
/// two record paths (MPV launch in the UI and the streaming server's
/// `stream-playback-started` event) cannot double-count a single session.
const PLAY_DEDUPE_MILLIS: i64 = 60_000;

fn bind_entry(stmt: &mut sqlite::Statement, entry: &WatchHistoryRow) -> Result<(), String> {
    stmt.bind((1, entry.file_id)).map_err(|e| e.to_string())?;
    stmt.bind((2, entry.file_name.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((3, entry.folder_id.unwrap_or(-1))).map_err(|e| e.to_string())?;
    stmt.bind((4, entry.file_size)).map_err(|e| e.to_string())?;
    stmt.bind((5, entry.timestamp)).map_err(|e| e.to_string())?;
    stmt.bind((6, entry.status.as_str())).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<&str>)>((7, entry.quality_tag.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<f64>)>((8, entry.last_position_secs)).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<f64>)>((9, entry.total_duration_secs)).map_err(|e| e.to_string())?;
    stmt.bind((10, folder_key(entry.folder_id).as_str())).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cmd_watch_history_upsert(entry: WatchHistoryRow, db_pool: State<'_, DbConnection>) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;

    // Only a genuinely new session increments play_count; a re-record within the
    // dedupe window (or a progress update) leaves the counter alone.
    let mut probe = conn
        .prepare(WATCH_HISTORY_PROBE_SQL)
        .map_err(|e| e.to_string())?;
    probe.bind((1, entry.file_id)).map_err(|e| e.to_string())?;
    probe.bind((2, folder_key(entry.folder_id).as_str())).map_err(|e| e.to_string())?;
    let previous_ts: Option<i64> = if let sqlite::State::Row = probe.next().map_err(|e| e.to_string())? {
        Some(probe.read::<i64, _>("timestamp").map_err(|e| e.to_string())?)
    } else {
        None
    };
    drop(probe);
    let counts_as_new_play = match previous_ts {
        None => true,
        Some(prev) => entry.timestamp.saturating_sub(prev) > PLAY_DEDUPE_MILLIS,
    };
    let play_increment: i64 = if counts_as_new_play { 1 } else { 0 };

    let mut stmt = conn.prepare(WATCH_HISTORY_UPSERT_SQL).map_err(|e| e.to_string())?;
    bind_entry(&mut stmt, &entry)?;
    stmt.bind((11, play_increment)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    drop(stmt);
    trim_history(&conn)
}

#[tauri::command]
pub fn cmd_watch_history_list(db_pool: State<'_, DbConnection>) -> Result<Vec<WatchHistoryRow>, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(WATCH_HISTORY_LIST_SQL).map_err(|e| e.to_string())?;
    stmt.bind((1, HISTORY_CAP)).map_err(|e| e.to_string())?;

    let mut rows = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let key = stmt.read::<String, _>("folder_key").map_err(|e| e.to_string())?;
        rows.push(WatchHistoryRow {
            file_id: stmt.read::<i64, _>("file_id").map_err(|e| e.to_string())?,
            file_name: stmt.read::<String, _>("file_name").map_err(|e| e.to_string())?,
            folder_id: if key == "home" { None } else { Some(key.parse::<i64>().map_err(|e| e.to_string())?) },
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
pub fn cmd_watch_history_remove(file_id: i64, folder_id: Option<i64>, db_pool: State<'_, DbConnection>) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(WATCH_HISTORY_DELETE_SQL)
        .map_err(|e| e.to_string())?;
    stmt.bind((1, file_id)).map_err(|e| e.to_string())?;
    stmt.bind((2, folder_key(folder_id).as_str())).map_err(|e| e.to_string())?;
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
/// Rows already present (same folder and file) are skipped so re-running is harmless.
#[tauri::command]
pub fn cmd_watch_history_import(
    entries: Vec<WatchHistoryRow>,
    db_pool: State<'_, DbConnection>,
) -> Result<u32, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut imported: u32 = 0;
    for entry in &entries {
        let mut probe = conn
            .prepare(WATCH_HISTORY_PROBE_SQL)
            .map_err(|e| e.to_string())?;
        probe.bind((1, entry.file_id)).map_err(|e| e.to_string())?;
        probe.bind((2, folder_key(entry.folder_id).as_str())).map_err(|e| e.to_string())?;
        let exists = matches!(probe.next().map_err(|e| e.to_string())?, sqlite::State::Row);
        if exists {
            continue;
        }
        let mut stmt = conn.prepare(WATCH_HISTORY_IMPORT_SQL).map_err(|e| e.to_string())?;
        bind_entry(&mut stmt, entry)?;
        stmt.next().map_err(|e| e.to_string())?;
        imported += 1;
    }
    trim_history(&conn)?;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::PLAY_DEDUPE_MILLIS;

    /// Mirrors the play-count decision in cmd_watch_history_upsert.
    fn counts_as_new_play(previous_ts: Option<i64>, new_ts: i64) -> bool {
        match previous_ts {
            None => true,
            Some(prev) => new_ts.saturating_sub(prev) > PLAY_DEDUPE_MILLIS,
        }
    }

    #[test]
    fn first_record_for_a_file_is_always_a_new_play() {
        assert!(counts_as_new_play(None, 1_700_000_000_000));
    }

    #[test]
    fn the_two_record_paths_of_one_launch_count_once() {
        // MediaPlayer records, then the streaming server event arrives ~2s later.
        let first = 1_700_000_000_000;
        assert!(!counts_as_new_play(Some(first), first + 2_000));
    }

    #[test]
    fn rewatching_later_counts_as_another_play() {
        let first = 1_700_000_000_000;
        assert!(counts_as_new_play(Some(first), first + PLAY_DEDUPE_MILLIS + 1));
    }
}
