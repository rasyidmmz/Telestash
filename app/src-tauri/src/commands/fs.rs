use tauri::{Emitter, State};
use std::sync::Arc;
use grammers_client::media::Media;
use grammers_session::types::PeerRef;
use grammers_client::message::InputMessage;
use crate::TelegramState;
use crate::models::{
    SplitManifest, SplitPart, SPLIT_MANIFEST_SUFFIX,
    SPLIT_MANIFEST_UPLOAD_NAME, SPLIT_MANIFEST_VERSION, SPLIT_PART_CAPTION_PREFIX,
};
use crate::bandwidth::BandwidthManager;
use crate::commands::utils::{resolve_peer, map_error, media_size};
use crate::transfer_retry::{
    backoff_ms, flood_wait_retry_attempts, should_retry_upload_error, upload_error_kind,
    upload_stream_retry_attempts, DOWNLOAD_CHUNK_RETRY_ATTEMPTS, DOWNLOAD_STALL_TIMEOUT_SECS,
    RETRY_ATTEMPTS, RETRY_BASE_BACKOFF_MS, RETRY_MAX_BACKOFF_MS,
};
use crate::split_manifest::{
    is_split_manifest_candidate, validate_split_manifest, MAX_SPLIT_MANIFEST_BYTES,
};
use crate::split_upload_resume::{
    clear_split_upload_state, expected_split_part_size, load_split_upload_state,
    save_split_upload_state, split_upload_state_path, SplitUploadResumePart,
    SplitUploadResumeState,
};
use crate::transfer_log::record_transfer_log;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::sync::Mutex;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use tokio::sync::oneshot;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

// Submodules split out of this file. `fs.rs` remains the module root, so every
// existing `crate::commands::fs::…` path keeps resolving unchanged.
//
// Glob re-exports on purpose: `#[tauri::command]` also emits hidden `__cmd__*`
// items that `generate_handler!` looks up here, and an explicit re-export list
// silently drops them.
mod download;
mod files;
mod folders;
mod listing;

pub use download::*;
pub use files::*;
pub use folders::*;
pub use listing::*;

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

fn video_mime_from_name(name: &str) -> String {
    match Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("mkv") => "video/x-matroska".to_string(),
        Some("mp4") => "video/mp4".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

fn split_temp_path(file_name: &str, suffix: &str) -> PathBuf {
    let safe_name = file_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "telestash-{}-{}-{}",
        std::process::id(),
        unique,
        suffix.replace("{name}", &safe_name)
    ))
}

async fn write_next_split_part(
    src: &mut tokio::fs::File,
    part_path: &Path,
    max_bytes: u64,
) -> Result<u64, String> {
    let mut out = tokio::fs::File::create(part_path)
        .await
        .map_err(|e| format!("Failed to create split part: {}", e))?;
    let mut buf = vec![0u8; SPLIT_TEMP_BUFFER];
    let mut written = 0u64;

    while written < max_bytes {
        let want = std::cmp::min(buf.len() as u64, max_bytes - written) as usize;
        let n = src
            .read(&mut buf[..want])
            .await
            .map_err(|e| format!("Failed to read split source: {}", e))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .await
            .map_err(|e| format!("Failed to write split part: {}", e))?;
        written += n as u64;
    }

    out.flush()
        .await
        .map_err(|e| format!("Failed to flush split part: {}", e))?;
    Ok(written)
}

async fn download_manifest_bytes(
    client: &grammers_client::Client,
    media: &Media,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut iter = client.iter_download(media);
    while let Some(chunk) = iter.next().await.transpose() {
        let bytes = chunk.map_err(|e| map_error(&e))?;
        out.extend_from_slice(&bytes);
        if out.len() as u64 > MAX_SPLIT_MANIFEST_BYTES {
            return Err("Split manifest is too large".to_string());
        }
    }
    Ok(out)
}

