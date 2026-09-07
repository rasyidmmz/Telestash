//! File content metadata (TMDB-backed, opt-in) stored in the `file_metadata`
//! table. The frontend performs the actual TMDB HTTP requests (webview fetch)
//! and pushes parsed results here through `cmd_upsert_file_metadata`; the
//! backend only persists rows and caches poster bytes fetched by the UI, so
//! no HTTP client dependency is added to the Rust side.

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::db::DbConnection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadataRow {
    pub folder_id: Option<i64>,
    pub message_id: i32,
    pub media_type: String,
    pub tmdb_id: Option<i64>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub rating: Option<f64>,
    pub genres_json: Option<String>,
    pub poster_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchAnalytics {
    /// Unique titles watched, all time.
    pub unique_titles: i64,
    /// Sum of known durations of watched items, seconds (MPV-accurate when available).
    pub total_watch_secs: f64,
    /// Playback events in the last 30 days (per day, oldest first).
    pub activity_30d: Vec<ActivityDay>,
    pub top_titles: Vec<TopTitle>,
    /// Consecutive days with activity ending today (or yesterday if no play today).
    pub current_streak: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityDay {
    /// Unix epoch millis of the day start (UTC).
    pub day: i64,
    pub plays: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTitle {
    pub file_name: String,
    pub folder_id: Option<i64>,
    pub plays: i64,
    pub last_watched: i64,
}

#[tauri::command]
pub fn cmd_upsert_file_metadata(entry: FileMetadataRow, db_pool: State<'_, DbConnection>) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn
        .prepare(
            "INSERT INTO file_metadata
                (folder_id, message_id, media_type, tmdb_id, title, original_title, year, overview, rating, genres_json, poster_path, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(folder_id, message_id) DO UPDATE SET
                media_type = excluded.media_type,
                tmdb_id = excluded.tmdb_id,
                title = excluded.title,
                original_title = excluded.original_title,
                year = excluded.year,
                overview = excluded.overview,
                rating = excluded.rating,
                genres_json = excluded.genres_json,
                poster_path = excluded.poster_path,
                updated_at = excluded.updated_at;",
        )
        .map_err(|e| e.to_string())?;
    bind_metadata(&mut stmt, &entry, now)?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

fn bind_metadata(stmt: &mut sqlite::Statement, entry: &FileMetadataRow, now: i64) -> Result<(), String> {
    stmt.bind((1, entry.folder_id.unwrap_or(-1))).map_err(|e| e.to_string())?;
    stmt.bind((2, entry.message_id as i64)).map_err(|e| e.to_string())?;
    stmt.bind((3, entry.media_type.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((4, entry.tmdb_id.unwrap_or(-1))).map_err(|e| e.to_string())?;
    stmt.bind((5, entry.title.as_str())).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<&str>)>((6, entry.original_title.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind((7, entry.year.map(i64::from).unwrap_or(-1))).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<&str>)>((8, entry.overview.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind((9, entry.rating.unwrap_or(-1.0))).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<&str>)>((10, entry.genres_json.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<&str>)>((11, entry.poster_path.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind((12, now)).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cmd_get_file_metadata(
    message_id: i32,
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
) -> Result<Option<FileMetadataRow>, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT folder_id, message_id, media_type, tmdb_id, title, original_title, year,
                    overview, rating, genres_json, poster_path
             FROM file_metadata
             WHERE folder_id = {} AND message_id = ?1 LIMIT 1;",
            folder_id.unwrap_or(-1)
        ))
        .map_err(|e| e.to_string())?;
    stmt.bind((1, message_id as i64)).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        return Ok(Some(read_metadata_row(&mut stmt)));
    }
    Ok(None)
}

#[tauri::command]
pub fn cmd_list_file_metadata(
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
) -> Result<Vec<FileMetadataRow>, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT folder_id, message_id, media_type, tmdb_id, title, original_title, year,
                    overview, rating, genres_json, poster_path
             FROM file_metadata WHERE folder_id = {} ORDER BY updated_at DESC;",
            folder_id.unwrap_or(-1)
        ))
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        rows.push(read_metadata_row(&mut stmt));
    }
    Ok(rows)
}

fn read_metadata_row(stmt: &mut sqlite::Statement) -> FileMetadataRow {
    let folder_raw = stmt.read::<i64, _>("folder_id").unwrap_or(-1);
    let tmdb_raw = stmt.read::<i64, _>("tmdb_id").unwrap_or(-1);
    let year_raw = stmt.read::<i64, _>("year").unwrap_or(-1);
    let rating_raw = stmt.read::<f64, _>("rating").unwrap_or(-1.0);
    FileMetadataRow {
        folder_id: if folder_raw < 0 { None } else { Some(folder_raw) },
        message_id: stmt.read::<i64, _>("message_id").unwrap_or(0) as i32,
        media_type: stmt.read::<String, _>("media_type").unwrap_or_default(),
        tmdb_id: if tmdb_raw < 0 { None } else { Some(tmdb_raw) },
        title: stmt.read::<String, _>("title").unwrap_or_default(),
        original_title: stmt.read::<Option<String>, _>("original_title").ok().flatten(),
        year: if year_raw < 0 { None } else { Some(year_raw as i32) },
        overview: stmt.read::<Option<String>, _>("overview").ok().flatten(),
        rating: if rating_raw < 0.0 { None } else { Some(rating_raw) },
        genres_json: stmt.read::<Option<String>, _>("genres_json").ok().flatten(),
        poster_path: stmt.read::<Option<String>, _>("poster_path").ok().flatten(),
    }
}

#[tauri::command]
pub fn cmd_delete_file_metadata(
    message_id: i32,
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "DELETE FROM file_metadata WHERE folder_id = {} AND message_id = ?1;",
            folder_id.unwrap_or(-1)
        ))
        .map_err(|e| e.to_string())?;
    stmt.bind((1, message_id as i64)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

/// Persist a poster image fetched by the frontend into the local cache dir and
/// record its path on the metadata row. Bytes come as base64 JPEG.
#[tauri::command]
pub async fn cmd_save_tmdb_poster(
    message_id: i32,
    folder_id: Option<i64>,
    base64_jpeg: String,
    app_handle: tauri::AppHandle,
    db_pool: State<'_, DbConnection>,
) -> Result<(), String> {
    use base64::{Engine as _, engine::general_purpose};

    let bytes = general_purpose::STANDARD
        .decode(base64_jpeg.as_bytes())
        .map_err(|e| format!("Invalid poster payload: {e}"))?;
    if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err("Poster payload outside accepted size".to_string());
    }

    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?
        .join("tmdb_posters");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let folder_key = folder_id.map(|id| id.to_string()).unwrap_or_else(|| "home".to_string());
    let path = dir.join(format!("{folder_key}_{message_id}.jpg"));
    let tmp = dir.join(format!("{folder_key}_{message_id}.jpg.part"));
    tokio::fs::write(&tmp, &bytes).await.map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp, &path).await.map_err(|e| e.to_string())?;

    let rel = path.file_name().and_then(|n| n.to_str()).map(|s| s.to_string());
    set_poster_path_sync(&db_pool, folder_id, message_id, rel)?;

    // Keep the poster cache bounded (200 MB / 4000 files), LRU by mtime.
    prune_posters(&dir).await;
    Ok(())
}

/// Locks the DB pool briefly; called from async context only after every
/// await point, so the command's future stays `Send`.
fn set_poster_path_sync(
    db_pool: &State<'_, DbConnection>,
    folder_id: Option<i64>,
    message_id: i32,
    rel: Option<String>,
) -> Result<(), String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!(
            "UPDATE file_metadata SET poster_path = ?2 WHERE folder_id = {} AND message_id = ?1;",
            folder_id.unwrap_or(-1)
        ))
        .map_err(|e| e.to_string())?;
    stmt.bind((1, message_id as i64)).map_err(|e| e.to_string())?;
    stmt.bind::<(usize, Option<&str>)>((2, rel.as_deref())).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

/// Serve a cached poster as a base64 data URL; empty string when absent.
#[tauri::command]
pub async fn cmd_get_tmdb_poster(
    message_id: i32,
    folder_id: Option<i64>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose};

    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?
        .join("tmdb_posters");
    let folder_key = folder_id.map(|id| id.to_string()).unwrap_or_else(|| "home".to_string());
    let path = dir.join(format!("{folder_key}_{message_id}.jpg"));
    match tokio::fs::read(&path).await {
        Ok(bytes) => Ok(format!(
            "data:image/jpeg;base64,{}",
            general_purpose::STANDARD.encode(bytes)
        )),
        Err(_) => Ok(String::new()),
    }
}

