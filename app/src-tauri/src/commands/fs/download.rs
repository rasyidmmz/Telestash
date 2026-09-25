//! File download: single-file and split-file paths into a chosen local path.
//!
//! Downloads are chunked with their own retry budget and a stall watchdog, and
//! a partial file is kept while retries remain.

use std::sync::Arc;

use grammers_client::media::Media;
use tauri::{Emitter, State};

use crate::bandwidth::BandwidthManager;
use crate::commands::utils::{map_error, resolve_peer};
use crate::transfer_retry::{
    backoff_ms, DOWNLOAD_CHUNK_RETRY_ATTEMPTS, DOWNLOAD_STALL_TIMEOUT_SECS, RETRY_BASE_BACKOFF_MS,
    RETRY_MAX_BACKOFF_MS,
};
use crate::TelegramState;

use super::split::{download_split_file, split_manifest_from_media};
use super::upload::{cleanup_partial_file, ProgressPayload};

#[derive(Debug, serde::Deserialize)]
pub struct DownloadFileRequest {
    message_id: i32,
    save_path: String,
    folder_id: Option<i64>,
    transfer_id: Option<String>,
}

/// Avoid silently truncating an existing local file: if the target exists,
/// append " (1)", " (2)", ... before the extension until a free name is found.
fn unique_save_path(path: &str) -> String {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return path.to_string();
    }
    let parent = p.parent();
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("download");
    let ext = p.extension().and_then(|s| s.to_str());
    for n in 1..1000u32 {
        let candidate = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let full = match parent {
            Some(dir) => dir.join(&candidate),
            None => std::path::PathBuf::from(&candidate),
        };
        if !full.exists() {
            return full.to_string_lossy().to_string();
        }
    }
    path.to_string()
}