pub(crate) async fn split_manifest_from_media(
    client: &grammers_client::Client,
    media: &Media,
    caption: &str,
) -> Option<SplitManifest> {
    match media {
        Media::Document(d)
            if is_split_manifest_candidate(
                d.name().unwrap_or_default(),
                d.mime_type(),
                d.size().unwrap_or(0) as u64,
                caption,
            ) =>
        {
            let bytes = download_manifest_bytes(client, media).await.ok()?;
            let manifest: SplitManifest = serde_json::from_slice(&bytes).ok()?;
            if validate_split_manifest(&manifest).is_ok() {
                Some(manifest)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) async fn validate_split_parts_present(
    client: &grammers_client::Client,
    peer: PeerRef,
    manifest: &SplitManifest,
    source: &str,
) -> Result<(), String> {
    validate_split_manifest(manifest)?;

    let mut base_index = 0usize;
    for chunk in manifest.parts.chunks(100) {
        let ids: Vec<i32> = chunk.iter().map(|part| part.message_id).collect();
        let messages = client
            .get_messages_by_id(peer, &ids)
            .await
            .map_err(|e| format!("Failed to validate split parts: {}", e))?;

        for (offset, (part, msg)) in chunk.iter().zip(messages.into_iter()).enumerate() {
            let part_number = base_index + offset + 1;
            let msg = msg.ok_or_else(|| {
                let err = format!(
                    "Split file incomplete: missing part {}/{} (message_id {})",
                    part_number,
                    manifest.parts.len(),
                    part.message_id
                );
                record_transfer_log(source, err.clone(), Some(format!("file: {}", manifest.filename)));
                err
            })?;
            let media = msg.media().ok_or_else(|| {
                let err = format!(
                    "Split file incomplete: part {}/{} has no media (message_id {})",
                    part_number,
                    manifest.parts.len(),
                    part.message_id
                );
                record_transfer_log(source, err.clone(), Some(format!("file: {}", manifest.filename)));
                err
            })?;
            if let Some(actual_size) = media_size(&media) {
                if actual_size != part.size {
                    let err = format!(
                        "Split part size mismatch: part {}/{} expected {} bytes, Telegram has {} bytes",
                        part_number,
                        manifest.parts.len(),
                        part.size,
                        actual_size
                    );
                    record_transfer_log(source, err.clone(), Some(format!("file: {}", manifest.filename)));
                    return Err(err);
                }
            }
        }

        base_index += chunk.len();
    }

    Ok(())
}

pub(crate) async fn upload_path_and_send(
    client: &grammers_client::Client,
    peer: PeerRef,
    path: &str,
    upload_name: String,
    caption: String,
    tid: &str,
    state: &TelegramState,
    app_handle: &tauri::AppHandle,
    progress_base: u64,
    progress_total: u64,
) -> Result<i32, String> {
    let configured_attempts = RETRY_ATTEMPTS;
    let max_attempts = upload_stream_retry_attempts(configured_attempts);
    let base_ms = RETRY_BASE_BACKOFF_MS;
    let max_ms = RETRY_MAX_BACKOFF_MS;
    let flood_wait_attempts = flood_wait_retry_attempts(configured_attempts);
    let mut attempt = 0;
    let mut attempts_made = 0;
    let mut last_err = String::new();
    let mut uploaded_file = None;
    let mut flood_wait_count = 0;

    while attempt <= max_attempts {
        if state.cancelled_transfers.read().await.contains(tid) {
            state.cancelled_transfers.write().await.remove(tid);
            return Err("Transfer cancelled".to_string());
        }

        let (mut reader, file_size, bytes_counter) = ProgressReader::new_with_pause(path, 0, Some(state.paused_transfers.clone()), tid).await?;
        let progress_handle = app_handle.clone();
        let progress_tid = tid.to_string();
        let progress_task = if !tid.is_empty() {
            Some(tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    let current = progress_base + bytes_counter.load(std::sync::atomic::Ordering::Relaxed);
                    let percent = if progress_total > 0 {
                        ((current as f64 / progress_total as f64) * 100.0).min(99.0) as u8
                    } else {
                        0
                    };
                    let _ = progress_handle.emit("upload-progress", ProgressPayload {
                        id: progress_tid.clone(),
                        percent,
                        uploaded_bytes: current,
                        total_bytes: progress_total,
                        speed_bytes_per_sec: 0,
                    });
                    if current >= progress_base + file_size {
                        break;
                    }
                }
            }))
        } else {
            None
        };

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        if !tid.is_empty() {
            get_upload_cancellations().lock().unwrap_or_else(|p| p.into_inner()).insert(tid.to_string(), cancel_tx);
        }

        let client_clone = client.clone();
        let attempt_upload_name = upload_name.clone();
        let mut upload_task = tokio::spawn(async move {
            client_clone.upload_stream(&mut reader, file_size as usize, attempt_upload_name).await
        });

        attempts_made = attempt + 1;

        let upload_result = tokio::select! {
            res = &mut upload_task => {
                if !tid.is_empty() {
                    get_upload_cancellations().lock().unwrap_or_else(|p| p.into_inner()).remove(tid);
                }
                res.map_err(|e| format!("Task join error: {}", e))
            }
            _ = cancel_rx => {
                upload_task.abort();
                if let Some(t) = progress_task { t.abort(); }
                state.cancelled_transfers.write().await.remove(tid);
                return Err("Transfer cancelled".to_string());
            }
        };

        if let Some(t) = progress_task {
            t.abort();
        }

        match upload_result {
            Ok(Ok(file)) => {
                uploaded_file = Some(file);
                break;
            }
            Ok(Err(e)) => {
                let err = map_error(e);
                last_err = format!("{}: {}", upload_error_kind(&err), err);
                record_transfer_log(
                    "Upload",
                    format!("Upload attempt {}/{} failed for {}", attempt + 1, max_attempts + 1, upload_name),
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
                        log::info!("Respecting FLOOD_WAIT for split upload ({}/{}): sleeping {}s", flood_wait_count, flood_wait_attempts, wait);
                        let _ = app_handle.emit("flood-wait", FloodWaitPayload { wait_seconds: wait, attempt: flood_wait_count, max_attempts: flood_wait_attempts });
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                        continue;
                    }
                }
                if should_retry_upload_error(&err, attempt, configured_attempts) {
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms(attempt, base_ms, max_ms))).await;
                } else {
                    break;
                }
            }
            Err(e) => {
                last_err = e;
                record_transfer_log(
                    "Upload",
                    format!("Upload task attempt {}/{} failed for {}", attempt + 1, max_attempts + 1, upload_name),
                    Some(format!(
                        "transfer_id: {}\nfile_size: {}\nretry: {}\nerror: {}",
                        tid,
                        file_size,
                        should_retry_upload_error(&last_err, attempt, configured_attempts),
                        last_err
                    )),
                );
                if should_retry_upload_error(&last_err, attempt, configured_attempts) {
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms(attempt, base_ms, max_ms))).await;
                } else {
                    break;
                }
            }
        }
        attempt += 1;
    }

    let uploaded_file = uploaded_file
        .ok_or_else(|| format!("Upload failed after {} attempts: {}", attempts_made, last_err))?;
    let message = InputMessage::new().text(caption).file(uploaded_file);
    let mut last_err = String::new();

    let max_send_attempts = flood_wait_attempts;
    let mut send_attempts_made = 0;
    for attempt in 0..=max_send_attempts {
        send_attempts_made = attempt + 1;
        match client.send_message(peer, message.clone()).await {
            Ok(msg) => return Ok(msg.id()),
            Err(e) => {
                let err = map_error(e);
                if err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        last_err = err;
                        if attempt < max_send_attempts {
                            let wait = secs.min(300);
                            let _ = app_handle.emit("flood-wait", FloodWaitPayload { wait_seconds: wait, attempt: attempt + 1, max_attempts: max_send_attempts });
                            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                            continue;
                        }
                        break;
                    }
                }
                last_err = err;
                if attempt < configured_attempts {
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms(attempt, base_ms, max_ms))).await;
                } else {
                    break;
                }
            }
        }
    }

    Err(format!("Send failed after {} attempts: {}", send_attempts_made, last_err))
}

