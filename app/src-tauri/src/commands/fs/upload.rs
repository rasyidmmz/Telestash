//! Upload pipeline and transfer control: progress reporting, pause/cancel
//! plumbing, the single-file upload path, and the shared upload+send helper
//! used by both the local queue and the REST API.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use grammers_client::message::InputMessage;
use tauri::{Emitter, State};
use tokio::sync::oneshot;

use crate::bandwidth::BandwidthManager;
use crate::commands::utils::{map_error, resolve_peer};
use crate::transfer_log::record_transfer_log;
use crate::transfer_retry::{
    backoff_ms, flood_wait_retry_attempts, should_retry_upload_error, upload_error_kind,
    upload_stream_retry_attempts, RETRY_ATTEMPTS, RETRY_BASE_BACKOFF_MS, RETRY_MAX_BACKOFF_MS,
};
use crate::TelegramState;

use super::split::{upload_large_file_split, upload_path_and_send};

static UPLOAD_CANCELLATIONS: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> = OnceLock::new();

const TELEGRAM_SINGLE_FILE_LIMIT: u64 = 2_000_000_000;
const SPLIT_PART_SIZE: u64 = 512 * 1024 * 1024;
const SPLIT_TEMP_BUFFER: usize = 1024 * 1024;

fn requires_split_upload(size: u64) -> bool {
    size > TELEGRAM_SINGLE_FILE_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_files_above_telegram_safe_single_file_limit() {
        assert!(!requires_split_upload(TELEGRAM_SINGLE_FILE_LIMIT));
        assert!(requires_split_upload(2_099_465_912));
    }
}

fn get_upload_cancellations() -> &'static Mutex<HashMap<String, oneshot::Sender<()>>> {
    UPLOAD_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    id: String,
    percent: u8,
    uploaded_bytes: u64,
    total_bytes: u64,
    speed_bytes_per_sec: u64,
}

/// Emitted when a transfer enters Telegram FLOOD_WAIT so the UI can show a countdown.
#[derive(Clone, serde::Serialize)]
pub struct FloodWaitPayload {
    pub wait_seconds: u64,
    pub attempt: u32,
    pub max_attempts: u32,
}

/// Async reader wrapper that tracks bytes read for progress reporting.
/// Wraps a tokio File and counts how many bytes have been consumed.
pub(crate) struct ProgressReader {
    inner: tokio::io::BufReader<tokio::fs::File>,
    bytes_read: std::sync::Arc<std::sync::atomic::AtomicU64>,
    paused_state: Option<(Arc<tokio::sync::RwLock<HashSet<String>>>, String)>,
}

impl ProgressReader {
    pub(crate) async fn new(path: &str, _limit: u64) -> Result<(Self, u64, std::sync::Arc<std::sync::atomic::AtomicU64>), String> {
        Self::new_with_pause(path, _limit, None, "").await
    }

    pub(crate) async fn new_with_pause(
        path: &str,
        _limit: u64,
        paused_set: Option<Arc<tokio::sync::RwLock<HashSet<String>>>>,
        tid: &str,
    ) -> Result<(Self, u64, std::sync::Arc<std::sync::atomic::AtomicU64>), String> {
        let file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
        let metadata = file.metadata().await.map_err(|e| e.to_string())?;
        let size = metadata.len();
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let paused_state = paused_set.map(|s| (s, tid.to_string()));
        let reader = Self {
            inner: tokio::io::BufReader::new(file),
            bytes_read: counter.clone(),
            paused_state,
        };
        Ok((reader, size, counter))
    }
}

impl tokio::io::AsyncRead for ProgressReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if let Some((paused_set, tid)) = &self.paused_state {
            if let Ok(guard) = paused_set.try_read() {
                if guard.contains(tid) {
                    let waker = cx.waker().clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        waker.wake();
                    });
                    return std::task::Poll::Pending;
                }
            }
        }
        let before = buf.filled().len();
        let result = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        if let std::task::Poll::Ready(Ok(())) = &result {
            let after = buf.filled().len();
            let delta = (after - before) as u64;
            self.bytes_read.fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
        }
        result
    }
}