async fn prune_posters(dir: &std::path::Path) {
    let dir = dir.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        const MAX_BYTES: u64 = 200 * 1024 * 1024;
        const MAX_FILES: usize = 4_000;
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut files: Vec<(std::path::PathBuf, std::time::SystemTime, u64)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    files.push((path, meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH), meta.len()));
                }
            }
        }
        files.sort_by_key(|(_, modified, _)| *modified);
        let mut total: u64 = files.iter().map(|(_, _, len)| *len).sum();
        while files.len() > MAX_FILES || total > MAX_BYTES {
            if let Some((path, _, len)) = files.first().cloned() {
                let _ = std::fs::remove_file(&path);
                total = total.saturating_sub(len);
                files.remove(0);
            } else {
                break;
            }
        }
    })
    .await;
}

#[tauri::command]
pub fn cmd_watch_analytics(
    app_handle: tauri::AppHandle,
    db_pool: State<'_, DbConnection>,
) -> Result<WatchAnalytics, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;

    let unique_titles: i64 = conn
        .prepare("SELECT COUNT(*) FROM watch_history;")
        .map_err(|e| e.to_string())?
        .read::<i64, _>(0)
        .map_err(|e| e.to_string())?;

    // MPV watch-later holds the accurate positions; history rows only carry
    // them when a record call happened to pass one. Combine both sources.
    let resume_secs: std::collections::HashMap<i32, f64> = crate::commands::resume::collect_resume_positions(&app_handle)
        .into_iter()
        .map(|p| (p.message_id, p.seconds))
        .collect();

    let mut total_watch_secs: f64 = 0.0;
    let mut stmt = conn
        .prepare(
            "SELECT message_id, last_position_secs, total_duration_secs FROM watch_history;",
        )
        .map_err(|e| e.to_string())?;
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let message_id = stmt.read::<i64, _>("message_id").unwrap_or(0) as i32;
        let last_pos = stmt.read::<f64, _>("last_position_secs").unwrap_or(0.0);
        let duration = stmt.read::<f64, _>("total_duration_secs").unwrap_or(0.0);
        let mpv = resume_secs.get(&message_id).copied().unwrap_or(0.0);
        total_watch_secs += mpv.max(last_pos).max(duration);
    }

    let now = chrono::Utc::now();
    let today = now.date_naive();
    // History timestamps are stored as epoch MILLIS (frontend Date.now()).
    let cutoff = (today - chrono::Duration::days(29))
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();

    let mut stmt = conn
        .prepare(&format!(
            "SELECT timestamp, COUNT(*) FROM watch_history
             WHERE timestamp >= {cutoff} GROUP BY date(timestamp / 1000, 'unixepoch') ORDER BY date(timestamp / 1000, 'unixepoch');"
        ))
        .map_err(|e| e.to_string())?;
    let mut per_day: std::collections::HashMap<chrono::NaiveDate, i64> = std::collections::HashMap::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let ts = stmt.read::<i64, _>("timestamp").map_err(|e| e.to_string())?;
        let plays = stmt.read::<i64, _>(1).map_err(|e| e.to_string())?;
        let date = chrono::DateTime::from_timestamp_millis(ts)
            .map(|d| d.date_naive())
            .unwrap_or(today);
        *per_day.entry(date).or_insert(0) += plays;
    }
    let mut activity_30d = Vec::new();
    for i in (0..30).rev() {
        let day = today - chrono::Duration::days(i);
        activity_30d.push(ActivityDay {
            day: day.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis(),
            plays: *per_day.get(&day).unwrap_or(&0),
        });
    }

    let mut top_titles = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT file_name, folder_id, COUNT(*) AS plays, MAX(timestamp) AS last_watched
             FROM watch_history GROUP BY file_name ORDER BY plays DESC, last_watched DESC LIMIT 10;",
        )
        .map_err(|e| e.to_string())?;
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let folder_raw = stmt.read::<i64, _>("folder_id").unwrap_or(-1);
        top_titles.push(TopTitle {
            file_name: stmt.read::<String, _>("file_name").map_err(|e| e.to_string())?,
            folder_id: if folder_raw < 0 { None } else { Some(folder_raw) },
            plays: stmt.read::<i64, _>("plays").map_err(|e| e.to_string())?,
            last_watched: stmt.read::<i64, _>("last_watched").map_err(|e| e.to_string())?,
        });
    }

    // Current streak: walk back from today; if nothing today, allow starting at yesterday.
    let mut streak: i64 = 0;
    let mut cursor = if per_day.contains_key(&today) { today } else { today - chrono::Duration::days(1) };
    while per_day.get(&cursor).copied().unwrap_or(0) > 0 {
        streak += 1;
        cursor -= chrono::Duration::days(1);
    }

    Ok(WatchAnalytics {
        unique_titles,
        total_watch_secs,
        activity_30d,
        top_titles,
        current_streak: streak,
    })
}