pub(crate) async fn delete_message_ids(
    client: &grammers_client::Client,
    peer: PeerRef,
    ids: &[i32],
    source: &str,
) -> Result<(), String> {
    for chunk in ids.chunks(100) {
        client
            .delete_messages(peer, chunk)
            .await
            .map_err(|e| {
                let err = format!("Delete failed: {}", e);
                record_transfer_log(source, err.clone(), Some(format!("message_ids: {:?}", chunk)));
                err
            })?;
    }
    Ok(())
}

async fn cleanup_split_messages(client: &grammers_client::Client, peer: PeerRef, ids: &[i32]) {
    if !ids.is_empty() {
        let _ = delete_message_ids(client, peer, ids, "Split cleanup").await;
    }
}

pub(crate) async fn forward_message_ids_checked(
    client: &grammers_client::Client,
    source_peer: PeerRef,
    target_peer: PeerRef,
    ids: &[i32],
    source: &str,
) -> Result<Vec<i32>, String> {
    let mut forwarded_ids = Vec::new();

    for chunk in ids.chunks(100) {
        let messages = match client.forward_messages(target_peer, chunk, source_peer).await {
            Ok(messages) => messages,
            Err(e) => {
                cleanup_split_messages(client, target_peer, &forwarded_ids).await;
                let err = format!("Forward failed: {}", e);
                record_transfer_log(source, err.clone(), Some(format!("message_ids: {:?}", chunk)));
                return Err(err);
            }
        };

        for (source_id, forwarded) in chunk.iter().zip(messages.into_iter()) {
            match forwarded {
                Some(msg) => forwarded_ids.push(msg.id()),
                None => {
                    cleanup_split_messages(client, target_peer, &forwarded_ids).await;
                    let err = format!("Forward failed for message {}", source_id);
                    record_transfer_log(source, err.clone(), Some(format!("message_ids: {:?}", chunk)));
                    return Err(err);
                }
            }
        }
    }

    Ok(forwarded_ids)
}

