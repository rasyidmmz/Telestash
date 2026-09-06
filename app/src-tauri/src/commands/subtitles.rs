use tauri::{Manager, State};
use serde::{Deserialize, Serialize};
use grammers_client::message::InputMessage;
use std::path::Path;
use crate::TelegramState;
use crate::db::DbConnection;
use crate::commands::utils::resolve_peer;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VideoSubtitleInfo {
    pub id: String,
    pub folder_id: Option<i64>,
    pub video_message_id: i64,
    pub subtitle_message_id: Option<i64>,
    pub format: String, // 'vobsub_idx', 'vobsub_sub', 'srt', 'ass', 'ssa', 'vtt'
    pub language: String,
    pub label: Option<String>,
    pub original_filename: String,
    pub is_paired_vobsub: bool,
    pub paired_message_id: Option<i64>,
    pub created_at: i64,
}

#[tauri::command]
pub async fn cmd_get_video_subtitles(
    folder_id: Option<i64>,
    video_message_id: i64,
    db: State<'_, DbConnection>,
) -> Result<Vec<VideoSubtitleInfo>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, folder_id, video_message_id, subtitle_message_id, format, language, label, original_filename, is_paired_vobsub, paired_message_id, created_at 
         FROM video_subtitles 
         WHERE (folder_id IS ? OR folder_id = ?) AND video_message_id = ?
         ORDER BY created_at ASC"
    ).map_err(|e| e.to_string())?;

    let folder_id_val = folder_id.unwrap_or(0);
    let mut subs = Vec::new();

    stmt.bind((1, folder_id)).map_err(|e| e.to_string())?;
    stmt.bind((2, folder_id_val)).map_err(|e| e.to_string())?;
    stmt.bind((3, video_message_id)).map_err(|e| e.to_string())?;

    while let Ok(sqlite::State::Row) = stmt.next() {
        let id: String = stmt.read(0).unwrap_or_default();
        let f_id: Option<i64> = stmt.read(1).ok();
        let v_id: i64 = stmt.read(2).unwrap_or_default();
        let s_id: Option<i64> = stmt.read(3).ok();
        let format: String = stmt.read(4).unwrap_or_default();
        let language: String = stmt.read(5).unwrap_or_default();
        let label: Option<String> = stmt.read(6).ok();
        let original_filename: String = stmt.read(7).unwrap_or_default();
        let is_paired_vobsub_int: i64 = stmt.read(8).unwrap_or(0);
        let paired_msg_id: Option<i64> = stmt.read(9).ok();
        let created_at: i64 = stmt.read(10).unwrap_or_default();

        subs.push(VideoSubtitleInfo {
            id,
            folder_id: f_id,
            video_message_id: v_id,
            subtitle_message_id: s_id,
            format,
            language,
            label,
            original_filename,
            is_paired_vobsub: is_paired_vobsub_int != 0,
            paired_message_id: paired_msg_id,
            created_at,
        });
    }

    Ok(subs)
}

