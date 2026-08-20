use sqlite::Connection;

#[derive(Debug, Clone)]
pub struct UploadCheckpoint {
    pub id: String,
    pub file_path: String,
    pub file_size: u64,
    pub modified_time: u64,
    pub telegram_file_id: i64,
    pub last_part_index: u32,
    pub total_parts: u32,
}

pub fn get_file_mtime(path_str: &str) -> u64 {
    std::fs::metadata(path_str)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn find_checkpoint(
    conn: &Connection,
    file_path: &str,
    file_size: u64,
    modified_time: u64,
) -> Option<UploadCheckpoint> {
    let mut stmt = conn
        .prepare(
            "SELECT id, telegram_file_id, last_part_index, total_parts FROM upload_checkpoints WHERE file_path = ? AND file_size = ? AND modified_time = ?",
        )
        .ok()?;

    stmt.bind((1, file_path)).ok()?;
    stmt.bind((2, file_size as i64)).ok()?;
    stmt.bind((3, modified_time as i64)).ok()?;

    if let Ok(sqlite::State::Row) = stmt.next() {
        let id = stmt.read::<String, _>("id").ok()?;
        let telegram_file_id = stmt.read::<i64, _>("telegram_file_id").ok()?;
        let last_part_index = stmt.read::<i64, _>("last_part_index").ok()? as u32;
        let total_parts = stmt.read::<i64, _>("total_parts").ok()? as u32;

        Some(UploadCheckpoint {
            id,
            file_path: file_path.to_string(),
            file_size,
            modified_time,
            telegram_file_id,
            last_part_index,
            total_parts,
        })
    } else {
        None
    }
}

pub fn save_checkpoint(
    conn: &Connection,
    checkpoint: &UploadCheckpoint,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO upload_checkpoints (id, file_path, file_size, modified_time, telegram_file_id, last_part_index, total_parts, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET last_part_index = excluded.last_part_index, updated_at = excluded.updated_at",
        )
        .map_err(|e| e.to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    stmt.bind((1, checkpoint.id.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, checkpoint.file_path.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((3, checkpoint.file_size as i64)).map_err(|e| e.to_string())?;
    stmt.bind((4, checkpoint.modified_time as i64)).map_err(|e| e.to_string())?;
    stmt.bind((5, checkpoint.telegram_file_id)).map_err(|e| e.to_string())?;
    stmt.bind((6, checkpoint.last_part_index as i64)).map_err(|e| e.to_string())?;
    stmt.bind((7, checkpoint.total_parts as i64)).map_err(|e| e.to_string())?;
    stmt.bind((8, now)).map_err(|e| e.to_string())?;

    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remove_checkpoint(conn: &Connection, id: &str) {
    if let Ok(mut stmt) = conn.prepare("DELETE FROM upload_checkpoints WHERE id = ?") {
        let _ = stmt.bind((1, id));
        let _ = stmt.next();
    }
}