async fn move_split_file(
    client: &grammers_client::Client,
    source_peer: PeerRef,
    target_peer: PeerRef,
    manifest_message_id: i32,
    manifest: SplitManifest,
    app_handle: &tauri::AppHandle,
    state: &TelegramState,
) -> Result<(), String> {
    validate_split_parts_present(client, source_peer, &manifest, "Split move").await?;

    let source_part_ids: Vec<i32> = manifest.parts.iter().map(|part| part.message_id).collect();
    let forwarded_part_ids = forward_message_ids_checked(
        client,
        source_peer,
        target_peer,
        &source_part_ids,
        "Split move",
    )
    .await?;

    let mut target_cleanup_ids = forwarded_part_ids.clone();
    let mut target_manifest = manifest.clone();
    for (part, new_id) in target_manifest.parts.iter_mut().zip(forwarded_part_ids.iter()) {
        part.message_id = *new_id;
    }
    validate_split_manifest(&target_manifest)?;

    let manifest_json = serde_json::to_vec(&target_manifest)
        .map_err(|e| format!("Failed to encode moved split manifest: {}", e))?;
    let manifest_path = split_temp_path(&target_manifest.filename, &format!("{{name}}{}", SPLIT_MANIFEST_SUFFIX));
    if let Err(e) = tokio::fs::write(&manifest_path, manifest_json).await {
        cleanup_split_messages(client, target_peer, &target_cleanup_ids).await;
        return Err(format!("Failed to write moved split manifest: {}", e));
    }

    let manifest_path_str = manifest_path.to_string_lossy().to_string();
    let manifest_id = match upload_path_and_send(
        client,
        target_peer,
        &manifest_path_str,
        SPLIT_MANIFEST_UPLOAD_NAME.to_string(),
        target_manifest.filename.clone(),
        "",
        state,
        app_handle,
        target_manifest.size,
        target_manifest.size,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            let _ = tokio::fs::remove_file(&manifest_path).await;
            cleanup_split_messages(client, target_peer, &target_cleanup_ids).await;
            record_transfer_log("Split move", e.clone(), Some(format!("file: {}", target_manifest.filename)));
            return Err(e);
        }
    };

    let _ = tokio::fs::remove_file(&manifest_path).await;
    target_cleanup_ids.push(manifest_id);
    if let Err(e) = validate_split_parts_present(client, target_peer, &target_manifest, "Split move target validation").await {
        cleanup_split_messages(client, target_peer, &target_cleanup_ids).await;
        return Err(e);
    }

    let mut source_delete_ids = vec![manifest_message_id];
    source_delete_ids.extend(source_part_ids);
    delete_message_ids(client, source_peer, &source_delete_ids, "Split move source cleanup").await?;
    Ok(())
}