#[tauri::command]
pub async fn cmd_attach_video_subtitles(
    folder_id: Option<i64>,
    video_message_id: i64,
    video_file_name: Option<String>,
    primary_path: String,
    paired_path: Option<String>,
    format: String,
    language: String,
    label: Option<String>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    db: State<'_, DbConnection>,
) -> Result<VideoSubtitleInfo, String> {
    let client = { state.client.lock().await.clone() }.ok_or("Telegram client not initialized")?;
    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;

    let primary_file_name = Path::new(&primary_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "subtitle.srt".to_string());

    // 1. Upload paired file first if it's VobSub (.sub)
    let mut paired_msg_id: Option<i64> = None;
    if let Some(ref p_path) = paired_path {
        if Path::new(p_path).exists() {
            let p_bytes = tokio::fs::read(p_path).await.map_err(|e| format!("Failed to read paired file: {}", e))?;
            let p_len = p_bytes.len();
            let p_name = Path::new(p_path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "subtitle.sub".to_string());

            let mut cursor = std::io::Cursor::new(p_bytes);
            let uploaded = client.upload_stream(&mut cursor, p_len, p_name.clone()).await
                .map_err(|e| format!("Failed to upload paired subtitle: {}", e))?;

            let caption = format!("#telestash_sub:{}:{}:vobsub_sub", video_message_id, language);
            let msg = client.send_message(peer, InputMessage::new().file(uploaded).text(caption)).await
                .map_err(|e| format!("Failed to send paired subtitle message: {}", e))?;
            paired_msg_id = Some(msg.id() as i64);
        }
    }

    // 2. Upload primary subtitle file (.idx or .srt or .ass or .vtt)
    let primary_bytes = tokio::fs::read(&primary_path).await.map_err(|e| format!("Failed to read subtitle file: {}", e))?;
    let primary_len = primary_bytes.len();
    let mut cursor = std::io::Cursor::new(primary_bytes.clone());
    let uploaded = client.upload_stream(&mut cursor, primary_len, primary_file_name.clone()).await
        .map_err(|e| format!("Failed to upload subtitle file: {}", e))?;

    let caption = format!("#telestash_sub:{}:{}:{}", video_message_id, language, format);
    let msg = client.send_message(peer, InputMessage::new().file(uploaded).text(caption)).await
        .map_err(|e| format!("Failed to send subtitle message: {}", e))?;
    let primary_msg_id = msg.id() as i64;

    // 3. Cache locally in streaming/captions for immediate playback
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let captions_dir = app_dir.join("streaming").join("captions");
        let _ = tokio::fs::create_dir_all(&captions_dir).await;

        let local_ext = match format.as_str() {
            "vobsub_idx" => "idx",
            "vobsub_sub" => "sub",
            "ass" => "ass",
            "ssa" => "ssa",
            "vtt" => "vtt",
            _ => "srt",
        };

        // Key 1: {folder_id}_{video_message_id}.{ext}
        let local_name = format!("{}_{}.{}", folder_id.unwrap_or(0), video_message_id, local_ext);
        let _ = tokio::fs::write(captions_dir.join(&local_name), &primary_bytes).await;

        // Key 2: {video_message_id}.{ext}
        let msg_name = format!("{}.{}", video_message_id, local_ext);
        let _ = tokio::fs::write(captions_dir.join(&msg_name), &primary_bytes).await;

        if let Some(ref p_path) = paired_path {
            if let Ok(p_bytes) = tokio::fs::read(p_path).await {
                let local_paired = format!("{}_{}.sub", folder_id.unwrap_or(0), video_message_id);
                let _ = tokio::fs::write(captions_dir.join(&local_paired), &p_bytes).await;
                let msg_paired = format!("{}.sub", video_message_id);
                let _ = tokio::fs::write(captions_dir.join(&msg_paired), &p_bytes).await;
            }
        }

        // Key 3: Exact video filename stem (for MPV fuzzy playlist auto-matching)
        if let Some(ref v_name) = video_file_name {
            if let Some(v_stem) = Path::new(v_name).file_stem().and_then(|s| s.to_str()) {
                if !v_stem.is_empty() {
                    let stem_name = format!("{}.{}", v_stem, local_ext);
                    let _ = tokio::fs::write(captions_dir.join(&stem_name), &primary_bytes).await;
                    if let Some(ref p_path) = paired_path {
                        if let Ok(p_bytes) = tokio::fs::read(p_path).await {
                            let paired_stem = format!("{}.sub", v_stem);
                            let _ = tokio::fs::write(captions_dir.join(&paired_stem), &p_bytes).await;
                        }
                    }
                }
            }
        }
    }

    // 4. Save to SQLite database
    let sub_id = format!("{}_{}_{}_{}", folder_id.unwrap_or(0), video_message_id, primary_msg_id, format);
    let now = chrono::Utc::now().timestamp();
    let is_paired = if paired_path.is_some() { 1 } else { 0 };

    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO video_subtitles 
             (id, folder_id, video_message_id, subtitle_message_id, format, language, label, original_filename, is_paired_vobsub, paired_message_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ).map_err(|e| e.to_string())?;

        stmt.bind((1, sub_id.as_str())).map_err(|e| e.to_string())?;
        stmt.bind((2, folder_id)).map_err(|e| e.to_string())?;
        stmt.bind((3, video_message_id)).map_err(|e| e.to_string())?;
        stmt.bind((4, Some(primary_msg_id))).map_err(|e| e.to_string())?;
        stmt.bind((5, format.as_str())).map_err(|e| e.to_string())?;
        stmt.bind((6, language.as_str())).map_err(|e| e.to_string())?;
        stmt.bind((7, label.as_deref())).map_err(|e| e.to_string())?;
        stmt.bind((8, primary_file_name.as_str())).map_err(|e| e.to_string())?;
        stmt.bind((9, is_paired)).map_err(|e| e.to_string())?;
        stmt.bind((10, paired_msg_id)).map_err(|e| e.to_string())?;
        stmt.bind((11, now)).map_err(|e| e.to_string())?;

        stmt.next().map_err(|e| e.to_string())?;
    }

    crate::transfer_log::record_transfer_success(
        "Subtitle Attach",
        format!("Subtitle {} attached to video message {}", primary_file_name, video_message_id),
        folder_id.map(|id| format!("folder_id: {}", id)),
    );

    Ok(VideoSubtitleInfo {
        id: sub_id,
        folder_id,
        video_message_id,
        subtitle_message_id: Some(primary_msg_id),
        format,
        language,
        label,
        original_filename: primary_file_name,
        is_paired_vobsub: is_paired != 0,
        paired_message_id: paired_msg_id,
        created_at: now,
    })
}

