//! Single-file operations: rename, delete, and move.
//!
//! Split-part and subtitle side tables are purged together with the message so
//! a delete does not leave stale rows behind.

use grammers_client::message::InputMessage;
use grammers_tl_types as tl;
use tauri::{Manager, State};

use crate::commands::utils::resolve_peer;
use crate::db::DbConnection;
use crate::transfer_log::record_transfer_log;
use crate::TelegramState;

use super::{
    delete_message_ids, forward_message_ids_checked, move_split_file, split_manifest_from_media,
    validate_split_parts_present,
};

#[tauri::command]
pub async fn cmd_rename_file(
    message_id: i32,
    folder_id: Option<i64>,
    new_name: String,
    state: State<'_, TelegramState>,
) -> Result<bool, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        log::info!("[MOCK] Renamed message {} to {}", message_id, new_name);
        return Ok(true);
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;

    // Verify the message exists before attempting to edit it.
    // This avoids a cryptic MESSAGE_ID_INVALID RPC error when the message
    // was moved (forwarded → new ID) or deleted since the file list was loaded.
    let messages = client.get_messages_by_id(peer, &[message_id])
        .await
        .map_err(|e| format!("Failed to fetch message for rename: {}", e))?;
    let target_msg = match messages.into_iter().flatten().next() {
        Some(m) => m,
        None => {
            return Err(format!(
                "Message {} not found in folder {:?}. The file may have been moved or deleted. Please refresh the folder.",
                message_id, folder_id
            ));
        }
    };

    let input_peer = tl::enums::InputPeer::from(peer);

    // 1. First attempt: Direct in-place EditMessage
    let edit_res = client.invoke(&tl::functions::messages::EditMessage {
        peer: input_peer,
        id: message_id,
        rich_message: None,
        no_webpage: false,
        invert_media: false,
        message: Some(new_name.clone()),
        media: None,
        reply_markup: None,
        entities: None,
        schedule_date: None,
        quick_reply_shortcut_id: None,
        schedule_repeat_period: None,
    }).await;

    if edit_res.is_ok() {
        return Ok(true);
    }

    // 2. Fallback for forwarded / moved / immutable messages:
    // When messages are forwarded (such as when moved between folders), Telegram MTProto
    // prohibits in-place message text edits and returns MESSAGE_ID_INVALID.
    // We seamlessly re-send the existing cloud media with the new name and delete the old message.
    if let Some(media) = target_msg.media() {
        let input_msg = InputMessage::new().text(new_name).copy_media(&media);
        if client.send_message(peer, input_msg).await.is_ok() {
            let _ = delete_message_ids(&client, peer, &[message_id], "Rename forwarded message fallback").await;
            return Ok(true);
        }
    }

    edit_res.map_err(|e| format!("Failed to rename file: {}", e))?;
    Ok(true)
}

/// Remove a deleted file's rows from the watch-history table and favorites.
/// Takes the pool by value so no state guard is held across an await.
fn purge_file_side_tables(
    pool: DbConnection,
    message_id: i32,
    folder_id: Option<i64>,
) -> Result<(), String> {
    let conn = pool.lock().map_err(|e| e.to_string())?;

    let mut history = conn
        .prepare("DELETE FROM watch_history WHERE file_id = ?1")
        .map_err(|e| e.to_string())?;
    history.bind((1, message_id as i64)).map_err(|e| e.to_string())?;
    history.next().map_err(|e| e.to_string())?;

    let _ = crate::commands::favorites::purge_favorite_for_file(&conn, folder_id, message_id);
    let _ = crate::commands::file_tags::purge_tags_for_file(&conn, folder_id, message_id);

    Ok(())
}