#[tauri::command]
pub async fn cmd_download_file(
    req: DownloadFileRequest,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    bw_state: State<'_, Arc<BandwidthManager>>,
) -> Result<String, String> {
    let tid = req.transfer_id.unwrap_or_default();
    let save_path = req.save_path;
    let folder_id = req.folder_id;
    let message_id = req.message_id;

    let actual_save_path = unique_save_path(&save_path);

    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        log::info!("[MOCK] Downloaded message {} from {:?} to {}", message_id, folder_id, actual_save_path);
        if let Err(e) = tokio::fs::write(&actual_save_path, b"Mock Content").await { return Err(e.to_string()); }
        return Ok("Download successful".to_string());
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;

    // Use get_messages_by_id for efficient message lookup (same as server.rs)
    let messages = client.get_messages_by_id(peer, &[message_id]).await.map_err(|e| e.to_string())?;

    let msg = messages.into_iter()
        .flatten()
        .next()
        .ok_or_else(|| "Message not found".to_string())?;

    let media = msg.media()
        .ok_or_else(|| "No media in message".to_string())?;

    if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
        return download_split_file(
            &client,
            peer,
            manifest,
            &actual_save_path,
            &tid,
            &app_handle,
            state.inner(),
            bw_state.inner().as_ref(),
        )
        .await;
    }

    let expected_file_size = match &media {
        Media::Document(d) => Some(d.size().unwrap_or(0) as u64),
        _ => None,
    };
    let total_size = expected_file_size.unwrap_or(match &media {
        Media::Photo(_) => 1024 * 1024,
        _ => 0,
    });

    bw_state.try_reserve_down(total_size)?;

    // Emit start
    if !tid.is_empty() {
        let _ = app_handle.emit("download-progress", ProgressPayload {
            id: tid.clone(), percent: 0, uploaded_bytes: 0, total_bytes: total_size, speed_bytes_per_sec: 0,
        });
    }

    // Stream download with per-chunk progress
    let mut download_iter = client.iter_download(&media);
    let mut file = tokio::fs::File::create(&actual_save_path).await.map_err(|e| {
        bw_state.release_down(total_size);
        e.to_string()
    })?;
    let mut downloaded: u64 = 0;
    let mut last_emit_time = std::time::Instant::now();
    let mut last_emit_bytes: u64 = 0;
    let mut chunk_retry_budget = DOWNLOAD_CHUNK_RETRY_ATTEMPTS;

    loop {
        // Check cancellation
        if state.cancelled_transfers.read().await.contains(&tid) {
            state.cancelled_transfers.write().await.remove(&tid);
            drop(file);
            cleanup_partial_file(&actual_save_path);
            bw_state.release_down(total_size);
            return Err("Transfer cancelled".to_string());
        }

        // Check pause
        while state.paused_transfers.read().await.contains(&tid) {
            let notifier = {
                let mut map = state.pause_notifiers.lock().unwrap_or_else(|p| p.into_inner());
                map.entry(tid.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
                    .clone()
            };
            notifier.notified().await;
            if state.cancelled_transfers.read().await.contains(&tid) {
                state.cancelled_transfers.write().await.remove(&tid);
                drop(file);
                cleanup_partial_file(&actual_save_path);
                bw_state.release_down(total_size);
                return Err("Transfer cancelled".to_string());
            }
        }

        // Stall watchdog: a download that silently stops producing chunks
        // previously hung forever. Time out, treat as a chunk error, and let
        // the retry budget decide when to give up.
        let timed = tokio::time::timeout(
            std::time::Duration::from_secs(DOWNLOAD_STALL_TIMEOUT_SECS),
            download_iter.next(),
        )
        .await;

        let (bytes, chunk_err) = match timed {
            Err(_) => (
                None,
                Some(format!("no chunk for {}s (stalled)", DOWNLOAD_STALL_TIMEOUT_SECS)),
            ),
            Ok(res) => match res.transpose() {
                None => break,
                Some(Ok(b)) => {
                    chunk_retry_budget = DOWNLOAD_CHUNK_RETRY_ATTEMPTS; // reset on success
                    (Some(b), None)
                }
                Some(Err(e)) => (None, Some(map_error(&e))),
            },
        };

        let bytes = if let Some(b) = bytes {
            b
        } else {
            let err = chunk_err.expect("missing chunk must carry an error");
            if chunk_retry_budget > 0 {
                chunk_retry_budget -= 1;
                log::warn!("Download chunk error (retries left: {}): {}", chunk_retry_budget, err);
                let delay = backoff_ms(0, RETRY_BASE_BACKOFF_MS, RETRY_MAX_BACKOFF_MS);
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                continue;
            }
            drop(file);
            cleanup_partial_file(&actual_save_path);
            bw_state.release_down(total_size);
            return Err(format!("Download chunk error: {}", err));
        };
        tokio::io::AsyncWriteExt::write_all(&mut file, &bytes).await.map_err(|e| e.to_string())?;
        downloaded += bytes.len() as u64;

        // Time-based progress emission (every 250ms)
        if !tid.is_empty() {
            let now = std::time::Instant::now();
            let dt = now.duration_since(last_emit_time).as_secs_f64();
            if dt >= 0.25 || downloaded >= total_size {
                let speed = if dt > 0.0 { ((downloaded - last_emit_bytes) as f64 / dt) as u64 } else { 0 };
                let percent = if total_size > 0 { ((downloaded as f64 / total_size as f64) * 100.0).min(100.0) as u8 } else { 0 };
                let _ = app_handle.emit("download-progress", ProgressPayload {
                    id: tid.clone(), percent, uploaded_bytes: downloaded, total_bytes: total_size, speed_bytes_per_sec: speed,
                });
                last_emit_time = now;
                last_emit_bytes = downloaded;
            }
        }

    }

    // Explicitly flush, sync, and close the file before reporting completion.
    if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut file).await {
        drop(file);
        cleanup_partial_file(&actual_save_path);
        bw_state.release_down(total_size);
        return Err(format!("Failed to flush downloaded file: {}", e));
    }
    if let Err(e) = file.sync_all().await {
        drop(file);
        cleanup_partial_file(&actual_save_path);
        bw_state.release_down(total_size);
        return Err(format!("Failed to sync downloaded file: {}", e));
    }
    drop(file);

    let actual_written = tokio::fs::metadata(&actual_save_path)
        .await
        .map_err(|e| format!("Downloaded file missing before save: {}", e))?
        .len();
    if actual_written == 0 {
        cleanup_partial_file(&actual_save_path);
        bw_state.release_down(total_size);
        return Err("Downloaded file was empty before saving".to_string());
    }
    if actual_written != downloaded {
        cleanup_partial_file(&actual_save_path);
        bw_state.release_down(total_size);
        return Err(format!(
            "Downloaded file size mismatch before saving: streamed {} bytes, file has {} bytes",
            downloaded, actual_written
        ));
    }
    if let Some(expected) = expected_file_size {
        if expected > 0 && downloaded != expected {
            cleanup_partial_file(&actual_save_path);
            bw_state.release_down(total_size);
            return Err(format!(
                "Incomplete download before saving: expected {} bytes, received {} bytes",
                expected, downloaded
            ));
        }
    }
    log::info!(
        "Download completed to {} ({} bytes)",
        actual_save_path,
        actual_written
    );

    // Emit completion
    if !tid.is_empty() {
        let _ = app_handle.emit("download-progress", ProgressPayload {
            id: tid, percent: 100, uploaded_bytes: downloaded, total_bytes: total_size, speed_bytes_per_sec: 0,
        });
    }

    Ok("Download successful".to_string())
}