#[tauri::command]
pub async fn cmd_delete_video_subtitle(
    subtitle_id: String,
    folder_id: Option<i64>,
    video_file_name: Option<String>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    db: State<'_, DbConnection>,
) -> Result<bool, String> {
    // Read the row first so we know which Telegram sidecar messages to remove.
    let (row_folder_id, video_message_id, subtitle_msg_id, paired_msg_id) = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT folder_id, video_message_id, subtitle_message_id, paired_message_id \
             FROM video_subtitles WHERE id = ?"
        ).map_err(|e| e.to_string())?;
        stmt.bind((1, subtitle_id.as_str())).map_err(|e| e.to_string())?;
        if let Ok(sqlite::State::Row) = stmt.next() {
            let f_id: Option<i64> = stmt.read(0).ok();
            let v_id: i64 = stmt.read(1).unwrap_or_default();
            let s_id: Option<i64> = stmt.read(2).ok();
            let p_id: Option<i64> = stmt.read(3).ok();
            (f_id, v_id, s_id, p_id)
        } else {
            return Ok(false);
        }
    };

    // The sidecar messages live in the same folder channel as the video.
    let effective_folder_id = folder_id.or(row_folder_id);
    if subtitle_msg_id.is_some() || paired_msg_id.is_some() {
        let client = { state.client.lock().await.clone() }.ok_or("Telegram client not initialized")?;
        let peer = resolve_peer(&client, effective_folder_id, &state.peer_cache).await?;
        let ids: Vec<i32> = [subtitle_msg_id, paired_msg_id]
            .into_iter()
            .flatten()
            .map(|id| id as i32)
            .collect();
        if !ids.is_empty() {
            crate::commands::fs::delete_message_ids(&client, peer, &ids, "Subtitle detach").await?;
        }
    }

    // Remove every cached copy (3 key layouts, plus whisper-era .{lang}.srt variants).
    let subtitle_exts = ["srt", "ass", "ssa", "vtt", "idx", "sub"];
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        if let Ok(entries) = std::fs::read_dir(app_dir.join("streaming").join("captions")) {
            let mut prefixes = vec![format!("{}_{}.", effective_folder_id.unwrap_or(0), video_message_id), format!("{}.", video_message_id)];
            if let Some(ref v_name) = video_file_name {
                if let Some(stem) = Path::new(v_name).file_stem().and_then(|s| s.to_str()) {
                    if !stem.is_empty() {
                        prefixes.push(format!("{}.", stem));
                    }
                }
            }
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
                if !subtitle_exts.contains(&ext.as_str()) {
                    continue;
                }
                if prefixes.iter().any(|p| name.starts_with(p)) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("DELETE FROM video_subtitles WHERE id = ?").map_err(|e| e.to_string())?;
        stmt.bind((1, subtitle_id.as_str())).map_err(|e| e.to_string())?;
        stmt.next().map_err(|e| e.to_string())?;
    }

    crate::transfer_log::record_transfer_success(
        "Subtitle Delete",
        format!("Subtitle {} (message {}) removed from video message {}", subtitle_id, subtitle_msg_id.unwrap_or(0), video_message_id),
        effective_folder_id.map(|id| format!("folder_id: {}", id)),
    );

    Ok(true)
}

#[tauri::command]
pub fn cmd_list_directory_files(path: String) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let dir = Path::new(&path);
    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    files.push(p.to_string_lossy().to_string());
                }
            }
        }
    }
    Ok(files)
}