#[tauri::command]
pub async fn cmd_delete_file(
    message_id: i32,
    folder_id: Option<i64>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
) -> Result<bool, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
         log::info!("[MOCK] Deleted message {} from folder {:?}", message_id, folder_id);
        return Ok(true);
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;

    // Verify the message exists before attempting to delete it.
    // This avoids a cryptic MESSAGE_ID_INVALID RPC error when the message
    // was already moved or deleted since the file list was loaded.
    let messages = client.get_messages_by_id(peer, &[message_id])
        .await
        .map_err(|e| format!("Failed to fetch message for delete: {}", e))?;
    let msg = messages.iter().flatten().next().ok_or_else(|| format!(
            "Message {} not found in folder {:?}. The file may have already been moved or deleted. Please refresh the folder.",
            message_id, folder_id
    ))?;

    let mut ids = vec![message_id];
    if let Some(media) = msg.media() {
        if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
            validate_split_parts_present(&client, peer, &manifest, "Split delete").await?;
            ids.extend(manifest.parts.iter().map(|p| p.message_id));
        }
    }

    delete_message_ids(&client, peer, &ids, "Delete").await?;

    // Remove attached subtitle sidecars so they don't become hidden orphans:
    // their Telegram messages, video_subtitles rows, and all cached copies.
    let subtitle_rows = {
        let db = app_handle.state::<DbConnection>();
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT subtitle_message_id, paired_message_id FROM video_subtitles \
             WHERE (folder_id IS ? OR folder_id = ?) AND video_message_id = ?"
        ).map_err(|e| e.to_string())?;
        stmt.bind((1, folder_id)).map_err(|e| e.to_string())?;
        stmt.bind((2, folder_id.unwrap_or(0))).map_err(|e| e.to_string())?;
        stmt.bind((3, message_id as i64)).map_err(|e| e.to_string())?;
        let mut rows = Vec::new();
        while let Ok(sqlite::State::Row) = stmt.next() {
            rows.push((stmt.read::<Option<i64>, _>(0).ok().flatten(), stmt.read::<Option<i64>, _>(1).ok().flatten()));
        }
        let mut stmt = conn.prepare(
            "DELETE FROM video_subtitles WHERE (folder_id IS ? OR folder_id = ?) AND video_message_id = ?"
        ).map_err(|e| e.to_string())?;
        stmt.bind((1, folder_id)).map_err(|e| e.to_string())?;
        stmt.bind((2, folder_id.unwrap_or(0))).map_err(|e| e.to_string())?;
        stmt.bind((3, message_id as i64)).map_err(|e| e.to_string())?;
        let _ = stmt.next();
        rows
    };
    let sidecar_ids: Vec<i32> = subtitle_rows
        .into_iter()
        .flat_map(|(s, p)| [s, p])
        .flatten()
        .map(|id| id as i32)
        .collect();
    if !sidecar_ids.is_empty() {
        delete_message_ids(&client, peer, &sidecar_ids, "Subtitle sidecar delete").await?;
    }

    // Drop the file's watch-history row so analytics
    // and the poster wall stop counting a file that no longer exists.
    {
        let pool = (*app_handle.state::<DbConnection>()).clone();
        let _ = purge_file_side_tables(pool, message_id, folder_id);
    }

    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        // Remove every cached caption variant ({folder}_{msg}.* and bare {msg}.*)
        let prefixes = [
            format!("{}_{}.", folder_id.unwrap_or(0), message_id),
            format!("{}.", message_id),
        ];
        let caption_exts = ["srt", "ass", "ssa", "vtt", "idx", "sub"];
        if let Ok(entries) = std::fs::read_dir(app_dir.join("streaming").join("captions")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
                if !caption_exts.contains(&ext.as_str()) {
                    continue;
                }
                if prefixes.iter().any(|p| name.starts_with(p)) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    Ok(true)
}

#[tauri::command]
pub async fn cmd_move_files(
    message_ids: Vec<i32>,
    source_folder_id: Option<i64>,
    target_folder_id: Option<i64>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
) -> Result<bool, String> {
    if source_folder_id == target_folder_id { return Ok(true); }
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        log::info!("[MOCK] Moved msgs {:?} from {:?} to {:?}", message_ids, source_folder_id, target_folder_id);
        return Ok(true);
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    let source_peer = resolve_peer(&client, source_folder_id, &state.peer_cache).await?;
    let target_peer = resolve_peer(&client, target_folder_id, &state.peer_cache).await?;

    let source_messages = client
        .get_messages_by_id(source_peer, &message_ids)
        .await
        .map_err(|e| format!("Failed to inspect files before move: {}", e))?;

    let mut normal_ids = Vec::new();
    let mut split_moves = Vec::new();
    for (message_id, msg) in message_ids.iter().zip(source_messages.into_iter()) {
        let msg = msg.ok_or_else(|| format!("Message {} not found in source folder. Please refresh and try again.", message_id))?;
        if let Some(media) = msg.media() {
            if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
                validate_split_parts_present(&client, source_peer, &manifest, "Split move preflight").await?;
                split_moves.push((*message_id, manifest));
                continue;
            }
        }
        normal_ids.push(*message_id);
    }

    for (manifest_message_id, manifest) in split_moves {
        move_split_file(
            &client,
            source_peer,
            target_peer,
            manifest_message_id,
            manifest,
            &app_handle,
            state.inner(),
        )
        .await?;
    }

    if !normal_ids.is_empty() {
        let forwarded_ids = forward_message_ids_checked(
            &client,
            source_peer,
            target_peer,
            &normal_ids,
            "Move",
        )
        .await?;
        if let Err(e) = delete_message_ids(&client, source_peer, &normal_ids, "Move source cleanup").await {
            record_transfer_log(
                "Move",
                "Moved files were forwarded but source delete failed".to_string(),
                Some(format!("target_message_ids: {:?}\nerror: {}", forwarded_ids, e)),
            );
            return Err(format!("Delete original failed: {}", e));
        }
    }

    Ok(true)
}