async fn split_error<T>(
    client: &grammers_client::Client,
    peer: PeerRef,
    uploaded_ids: &[i32],
    err: String,
) -> Result<T, String> {
    cleanup_split_messages(client, peer, uploaded_ids).await;
    Err(err)
}

async fn save_split_resume_snapshot(
    path: &PathBuf,
    folder_id: Option<i64>,
    file_name: &str,
    file_size: u64,
    total_parts: usize,
    resume_parts: &[SplitUploadResumePart],
) {
    let state = SplitUploadResumeState::new(
        folder_id,
        file_name.to_string(),
        file_size,
        SPLIT_PART_SIZE,
        total_parts,
        resume_parts.to_vec(),
    );
    if let Err(e) = save_split_upload_state(path, &state).await {
        record_transfer_log("Split upload", e, Some(format!("file: {}", file_name)));
    }
}

async fn upload_large_file_split(
    path: &str,
    folder_id: Option<i64>,
    file_name: String,
    size: u64,
    tid: &str,
    app_handle: &tauri::AppHandle,
    state: &TelegramState,
    client: &grammers_client::Client,
) -> Result<String, String> {
    let peer = resolve_peer(client, folder_id, &state.peer_cache).await?;
    let total_parts = ((size + SPLIT_PART_SIZE - 1) / SPLIT_PART_SIZE) as usize;
    let resume_path = split_upload_state_path(app_handle, folder_id, &file_name, size, SPLIT_PART_SIZE)?;
    let mut src = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open large file for splitting: {}", e))?;
    let mut uploaded_ids: Vec<i32> = Vec::new();
    let mut parts: Vec<SplitPart> = Vec::new();
    let mut resume_parts: Vec<SplitUploadResumePart> = Vec::new();
    let mut uploaded_bytes = 0u64;
    let mut resume_by_index: HashMap<usize, SplitUploadResumePart> = HashMap::new();

    if let Some(resume) = load_split_upload_state(&resume_path).await {
        if resume.matches_upload(folder_id, &file_name, size, SPLIT_PART_SIZE, total_parts) {
            for part in resume.parts {
                let Some(expected_size) = expected_split_part_size(size, SPLIT_PART_SIZE, part.index, total_parts) else {
                    continue;
                };
                if part.message_id > 0 && part.size == expected_size && !resume_by_index.contains_key(&part.index) {
                    resume_by_index.insert(part.index, part);
                }
            }
            if !resume_by_index.is_empty() {
                record_transfer_log(
                    "Split upload",
                    format!("Resuming split upload for {}", file_name),
                    Some(format!("reused_parts: {}/{}", resume_by_index.len(), total_parts)),
                );
            }
        }
    }

    for index in 0..total_parts {
        if state.cancelled_transfers.read().await.contains(tid) {
            state.cancelled_transfers.write().await.remove(tid);
            clear_split_upload_state(&resume_path).await;
            return split_error(client, peer, &uploaded_ids, "Transfer cancelled".to_string()).await;
        }

        if let Some(resumed) = resume_by_index.remove(&index) {
            uploaded_ids.push(resumed.message_id);
            parts.push(SplitPart { message_id: resumed.message_id, size: resumed.size });
            uploaded_bytes += resumed.size;
            resume_parts.push(resumed);
            if !tid.is_empty() {
                let _ = app_handle.emit("upload-progress", ProgressPayload {
                    id: tid.to_string(),
                    percent: ((uploaded_bytes.saturating_mul(100) / size.max(1)) as u8).min(100),
                    uploaded_bytes,
                    total_bytes: size,
                    speed_bytes_per_sec: 0,
                });
            }
            continue;
        }

        if let Err(e) = src.seek(SeekFrom::Start(index as u64 * SPLIT_PART_SIZE)).await {
            clear_split_upload_state(&resume_path).await;
            return split_error(
                client,
                peer,
                &uploaded_ids,
                format!("Failed to seek split source: {}", e),
            )
            .await;
        }
        let part_path = split_temp_path(&file_name, &format!("{{name}}-part-{:04}.bin", index + 1));
        let written = match write_next_split_part(&mut src, &part_path, SPLIT_PART_SIZE).await {
            Ok(n) if n > 0 => n,
            Ok(_) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                clear_split_upload_state(&resume_path).await;
                return split_error(client, peer, &uploaded_ids, "Split produced an empty part".to_string()).await;
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                clear_split_upload_state(&resume_path).await;
                return split_error(client, peer, &uploaded_ids, e).await;
            }
        };

        let upload_name = format!("{}.tdpart{:04}of{:04}", file_name, index + 1, total_parts);
        let caption = format!("{} {} {}/{}", SPLIT_PART_CAPTION_PREFIX, file_name, index + 1, total_parts);
        let part_path_str = part_path.to_string_lossy().to_string();
        let msg_id = match upload_path_and_send(
            client,
            peer,
            &part_path_str,
            upload_name,
            caption,
            tid,
            state,
            app_handle,
            uploaded_bytes,
            size,
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                save_split_resume_snapshot(
                    &resume_path,
                    folder_id,
                    &file_name,
                    size,
                    total_parts,
                    &resume_parts,
                )
                .await;
                return Err(format!("{}; retry this upload to resume from the last completed split part", e));
            }
        };

        let _ = tokio::fs::remove_file(&part_path).await;
        uploaded_ids.push(msg_id);
        parts.push(SplitPart { message_id: msg_id, size: written });
        resume_parts.push(SplitUploadResumePart { index, message_id: msg_id, size: written });
        uploaded_bytes += written;
        save_split_resume_snapshot(
            &resume_path,
            folder_id,
            &file_name,
            size,
            total_parts,
            &resume_parts,
        )
        .await;

        if index + 1 < total_parts {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    if uploaded_bytes != size {
        clear_split_upload_state(&resume_path).await;
        return split_error(
            client,
            peer,
            &uploaded_ids,
            format!("Split size mismatch: expected {} bytes, uploaded {}", size, uploaded_bytes),
        )
        .await;
    }

    let file_ext = Path::new(&file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase());
    let manifest = SplitManifest {
        telestash_split: SPLIT_MANIFEST_VERSION,
        filename: file_name.clone(),
        size,
        mime_type: video_mime_from_name(&file_name),
        file_ext,
        part_size: SPLIT_PART_SIZE,
        parts,
    };
    if let Err(e) = validate_split_manifest(&manifest) {
        clear_split_upload_state(&resume_path).await;
        return split_error(client, peer, &uploaded_ids, e).await;
    }
    if let Err(e) = validate_split_parts_present(client, peer, &manifest, "Split upload validation").await {
        clear_split_upload_state(&resume_path).await;
        return split_error(client, peer, &uploaded_ids, e).await;
    }
    let manifest_json = match serde_json::to_vec(&manifest) {
        Ok(json) => json,
        Err(e) => {
            clear_split_upload_state(&resume_path).await;
            return split_error(
                client,
                peer,
                &uploaded_ids,
                format!("Failed to encode split manifest: {}", e),
            )
            .await;
        }
    };
    let manifest_path = split_temp_path(&file_name, &format!("{{name}}{}", SPLIT_MANIFEST_SUFFIX));
    if let Err(e) = tokio::fs::write(&manifest_path, manifest_json).await {
        clear_split_upload_state(&resume_path).await;
        return split_error(client, peer, &uploaded_ids, format!("Failed to write split manifest: {}", e)).await;
    }

    let manifest_path_str = manifest_path.to_string_lossy().to_string();
    let manifest_id = match upload_path_and_send(
        client,
        peer,
        &manifest_path_str,
        SPLIT_MANIFEST_UPLOAD_NAME.to_string(),
        file_name.clone(),
        tid,
        state,
        app_handle,
        size,
        size,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            let _ = tokio::fs::remove_file(&manifest_path).await;
            save_split_resume_snapshot(
                &resume_path,
                folder_id,
                &file_name,
                size,
                total_parts,
                &resume_parts,
            )
            .await;
            return Err(format!("{}; retry this upload to reuse the completed split parts", e));
        }
    };

    let _ = tokio::fs::remove_file(&manifest_path).await;
    uploaded_ids.push(manifest_id);
    clear_split_upload_state(&resume_path).await;

    if !tid.is_empty() {
        let _ = app_handle.emit("upload-progress", ProgressPayload {
            id: tid.to_string(),
            percent: 100,
            uploaded_bytes: size,
            total_bytes: size,
            speed_bytes_per_sec: 0,
        });
    }

    Ok("Large file uploaded as split parts".to_string())
}

