use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResumePosition {
    pub folder_id: Option<i64>,
    pub message_id: i32,
    pub seconds: f64,
}

/// Read MPV watch-later files and extract the exact playback position per
/// stream URL (`# filename: .../stream/{folder}/{msg}` + `start=SECONDS`).
#[tauri::command]
pub fn cmd_list_resume_positions(app_handle: tauri::AppHandle) -> Result<Vec<ResumePosition>, String> {
    let dir: PathBuf = app_handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("mpv-watch-later");

    let mut out: Vec<ResumePosition> = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }

    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let Ok(content) = std::fs::read_to_string(entry.path()) else { continue };
        let mut folder_id: Option<i64> = None;
        let mut message_id: Option<i32> = None;
        let mut seconds: Option<f64> = None;

        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("# filename: ") {
                // .../stream/{folder|"home"}/{message_id}
                if let Some(idx) = rest.find("/stream/") {
                    let parts: Vec<&str> = rest[idx + 8..].split('/').collect();
                    if parts.len() >= 2 {
                        folder_id = parts[0].parse::<i64>().ok();
                        message_id = parts[1].split('?').next().and_then(|m| m.parse::<i32>().ok());
                    }
                }
            } else if let Some(rest) = line.strip_prefix("start=") {
                seconds = rest.trim().parse::<f64>().ok();
            }
        }

        if let (Some(fid), Some(mid), Some(secs)) = (folder_id, message_id, seconds) {
            if secs > 0.5 {
                out.push(ResumePosition { folder_id: Some(fid), message_id: mid, seconds: secs });
            } else if folder_id.is_none() {
                // "home" streams store folder_id as absent; message_id still parsed
                if let Some(mid) = message_id {
                    if secs > 0.5 {
                        out.push(ResumePosition { folder_id: None, message_id: mid, seconds: secs });
                    }
                }
            }
        }
    }

    Ok(out)
}
