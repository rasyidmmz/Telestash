//! Watch analytics aggregation from the local SQLite watch history.
//! TMDB-backed file metadata was removed in v1.6.3; this module keeps only
//! the analytics commands that read `watch_history` + MPV resume positions.

use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbConnection;

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
pub fn cmd_watch_analytics(
    app_handle: tauri::AppHandle,
    db_pool: State<'_, DbConnection>,
) -> Result<WatchAnalytics, String> {
    let conn = db_pool.lock().map_err(|e| e.to_string())?;

    // One row per file, so COUNT(*) is the number of distinct titles.
    let unique_titles: i64 = conn
        .prepare("SELECT COUNT(*) FROM watch_history;")
        .map_err(|e| e.to_string())?
        .read::<i64, _>(0)
        .map_err(|e| e.to_string())?;

    // MPV watch-later holds the accurate positions; history rows only carry
    // them when a record call happened to pass one. Combine both sources,
    // keyed per (folder, message) so identical message ids in different
    // folders cannot borrow each other's position.
    let resume_secs: std::collections::HashMap<(Option<i64>, i32), f64> =
        crate::commands::resume::collect_resume_positions(&app_handle)
            .into_iter()
            .map(|p| ((p.folder_id, p.message_id), p.seconds))
            .collect();

    // Watch time = furthest position actually reached, times the number of
    // plays. total_duration is deliberately excluded: it is the file length,
    // not time watched, and would inflate a file opened for one second.
    let mut total_watch_secs: f64 = 0.0;
    let mut stmt = conn
        .prepare("SELECT file_id, folder_id, last_position_secs, play_count FROM watch_history;")
        .map_err(|e| e.to_string())?;
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let file_id = stmt.read::<i64, _>("file_id").unwrap_or(0) as i32;
        let folder_raw = stmt.read::<i64, _>("folder_id").unwrap_or(-1);
        let folder_id = if folder_raw < 0 { None } else { Some(folder_raw) };
        let last_pos = stmt.read::<f64, _>("last_position_secs").unwrap_or(0.0);
        let plays = stmt.read::<i64, _>("play_count").unwrap_or(1).max(1) as f64;
        let mpv = resume_secs.get(&(folder_id, file_id)).copied().unwrap_or(0.0);
        total_watch_secs += mpv.max(last_pos) * plays;
    }
    drop(stmt);

    // Day buckets follow the user's local calendar; grouping in UTC would put
    // an evening play in Jakarta (UTC+7) on the wrong day.
    let today = chrono::Local::now().date_naive();
    let cutoff = chrono::Local
        .from_local_datetime(&(today - chrono::Duration::days(29)).and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);

    let mut stmt = conn
        .prepare(&format!(
            "SELECT timestamp, play_count FROM watch_history WHERE timestamp >= {cutoff};"
        ))
        .map_err(|e| e.to_string())?;
    let mut per_day: std::collections::HashMap<chrono::NaiveDate, i64> = std::collections::HashMap::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let ts = stmt.read::<i64, _>("timestamp").map_err(|e| e.to_string())?;
        let plays = stmt.read::<i64, _>("play_count").unwrap_or(1).max(1);
        let date = chrono::DateTime::from_timestamp_millis(ts)
            .map(|d| d.with_timezone(&chrono::Local).date_naive())
            .unwrap_or(today);
        *per_day.entry(date).or_insert(0) += plays;
    }
    drop(stmt);

    let mut activity_30d = Vec::new();
    for i in (0..30).rev() {
        let day = today - chrono::Duration::days(i);
        activity_30d.push(ActivityDay {
            day: chrono::Local
                .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .map(|dt| dt.timestamp_millis())
                .unwrap_or(0),
            plays: *per_day.get(&day).unwrap_or(&0),
        });
    }

    // play_count is per file; a series shares a title across episodes, so sum
    // the counts rather than counting rows (which was always 1 per file).
    let mut top_titles = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT file_name, MIN(folder_id) AS folder_id, SUM(play_count) AS plays, MAX(timestamp) AS last_watched
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
    drop(stmt);

    // Current streak: walk back from today; if nothing today, allow starting at
    // yesterday. Bounded by the 30-day window `per_day` covers.
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