async fn download_split_file(
    client: &grammers_client::Client,
    peer: PeerRef,
    manifest: SplitManifest,
    save_path: &str,
    tid: &str,
    app_handle: &tauri::AppHandle,
    state: &TelegramState,
    bw_state: &BandwidthManager,
) -> Result<String, String> {
    validate_split_parts_present(client, peer, &manifest, "Split download").await?;
    bw_state.try_reserve_down(manifest.size)?;
    if !tid.is_empty() {
        let _ = app_handle.emit("download-progress", ProgressPayload {
            id: tid.to_string(),
            percent: 0,
            uploaded_bytes: 0,
            total_bytes: manifest.size,
            speed_bytes_per_sec: 0,
        });
    }

    let mut file = tokio::fs::File::create(save_path).await.map_err(|e| {
        bw_state.release_down(manifest.size);
        e.to_string()
    })?;
    let mut downloaded = 0u64;
    let mut last_emit_time = std::time::Instant::now();
    let mut last_emit_bytes = 0u64;

    for part in &manifest.parts {
        if state.cancelled_transfers.read().await.contains(tid) {
            state.cancelled_transfers.write().await.remove(tid);
            drop(file);
            cleanup_partial_file(save_path);
            bw_state.release_down(manifest.size);
            return Err("Transfer cancelled".to_string());
        }

        let messages = client
            .get_messages_by_id(peer, &[part.message_id])
            .await
            .map_err(|e| e.to_string())?;
        let msg = messages
            .into_iter()
            .flatten()
            .next()
            .ok_or_else(|| format!("Split part {} not found", part.message_id))?;
        let media = msg.media().ok_or_else(|| format!("Split part {} has no media", part.message_id))?;
        let mut iter = client.iter_download(&media);
        let mut chunk_retry_budget = DOWNLOAD_CHUNK_RETRY_ATTEMPTS;

        loop {
            if state.cancelled_transfers.read().await.contains(tid) {
                state.cancelled_transfers.write().await.remove(tid);
                drop(file);
                cleanup_partial_file(save_path);
                bw_state.release_down(manifest.size);
                return Err("Transfer cancelled".to_string());
            }

            while state.paused_transfers.read().await.contains(tid) {
                let notifier = {
                    let mut map = state.pause_notifiers.lock().unwrap_or_else(|p| p.into_inner());
                    map.entry(tid.to_string())
                        .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
                        .clone()
                };
                notifier.notified().await;
                if state.cancelled_transfers.read().await.contains(tid) {
                    state.cancelled_transfers.write().await.remove(tid);
                    drop(file);
                    cleanup_partial_file(save_path);
                    bw_state.release_down(manifest.size);
                    return Err("Transfer cancelled".to_string());
                }
            }

            // Stall watchdog: a download that silently stops producing chunks
            // previously hung forever. Time out, treat as a chunk error, and
            // let the retry budget decide when to give up.
            let timed = tokio::time::timeout(
                std::time::Duration::from_secs(DOWNLOAD_STALL_TIMEOUT_SECS),
                iter.next(),
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
                        chunk_retry_budget = DOWNLOAD_CHUNK_RETRY_ATTEMPTS;
                        (Some(b), None)
                    }
                    Some(Err(e)) => (None, Some(map_error(&e))),
                },
            };

            let bytes = if let Some(b) = bytes {
                b
            } else {
                let err = chunk_err.expect("missing chunk must carry an error");
                record_transfer_log(
                    "Split download",
                    format!("Split download chunk error for {}", manifest.filename),
                    Some(format!(
                        "transfer_id: {}\nfile_size: {}\ndownloaded: {}\nretries_left: {}\nerror: {}",
                        tid,
                        manifest.size,
                        downloaded,
                        chunk_retry_budget,
                        err
                    )),
                );
                if chunk_retry_budget > 0 {
                    chunk_retry_budget -= 1;
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms(
                        0,
                        RETRY_BASE_BACKOFF_MS,
                        RETRY_MAX_BACKOFF_MS,
                    )))
                    .await;
                    continue;
                }
                drop(file);
                cleanup_partial_file(save_path);
                bw_state.release_down(manifest.size);
                return Err(format!("Split download chunk error: {}", err));
            };

            file.write_all(&bytes).await.map_err(|e| e.to_string())?;
            downloaded += bytes.len() as u64;

            if !tid.is_empty() {
                let now = std::time::Instant::now();
                let dt = now.duration_since(last_emit_time).as_secs_f64();
                if dt >= 0.25 || downloaded >= manifest.size {
                    let speed = if dt > 0.0 { ((downloaded - last_emit_bytes) as f64 / dt) as u64 } else { 0 };
                    let percent = if manifest.size > 0 {
                        ((downloaded as f64 / manifest.size as f64) * 100.0).min(100.0) as u8
                    } else {
                        0
                    };
                    let _ = app_handle.emit("download-progress", ProgressPayload {
                        id: tid.to_string(),
                        percent,
                        uploaded_bytes: downloaded,
                        total_bytes: manifest.size,
                        speed_bytes_per_sec: speed,
                    });
                    last_emit_time = now;
                    last_emit_bytes = downloaded;
                }
            }
        }
    }

    file.flush().await.map_err(|e| format!("Failed to flush split download: {}", e))?;
    file.sync_all().await.map_err(|e| format!("Failed to sync split download: {}", e))?;
    drop(file);

    if downloaded != manifest.size {
        cleanup_partial_file(save_path);
        bw_state.release_down(manifest.size);
        return Err(format!(
            "Split download size mismatch: expected {} bytes, received {} bytes",
            manifest.size, downloaded
        ));
    }

    if !tid.is_empty() {
        let _ = app_handle.emit("download-progress", ProgressPayload {
            id: tid.to_string(),
            percent: 100,
            uploaded_bytes: downloaded,
            total_bytes: manifest.size,
            speed_bytes_per_sec: 0,
        });
    }

    Ok("Download successful".to_string())
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