/// Delete a partial file with retries (best-effort cleanup)
fn cleanup_partial_file(path: &str) {
    let path = path.to_string();
    std::thread::spawn(move || {
        for attempt in 0..5 {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    log::info!("Cleaned up partial file: {}", path);
                    return;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
                Err(e) => {
                    log::warn!("Cleanup attempt {}/5 failed for {}: {}", attempt + 1, path, e);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }
    });
}

#[tauri::command]
pub async fn cmd_cancel_transfer(
    transfer_id: String,
    state: State<'_, TelegramState>,
) -> Result<bool, String> {
    log::info!("Cancelling transfer: {}", transfer_id);
    state.cancelled_transfers.write().await.insert(transfer_id.clone());
    state.paused_transfers.write().await.remove(&transfer_id);
    if let Some(notifier) = state.pause_notifiers.lock().unwrap_or_else(|p| p.into_inner()).remove(&transfer_id) {
        notifier.notify_waiters();
    }
    if let Some(tx) = get_upload_cancellations().lock().unwrap_or_else(|p| p.into_inner()).remove(&transfer_id) {
        let _ = tx.send(());
    }
    Ok(true)
}

#[tauri::command]
pub async fn cmd_pause_transfer(
    transfer_id: String,
    state: State<'_, TelegramState>,
) -> Result<bool, String> {
    log::info!("Pausing transfer: {}", transfer_id);
    state.paused_transfers.write().await.insert(transfer_id.clone());
    Ok(true)
}

#[tauri::command]
pub async fn cmd_resume_transfer(
    transfer_id: String,
    state: State<'_, TelegramState>,
) -> Result<bool, String> {
    log::info!("Resuming transfer: {}", transfer_id);
    state.paused_transfers.write().await.remove(&transfer_id);
    if let Some(notifier) = state.pause_notifiers.lock().unwrap_or_else(|p| p.into_inner()).remove(&transfer_id) {
        notifier.notify_waiters();
    }
    Ok(true)
}

#[tauri::command]
pub async fn cmd_upload_file(
    path: String,
    folder_id: Option<i64>,
    transfer_id: Option<String>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    bw_state: State<'_, Arc<BandwidthManager>>,
) -> Result<String, String> {
    let size = tokio::fs::metadata(&path).await.map_err(|e| e.to_string())?.len();
    bw_state.try_reserve_up(size)?;

    let tid = transfer_id.unwrap_or_default();

    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        log::info!("[MOCK] Uploaded file {} to {:?}", path, folder_id);
        bw_state.release_up(size);
        return Ok("Mock upload successful".to_string());
    }
    let client = client_opt.ok_or_else(|| {
        bw_state.release_up(size);
        "Client not connected".to_string()
    })?;

    // Emit start progress
    if !tid.is_empty() {
        let _ = app_handle.emit("upload-progress", ProgressPayload {
            id: tid.clone(), percent: 0, uploaded_bytes: 0, total_bytes: size, speed_bytes_per_sec: 0,
        });
    }

    let file_name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    if requires_split_upload(size) {
        let result = upload_large_file_split(
            &path,
            folder_id,
            file_name,
            size,
            &tid,
            &app_handle,
            state.inner(),
            &client,
        )
        .await;
        if result.is_err() {
            bw_state.release_up(size);
        }
        return result;
    }

    let mut attempt = 0;
    let configured_attempts = RETRY_ATTEMPTS;
    let max_attempts = upload_stream_retry_attempts(configured_attempts);
    let base_ms = RETRY_BASE_BACKOFF_MS;
    let max_ms = RETRY_MAX_BACKOFF_MS;
    let flood_wait_attempts = flood_wait_retry_attempts(configured_attempts);
    let mut last_err = String::new();
    let mut attempts_made = 0;
    let mut uploaded_file = None;
    let mut flood_wait_count = 0;

    while attempt <= max_attempts {
        if state.cancelled_transfers.read().await.contains(&tid) {
            state.cancelled_transfers.write().await.remove(&tid);
            bw_state.release_up(size);
            return Err("Transfer cancelled".to_string());
        }

        // Create progress-tracking reader
        let (mut reader, file_size, bytes_counter) = match ProgressReader::new_with_pause(&path, 0, Some(state.paused_transfers.clone()), &tid).await {
            Ok(res) => res,
            Err(e) => {
                bw_state.release_up(size);
                return Err(e);
            }
        };

        // Spawn a progress reporter task that emits events every 250ms
        let cancelled = state.cancelled_transfers.clone();
        let progress_tid = tid.clone();
        let progress_handle = app_handle.clone();
        let progress_counter = bytes_counter.clone();
        let progress_task = if !tid.is_empty() {
            Some(tokio::spawn(async move {
                let mut last_bytes: u64 = 0;
                let mut last_time = std::time::Instant::now();
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    let current = progress_counter.load(std::sync::atomic::Ordering::Relaxed);
                    let now = std::time::Instant::now();
                    let dt = now.duration_since(last_time).as_secs_f64();
                    let speed = if dt > 0.0 { ((current - last_bytes) as f64 / dt) as u64 } else { 0 };
                    let percent = if file_size > 0 { ((current as f64 / file_size as f64) * 100.0).min(99.0) as u8 } else { 0 };

                    let _ = progress_handle.emit("upload-progress", ProgressPayload {
                        id: progress_tid.clone(), percent, uploaded_bytes: current, total_bytes: file_size, speed_bytes_per_sec: speed,
                    });

                    last_bytes = current;
                    last_time = now;

                    if current >= file_size { break; }
                    if cancelled.read().await.contains(&progress_tid) { break; }
                }
            }))
        } else {
            None
        };

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        if !tid.is_empty() {
            get_upload_cancellations().lock().unwrap_or_else(|p| p.into_inner()).insert(tid.clone(), cancel_tx);
        }

        let client_clone = client.clone();
        let name_clone = file_name.clone();
        let mut upload_task = tokio::spawn(async move {
            client_clone.upload_stream(&mut reader, file_size as usize, name_clone).await
        });

        attempts_made = attempt + 1;

        let upload_result = {
            tokio::select! {
                res = &mut upload_task => {
                    if !tid.is_empty() {
                        get_upload_cancellations().lock().unwrap_or_else(|p| p.into_inner()).remove(&tid);
                    }
                    res.map_err(|e| format!("Task join error: {}", e))
                }
                _ = cancel_rx => {
                    log::info!("Aborting upload task for transfer ID: {}", tid);
                    upload_task.abort();
                    state.cancelled_transfers.write().await.remove(&tid);
                    if let Some(t) = progress_task { t.abort(); }
                    bw_state.release_up(size);
                    return Err("Transfer cancelled".to_string());
                }
            }
        };

        if let Some(t) = progress_task { t.abort(); }

        match upload_result {
            Ok(Ok(file)) => {
                uploaded_file = Some(file);
                break;
            }
            Ok(Err(e)) => {
                let err = map_error(e);
                log::warn!("upload_stream attempt {}/{}: {}", attempt + 1, max_attempts + 1, err);
                last_err = format!("{}: {}", upload_error_kind(&err), err);
                record_transfer_log(
                    "Upload",
                    format!("Upload attempt {}/{} failed for {}", attempt + 1, max_attempts + 1, file_name),
                    Some(format!(
                        "transfer_id: {}\nfile_size: {}\nerror_kind: {}\nretry: {}\nerror: {}",
                        tid,
                        file_size,
                        upload_error_kind(&err),
                        should_retry_upload_error(&err, attempt, configured_attempts),
                        err
                    )),
                );

                if err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        flood_wait_count += 1;
                        if flood_wait_count > flood_wait_attempts {
                            break;
                        }
                        let wait = secs.min(300);
                        log::info!("Respecting FLOOD_WAIT for upload ({}/{}): sleeping {}s", flood_wait_count, flood_wait_attempts, wait);
                        let _ = app_handle.emit("flood-wait", FloodWaitPayload { wait_seconds: wait, attempt: flood_wait_count, max_attempts: flood_wait_attempts });
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                        continue;
                    }
                }

                if should_retry_upload_error(&err, attempt, configured_attempts) {
                    let delay = backoff_ms(attempt, base_ms, max_ms);
                    log::info!("Retrying upload in {}ms...", delay);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                } else {
                    break;
                }
            }
            Err(e) => {
                log::warn!("upload_stream task failed attempt {}/{}: {}", attempt + 1, max_attempts + 1, e);
                last_err = e;
                record_transfer_log(
                    "Upload",
                    format!("Upload task attempt {}/{} failed for {}", attempt + 1, max_attempts + 1, file_name),
                    Some(format!(
                        "transfer_id: {}\nfile_size: {}\nretry: {}\nerror: {}",
                        tid,
                        file_size,
                        should_retry_upload_error(&last_err, attempt, configured_attempts),
                        last_err
                    )),
                );
                if should_retry_upload_error(&last_err, attempt, configured_attempts) {
                    let delay = backoff_ms(attempt, base_ms, max_ms);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                } else {
                    break;
                }
            }
        }

        attempt += 1;
    }

    let uploaded_file = match uploaded_file {
        Some(f) => f,
        None => {
            bw_state.release_up(size);
            return Err(format!("Upload failed after {} attempts: {}", attempts_made, last_err));
        }
    };

    let lower_ext = std::path::Path::new(&path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // Extract video metadata locally before sending.
    // We log it here; the metadata is available in video_metadata.rs for on-demand queries.
    if lower_ext == "mp4" || lower_ext == "mkv" {
        if let Ok(mut f) = std::fs::File::open(&path) {
            use std::io::Read;
            let mut buf = vec![0u8; 2 * 1024 * 1024]; // 2 MB header buffer
            if let Ok(n) = f.read(&mut buf) {
                buf.truncate(n);
                if lower_ext == "mp4" {
                    if let Ok(meta) = crate::commands::video_metadata::parse_mp4_metadata(&buf) {
                        let (width, height) = crate::mp4_utils::scan_video_tkhd_dimensions(&buf);
                        log::info!(
                            "Upload MP4 metadata: duration={:.1}s, size={}x{}, audio={}",
                            meta.duration_secs.unwrap_or(0.0),
                            width.unwrap_or(0),
                            height.unwrap_or(0),
                            meta.has_audio,
                        );
                    }
                    // Fast-Start check
                    if crate::mp4_utils::find_box(&buf, 0, b"moov").is_none() {
                        log::warn!(
                            "Upload: {:?} moov atom not found in first 2MB — \
                             video may not support instant streaming. \
                             Consider running qt-faststart or ffmpeg -movflags +faststart.",
                            path
                        );
                    }
                } else if lower_ext == "mkv" {
                    if let Some((duration_secs, width, height)) =
                        crate::mp4_utils::parse_mkv_metadata(&buf)
                    {
                        log::info!(
                            "Upload MKV metadata: duration={:.1}s, size={}x{}",
                            duration_secs.unwrap_or(0.0),
                            width.unwrap_or(0),
                            height.unwrap_or(0),
                        );
                    }
                }
            }
        }
    }

    let message = InputMessage::new().text(file_name.clone()).file(uploaded_file);

    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;

    // Shared retry logic for send_message.
    let configured_retries = RETRY_ATTEMPTS;
    let base_ms = RETRY_BASE_BACKOFF_MS;
    let max_ms = RETRY_MAX_BACKOFF_MS;
    let max_retries = flood_wait_retry_attempts(configured_retries);
    let mut last_err = String::new();
    let mut send_attempts_made = 0;

    for attempt in 0..=max_retries {
        send_attempts_made = attempt + 1;
        match client.send_message(peer, message.clone()).await {
            Ok(_) => {
                // Bandwidth was already reserved by try_reserve_up at start
        if !tid.is_empty() {
            let _ = app_handle.emit("upload-progress", ProgressPayload {
                id: tid, percent: 100, uploaded_bytes: size, total_bytes: size, speed_bytes_per_sec: 0,
            });
        }
        return Ok("File uploaded successfully".to_string());
            }
            Err(e) => {
                let err = map_error(e);
                log::warn!("send_message attempt {}/{}: {}", attempt + 1, max_retries + 1, err);

                // Handle FLOOD_WAIT: sleep the requested time if configured
                if err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        last_err = err;
                        if attempt < max_retries {
                            let wait = secs.min(300); // cap at 5 min
                            log::info!("Respecting FLOOD_WAIT: sleeping {}s", wait);
                            let _ = app_handle.emit("flood-wait", FloodWaitPayload { wait_seconds: wait, attempt: attempt + 1, max_attempts: max_retries });
                            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                            continue;
                        }
                        break;
                    }
                }

                last_err = err;
                if attempt < configured_retries {
                    let delay = backoff_ms(attempt, base_ms, max_ms);
                    log::info!("Retrying in {}ms...", delay);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                } else {
                    break;
                }
            }
        }
    }

    Err(format!("Upload failed after {} attempts: {}", send_attempts_made, last_err))
}
