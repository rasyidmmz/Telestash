use tauri::{Emitter, Manager, State};
use std::sync::Arc;
use grammers_client::types::{Media, Peer};
use grammers_client::InputMessage;
use grammers_tl_types as tl;
use crate::TelegramState;
use crate::models::{
    FileMetadata, FolderMetadata, SplitManifest, SplitPart, SPLIT_MANIFEST_SUFFIX,
    SPLIT_MANIFEST_UPLOAD_NAME, SPLIT_MANIFEST_VERSION, SPLIT_PART_CAPTION_PREFIX,
};
use crate::bandwidth::BandwidthManager;
use crate::commands::utils::{resolve_peer, map_error};
use crate::transfer_policy::{TransferPolicy, backoff_ms};
use crate::transfer_retry::{flood_wait_retry_attempts, should_retry_upload_error, upload_error_kind, upload_stream_retry_attempts};
use crate::split_manifest::{
    is_split_manifest_candidate, validate_split_manifest, MAX_SPLIT_MANIFEST_BYTES,
};
use crate::split_upload_resume::{
    clear_split_upload_state, expected_split_part_size, load_split_upload_state,
    save_split_upload_state, split_upload_state_path, SplitUploadResumePart,
    SplitUploadResumeState,
};
use crate::transfer_log::record_transfer_log;
use crate::db::DbConnection;
use sqlite;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::sync::Mutex;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use tokio::sync::oneshot;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

static UPLOAD_CANCELLATIONS: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> = OnceLock::new();

const TELEGRAM_SINGLE_FILE_LIMIT: u64 = 2_000_000_000;
const SPLIT_PART_SIZE: u64 = 512 * 1024 * 1024;
const SPLIT_TEMP_BUFFER: usize = 1024 * 1024;

fn requires_split_upload(size: u64) -> bool {
    size > TELEGRAM_SINGLE_FILE_LIMIT
}

#[derive(Debug, Serialize)]
pub struct SplitHealthReport {
    pub is_split: bool,
    pub ok: bool,
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub part_count: usize,
    pub issues: Vec<String>,
}

impl SplitHealthReport {
    fn not_split() -> Self {
        Self {
            is_split: false,
            ok: true,
            filename: None,
            size: None,
            part_count: 0,
            issues: Vec::new(),
        }
    }

    fn split(manifest: &SplitManifest, issues: Vec<String>) -> Self {
        Self {
            is_split: true,
            ok: issues.is_empty(),
            filename: Some(manifest.filename.clone()),
            size: Some(manifest.size),
            part_count: manifest.parts.len(),
            issues,
        }
    }
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

pub async fn create_folder_inner(
    name: &str,
    client: &grammers_client::Client,
    peer_cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
) -> Result<FolderMetadata, String> {
    log::info!("Creating Telegram Channel: {}", name);
    
    let result = client.invoke(&tl::functions::channels::CreateChannel {
        broadcast: true,
        megagroup: false,
        title: format!("{} [TD]", name),
        about: "TeleStash Storage Folder\n[telestash-folder]".to_string(),
        geo_point: None,
        address: None,
        for_import: false,
        forum: false,
        ttl_period: None,
    }).await.map_err(map_error)?;
    
    let (chat_id, access_hash) = match &result {
        tl::enums::Updates::Updates(u) => {
             let chat = u.chats.first().ok_or("No chat in updates")?;
             match chat {
                 tl::enums::Chat::Channel(c) => {
                      let channel_obj = grammers_client::types::Channel { raw: c.clone() };
                      peer_cache.write().await.insert(c.id, grammers_client::types::Peer::Channel(channel_obj));
                      (c.id, c.access_hash.unwrap_or(0))
                 }
                 _ => return Err("Created chat is not a channel".to_string()),
             }
        },
        _ => return Err("Unexpected response (not Updates::Updates)".to_string()), 
    };

    let _ = client.invoke(&tl::functions::messages::SetHistoryTtl {
        peer: tl::enums::InputPeer::Channel(tl::types::InputPeerChannel { channel_id: chat_id, access_hash }),
        period: 0, 
    }).await;
    Ok(FolderMetadata {
        id: chat_id,
        name: name.to_string(),
        parent_id: None,
        username: None,
        is_public: false,
        group_id: None,
        display_order: 0,
    })
}

#[tauri::command]
pub async fn cmd_create_folder(
    name: String,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<FolderMetadata, String> {
    let client_opt = {
        state.client.lock().await.clone()
    };
    
    let mut folder = if client_opt.is_none() {
        let mock_id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        log::info!("[MOCK] Created folder '{}' with ID {}", name, mock_id);
        FolderMetadata {
            id: mock_id,
            name,
            parent_id: None,
            username: None,
            is_public: false,
            group_id: None,
            display_order: 0,
        }
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
        create_folder_inner(&name, &client, &state.peer_cache).await?
    };
    
    // Save to SQLite
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    
    // Calculate new display order
    let mut max_stmt = conn.prepare("SELECT MAX(display_order) FROM folder_metadata").map_err(|e: sqlite::Error| e.to_string())?;
    let mut display_order = 0;
    if let sqlite::State::Row = max_stmt.next().map_err(|e: sqlite::Error| e.to_string())? {
        display_order = max_stmt.read::<Option<i64>, _>(0).ok().flatten().unwrap_or(0) + 1;
    }
    
    let mut insert_stmt = conn
        .prepare("INSERT INTO folder_metadata (channel_id, name, username, is_public, display_order, group_id) VALUES (?, ?, ?, ?, ?, NULL)")
        .map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt.bind((1, folder.id)).map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt.bind((2, folder.name.as_str())).map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt.bind((3, folder.username.as_deref())).map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt.bind((4, if folder.is_public { 1 } else { 0 })).map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt.bind((5, display_order)).map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt.next().map_err(|e: sqlite::Error| e.to_string())?;
    
    folder.display_order = display_order as i32;
    Ok(folder)
}

pub async fn delete_folder_inner(
    folder_id: i64,
    client: &grammers_client::Client,
    peer_cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
) -> Result<bool, String> {
    log::info!("Deleting folder/channel: {}", folder_id);

    let peer = resolve_peer(client, Some(folder_id), peer_cache).await?;
    
    let input_channel = match peer {
        Peer::Channel(c) => {
             let chan = &c.raw;
             tl::enums::InputChannel::Channel(tl::types::InputChannel {
                 channel_id: chan.id,
                 access_hash: chan.access_hash.ok_or("No access hash for channel")?,
              })
        },
        _ => return Err("Only channels (folders) can be deleted.".to_string()),
    };
    
    client.invoke(&tl::functions::channels::DeleteChannel {
        channel: input_channel,
    }).await.map_err(|e| format!("Failed to delete channel: {}", e))?;
    
    Ok(true)
}

#[tauri::command]
pub async fn cmd_delete_folder(
    folder_id: i64,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<bool, String> {
    let client_opt = {
        state.client.lock().await.clone()
    };
    
    if client_opt.is_none() {
        log::info!("[MOCK] Deleted folder ID {}", folder_id);
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
        delete_folder_inner(folder_id, &client, &state.peer_cache).await?;
    }
    
    // Delete from SQLite
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let mut stmt = conn.prepare("DELETE FROM folder_metadata WHERE channel_id = ?").map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((1, folder_id)).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.next().map_err(|e: sqlite::Error| e.to_string())?;
    
    Ok(true)
}

pub async fn rename_folder_inner(
    folder_id: i64,
    new_name: &str,
    client: &grammers_client::Client,
    peer_cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
) -> Result<bool, String> {
    log::info!("Renaming folder/channel: {} to {}", folder_id, new_name);

    let peer = resolve_peer(client, Some(folder_id), peer_cache).await?;
    
    let input_channel = match peer {
        Peer::Channel(c) => {
             let chan = &c.raw;
             tl::enums::InputChannel::Channel(tl::types::InputChannel {
                 channel_id: chan.id,
                 access_hash: chan.access_hash.ok_or("No access hash for channel")?,
              })
        },
        _ => return Err("Only channels (folders) can be renamed.".to_string()),
    };
    
    client.invoke(&tl::functions::channels::EditTitle {
        channel: input_channel,
        title: format!("{} [TD]", new_name),
    }).await.map_err(|e| format!("Failed to rename channel: {}", e))?;
    
    Ok(true)
}

#[tauri::command]
pub async fn cmd_rename_folder(
    folder_id: i64,
    new_name: String,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<bool, String> {
    let client_opt = {
        state.client.lock().await.clone()
    };
    
    if client_opt.is_none() {
        log::info!("[MOCK] Renamed folder ID {} to {}", folder_id, new_name);
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
        rename_folder_inner(folder_id, &new_name, &client, &state.peer_cache).await?;
    }
    
    // Update SQLite
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let mut stmt = conn.prepare("UPDATE folder_metadata SET name = ? WHERE channel_id = ?").map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((1, new_name.as_str())).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((2, folder_id)).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.next().map_err(|e: sqlite::Error| e.to_string())?;
    
    Ok(true)
}


#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    id: String,
    percent: u8,
    uploaded_bytes: u64,
    total_bytes: u64,
    speed_bytes_per_sec: u64,
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
                d.name(),
                d.mime_type(),
                d.size() as u64,
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
    peer: &Peer,
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

async fn inspect_split_parts(
    client: &grammers_client::Client,
    peer: &Peer,
    manifest: &SplitManifest,
) -> SplitHealthReport {
    let mut issues = Vec::new();
    if let Err(e) = validate_split_manifest(manifest) {
        issues.push(e);
    }

    for chunk in manifest.parts.chunks(100) {
        let ids: Vec<i32> = chunk.iter().map(|part| part.message_id).collect();
        let messages = match client.get_messages_by_id(peer, &ids).await {
            Ok(messages) => messages,
            Err(e) => {
                issues.push(format!("Failed to fetch split parts: {}", e));
                return SplitHealthReport::split(manifest, issues);
            }
        };

        for (part, msg) in chunk.iter().zip(messages.into_iter()) {
            let Some(msg) = msg else {
                issues.push(format!("Missing part message_id {}", part.message_id));
                continue;
            };
            let Some(media) = msg.media() else {
                issues.push(format!("Part {} has no media", part.message_id));
                continue;
            };
            if let Some(actual_size) = media_size(&media) {
                if actual_size != part.size {
                    issues.push(format!(
                        "Part {} size mismatch: expected {}, got {}",
                        part.message_id, part.size, actual_size
                    ));
                }
            }
        }
    }

    SplitHealthReport::split(manifest, issues)
}

fn media_size(media: &Media) -> Option<u64> {
    match media {
        Media::Document(d) => Some(d.size() as u64),
        _ => None,
    }
}

async fn upload_path_and_send(
    client: &grammers_client::Client,
    peer: &Peer,
    path: &str,
    upload_name: String,
    caption: String,
    limit: u64,
    net_config: &TransferPolicy,
    tid: &str,
    state: &TelegramState,
    app_handle: &tauri::AppHandle,
    progress_base: u64,
    progress_total: u64,
) -> Result<i32, String> {
    let configured_attempts = net_config.retry_attempts();
    let max_attempts = upload_stream_retry_attempts(configured_attempts);
    let base_ms = net_config.retry_base_backoff_ms();
    let max_ms = net_config.retry_max_backoff_ms();
    let respect_flood = net_config.should_respect_flood_wait();
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

        let (mut reader, file_size, bytes_counter) = ProgressReader::new_with_pause(path, limit, Some(state.paused_transfers.clone()), tid).await?;
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
            get_upload_cancellations().lock().unwrap().insert(tid.to_string(), cancel_tx);
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
                    get_upload_cancellations().lock().unwrap().remove(tid);
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
                if respect_flood && err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        flood_wait_count += 1;
                        if flood_wait_count > flood_wait_attempts {
                            break;
                        }
                        let wait = secs.min(300);
                        log::info!("Respecting FLOOD_WAIT for split upload ({}/{}): sleeping {}s", flood_wait_count, flood_wait_attempts, wait);
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

    let max_send_attempts = if respect_flood {
        flood_wait_attempts
    } else {
        configured_attempts
    };
    let mut send_attempts_made = 0;
    for attempt in 0..=max_send_attempts {
        send_attempts_made = attempt + 1;
        match client.send_message(peer, message.clone()).await {
            Ok(msg) => return Ok(msg.id()),
            Err(e) => {
                let err = map_error(e);
                if respect_flood && err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        last_err = err;
                        if attempt < max_send_attempts {
                            tokio::time::sleep(std::time::Duration::from_secs(secs.min(300))).await;
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
    peer: &Peer,
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

async fn cleanup_split_messages(client: &grammers_client::Client, peer: &Peer, ids: &[i32]) {
    if !ids.is_empty() {
        let _ = delete_message_ids(client, peer, ids, "Split cleanup").await;
    }
}

pub(crate) async fn forward_message_ids_checked(
    client: &grammers_client::Client,
    source_peer: &Peer,
    target_peer: &Peer,
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
    source_peer: &Peer,
    target_peer: &Peer,
    manifest_message_id: i32,
    manifest: SplitManifest,
    app_handle: &tauri::AppHandle,
    state: &TelegramState,
    net_config: &TransferPolicy,
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
        net_config.upload_limit_bytes_per_sec(),
        net_config,
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
    peer: &Peer,
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
    net_config: &TransferPolicy,
    limit: u64,
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
            return split_error(client, &peer, &uploaded_ids, "Transfer cancelled".to_string()).await;
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
                &peer,
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
                return split_error(client, &peer, &uploaded_ids, "Split produced an empty part".to_string()).await;
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                clear_split_upload_state(&resume_path).await;
                return split_error(client, &peer, &uploaded_ids, e).await;
            }
        };

        let upload_name = format!("{}.tdpart{:04}of{:04}", file_name, index + 1, total_parts);
        let caption = format!("{} {} {}/{}", SPLIT_PART_CAPTION_PREFIX, file_name, index + 1, total_parts);
        let part_path_str = part_path.to_string_lossy().to_string();
        let msg_id = match upload_path_and_send(
            client,
            &peer,
            &part_path_str,
            upload_name,
            caption,
            limit,
            net_config,
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
            &peer,
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
        return split_error(client, &peer, &uploaded_ids, e).await;
    }
    if let Err(e) = validate_split_parts_present(client, &peer, &manifest, "Split upload validation").await {
        clear_split_upload_state(&resume_path).await;
        return split_error(client, &peer, &uploaded_ids, e).await;
    }
    let manifest_json = match serde_json::to_vec(&manifest) {
        Ok(json) => json,
        Err(e) => {
            clear_split_upload_state(&resume_path).await;
            return split_error(
                client,
                &peer,
                &uploaded_ids,
                format!("Failed to encode split manifest: {}", e),
            )
            .await;
        }
    };
    let manifest_path = split_temp_path(&file_name, &format!("{{name}}{}", SPLIT_MANIFEST_SUFFIX));
    if let Err(e) = tokio::fs::write(&manifest_path, manifest_json).await {
        clear_split_upload_state(&resume_path).await;
        return split_error(client, &peer, &uploaded_ids, format!("Failed to write split manifest: {}", e)).await;
    }

    let manifest_path_str = manifest_path.to_string_lossy().to_string();
    let manifest_id = match upload_path_and_send(
        client,
        &peer,
        &manifest_path_str,
        SPLIT_MANIFEST_UPLOAD_NAME.to_string(),
        file_name.clone(),
        limit,
        net_config,
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
    peer: &Peer,
    manifest: SplitManifest,
    save_path: &str,
    tid: &str,
    app_handle: &tauri::AppHandle,
    state: &TelegramState,
    bw_state: &BandwidthManager,
    net_config: &TransferPolicy,
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
        let mut chunk_retry_budget = net_config.retry_attempts();

        while let Some(chunk) = iter.next().await.transpose() {
            if state.cancelled_transfers.read().await.contains(tid) {
                state.cancelled_transfers.write().await.remove(tid);
                drop(file);
                cleanup_partial_file(save_path);
                bw_state.release_down(manifest.size);
                return Err("Transfer cancelled".to_string());
            }

            while state.paused_transfers.read().await.contains(tid) {
                let notifier = {
                    let mut map = state.pause_notifiers.lock().unwrap();
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

            let bytes = match chunk {
                Ok(b) => {
                    chunk_retry_budget = net_config.retry_attempts();
                    b
                }
                Err(e) => {
                    let err = map_error(&e);
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
                            net_config.retry_base_backoff_ms(),
                            net_config.retry_max_backoff_ms(),
                        )))
                        .await;
                        continue;
                    }
                    drop(file);
                    cleanup_partial_file(save_path);
                    bw_state.release_down(manifest.size);
                    return Err(format!("Split download chunk error: {}", err));
                }
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
    if let Some(notifier) = state.pause_notifiers.lock().unwrap().remove(&transfer_id) {
        notifier.notify_waiters();
    }
    if let Some(tx) = get_upload_cancellations().lock().unwrap().remove(&transfer_id) {
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
    if let Some(notifier) = state.pause_notifiers.lock().unwrap().remove(&transfer_id) {
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
    net_config: State<'_, std::sync::Arc<TransferPolicy>>,
) -> Result<String, String> {
    cmd_upload_file_inner(
        path,
        folder_id,
        transfer_id,
        app_handle,
        state,
        bw_state,
        net_config,
    ).await
}

async fn cmd_upload_file_inner(
    path: String,
    folder_id: Option<i64>,
    transfer_id: Option<String>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    bw_state: State<'_, Arc<BandwidthManager>>,
    net_config: State<'_, std::sync::Arc<TransferPolicy>>,
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

    let mut limit = 0;
    let user_limit = net_config.upload_limit_bytes_per_sec();
    if user_limit > 0 {
        limit = user_limit;
    }
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
            net_config.inner().as_ref(),
            limit,
        )
        .await;
        if result.is_err() {
            bw_state.release_up(size);
        }
        return result;
    }

    let mut attempt = 0;
    let configured_attempts = net_config.retry_attempts();
    let max_attempts = upload_stream_retry_attempts(configured_attempts);
    let base_ms = net_config.retry_base_backoff_ms();
    let max_ms = net_config.retry_max_backoff_ms();
    let respect_flood = net_config.should_respect_flood_wait();
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
        let (mut reader, file_size, bytes_counter) = match ProgressReader::new_with_pause(&path, limit, Some(state.paused_transfers.clone()), &tid).await {
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
            get_upload_cancellations().lock().unwrap().insert(tid.clone(), cancel_tx);
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
                        get_upload_cancellations().lock().unwrap().remove(&tid);
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

                if respect_flood && err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        flood_wait_count += 1;
                        if flood_wait_count > flood_wait_attempts {
                            break;
                        }
                        let wait = secs.min(300);
                        log::info!("Respecting FLOOD_WAIT for upload ({}/{}): sleeping {}s", flood_wait_count, flood_wait_attempts, wait);
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
    let configured_retries = net_config.retry_attempts();
    let base_ms = net_config.retry_base_backoff_ms();
    let max_ms = net_config.retry_max_backoff_ms();
    let respect_flood = net_config.should_respect_flood_wait();
    let max_retries = if respect_flood {
        flood_wait_retry_attempts(configured_retries)
    } else {
        configured_retries
    };
    let mut last_err = String::new();
    let mut send_attempts_made = 0;

    for attempt in 0..=max_retries {
        send_attempts_made = attempt + 1;
        match client.send_message(&peer, message.clone()).await {
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
                if respect_flood && err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        last_err = err;
                        if attempt < max_retries {
                            let wait = secs.min(300); // cap at 5 min
                            log::info!("Respecting FLOOD_WAIT: sleeping {}s", wait);
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

#[tauri::command]
pub async fn initiate_upload(
    path: String,
    folder_id: Option<i64>,
    transfer_id: Option<String>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    bw_state: State<'_, Arc<BandwidthManager>>,
    net_config: State<'_, std::sync::Arc<TransferPolicy>>,
) -> Result<String, String> {
    cmd_upload_file(
        path,
        folder_id,
        transfer_id,
        app_handle,
        state,
        bw_state,
        net_config,
    ).await
}

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
    let messages = client.get_messages_by_id(&peer, &[message_id])
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

    let input_peer = match &peer {
        Peer::User(u) => {
            if folder_id.is_none() {
                tl::enums::InputPeer::PeerSelf
            } else {
                let (id, access_hash) = match &u.raw {
                    tl::enums::User::User(usr) => (usr.id, usr.access_hash.unwrap_or(0)),
                    tl::enums::User::Empty(usr) => (usr.id, 0),
                };
                tl::enums::InputPeer::User(tl::types::InputPeerUser {
                    user_id: id,
                    access_hash,
                })
            }
        }
        Peer::Channel(c) => {
            tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
                channel_id: c.raw.id,
                access_hash: c.raw.access_hash.ok_or("No access hash for channel")?,
            })
        }
        _ => return Err("Unsupported peer type".to_string()),
    };

    // 1. First attempt: Direct in-place EditMessage
    let edit_res = client.invoke(&tl::functions::messages::EditMessage {
        peer: input_peer,
        id: message_id,
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
        if client.send_message(&peer, input_msg).await.is_ok() {
            let _ = delete_message_ids(&client, &peer, &[message_id], "Rename forwarded message fallback").await;
            return Ok(true);
        }
    }

    edit_res.map_err(|e| format!("Failed to rename file: {}", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn cmd_check_split_file_health(
    message_id: i32,
    folder_id: Option<i64>,
    state: State<'_, TelegramState>,
) -> Result<SplitHealthReport, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        return Ok(SplitHealthReport::not_split());
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;
    let messages = client
        .get_messages_by_id(&peer, &[message_id])
        .await
        .map_err(|e| format!("Failed to fetch message for split health check: {}", e))?;
    let Some(msg) = messages.into_iter().flatten().next() else {
        return Err(format!("Message {} not found", message_id));
    };
    let Some(media) = msg.media() else {
        return Ok(SplitHealthReport::not_split());
    };
    let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await else {
        return Ok(SplitHealthReport::not_split());
    };

    Ok(inspect_split_parts(&client, &peer, &manifest).await)
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
    let messages = client.get_messages_by_id(&peer, &[message_id])
        .await
        .map_err(|e| format!("Failed to fetch message for delete: {}", e))?;
    let msg = messages.iter().flatten().next().ok_or_else(|| format!(
            "Message {} not found in folder {:?}. The file may have already been moved or deleted. Please refresh the folder.",
            message_id, folder_id
    ))?;

    let mut ids = vec![message_id];
    if let Some(media) = msg.media() {
        if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
            validate_split_parts_present(&client, &peer, &manifest, "Split delete").await?;
            ids.extend(manifest.parts.iter().map(|p| p.message_id));
        }
    }

    delete_message_ids(&client, &peer, &ids, "Delete").await?;

    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let srt_path = app_dir.join("streaming").join("captions").join(format!("{}_{}.en.srt", folder_id.unwrap_or(0), message_id));
        if srt_path.exists() {
            let _ = std::fs::remove_file(srt_path);
        }
    }

    Ok(true)
}

#[derive(Debug, serde::Deserialize)]
pub struct DownloadFileRequest {
    message_id: i32,
    save_path: String,
    folder_id: Option<i64>,
    transfer_id: Option<String>,
}

#[tauri::command]
pub async fn cmd_download_file(
    req: DownloadFileRequest,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    bw_state: State<'_, Arc<BandwidthManager>>,
    net_config: State<'_, std::sync::Arc<TransferPolicy>>,
) -> Result<String, String> {
    let tid = req.transfer_id.unwrap_or_default();
    let save_path = req.save_path;
    let folder_id = req.folder_id;
    let message_id = req.message_id;

    let actual_save_path = save_path.clone();

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
    let messages = client.get_messages_by_id(&peer, &[message_id]).await.map_err(|e| e.to_string())?;
    
    let msg = messages.into_iter()
        .flatten()
        .next()
        .ok_or_else(|| "Message not found".to_string())?;

    let media = msg.media()
        .ok_or_else(|| "No media in message".to_string())?;

    if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
        return download_split_file(
            &client,
            &peer,
            manifest,
            &actual_save_path,
            &tid,
            &app_handle,
            state.inner(),
            bw_state.inner().as_ref(),
            net_config.inner().as_ref(),
        )
        .await;
    }

    let expected_file_size = match &media {
        Media::Document(d) => Some(d.size() as u64),
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
    let mut chunk_retry_budget = net_config.retry_attempts();

    while let Some(chunk) = download_iter.next().await.transpose() {
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
                let mut map = state.pause_notifiers.lock().unwrap();
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

        let bytes = match chunk {
            Ok(b) => {
                chunk_retry_budget = net_config.retry_attempts(); // reset on success
                b
            },
            Err(e) => {
                let err = map_error(&e);
                if chunk_retry_budget > 0 {
                    chunk_retry_budget -= 1;
                    log::warn!("Download chunk error (retries left: {}): {}", chunk_retry_budget, err);
                    let delay = backoff_ms(0, net_config.retry_base_backoff_ms(), net_config.retry_max_backoff_ms());
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    continue;
                }
                drop(file);
                cleanup_partial_file(&actual_save_path);
                bw_state.release_down(total_size);
                return Err(format!("Download chunk error: {}", err));
            }
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

#[tauri::command]
pub async fn cmd_move_files(
    message_ids: Vec<i32>,
    source_folder_id: Option<i64>,
    target_folder_id: Option<i64>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    net_config: State<'_, std::sync::Arc<TransferPolicy>>,
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
        .get_messages_by_id(&source_peer, &message_ids)
        .await
        .map_err(|e| format!("Failed to inspect files before move: {}", e))?;

    let mut normal_ids = Vec::new();
    let mut split_moves = Vec::new();
    for (message_id, msg) in message_ids.iter().zip(source_messages.into_iter()) {
        let msg = msg.ok_or_else(|| format!("Message {} not found in source folder. Please refresh and try again.", message_id))?;
        if let Some(media) = msg.media() {
            if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
                validate_split_parts_present(&client, &source_peer, &manifest, "Split move preflight").await?;
                split_moves.push((*message_id, manifest));
                continue;
            }
        }
        normal_ids.push(*message_id);
    }

    for (manifest_message_id, manifest) in split_moves {
        move_split_file(
            &client,
            &source_peer,
            &target_peer,
            manifest_message_id,
            manifest,
            &app_handle,
            state.inner(),
            net_config.inner().as_ref(),
        )
        .await?;
    }

    if !normal_ids.is_empty() {
        let forwarded_ids = forward_message_ids_checked(
            &client,
            &source_peer,
            &target_peer,
            &normal_ids,
            "Move",
        )
        .await?;
        if let Err(e) = delete_message_ids(&client, &source_peer, &normal_ids, "Move source cleanup").await {
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

#[tauri::command]
pub async fn cmd_get_files(
    folder_id: Option<i64>,
    state: State<'_, TelegramState>,
) -> Result<Vec<FileMetadata>, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() { 
        log::info!("[MOCK] Returning mock files for folder {:?}", folder_id);
        return Ok(Vec::new()); // No mock files for now
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
    let mut files = Vec::new();
    
    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;

    let mut msgs = client.iter_messages(&peer);
    while let Some(msg) = msgs.next().await.map_err(|e| e.to_string())? {
        if let Some(doc) = msg.media() {
            let caption = msg.text();
            if crate::models::is_split_part_caption(caption) {
                continue;
            }
            if let Some(manifest) = split_manifest_from_media(&client, &doc, caption).await {
                files.push(FileMetadata {
                    id: msg.id() as i64,
                    folder_id,
                    name: manifest.filename.clone(),
                    size: manifest.size,
                    mime_type: Some(manifest.mime_type),
                    file_ext: manifest.file_ext,
                    created_at: msg.date().to_string(),
                    icon_type: "file".into(),
                });
                continue;
            }
            let (name, size, mime, ext) = match doc {
                Media::Document(d) => {
                    let doc_name = d.name().to_string();
                    // Prefer the message caption (set by rename via EditMessage) over the
                    // document's built-in filename attribute, so renames persist across refreshes.
                    let display_name = if caption.is_empty() { doc_name.clone() } else { caption.to_string() };
                    let s = d.size();
                    let m = d.mime_type().map(|s| s.to_string());
                    // Extension always from the original document name for correct file-type icon
                    let e = std::path::Path::new(&doc_name).extension().map(|os| os.to_str().unwrap_or("").to_string());
                    (display_name, s, m, e)
                },
                Media::Photo(_) => ("Photo.jpg".to_string(), 0, Some("image/jpeg".into()), Some("jpg".into())),
                _ => ("Unknown".to_string(), 0, None, None),
            };
            files.push(FileMetadata {
                id: msg.id() as i64, folder_id, name, size: size as u64, mime_type: mime, file_ext: ext, created_at: msg.date().to_string(), icon_type: "file".into()
            });
        }
    }

    Ok(files)
}

/// Extract FileMetadata entries from a list of Telegram messages returned by SearchGlobal.
fn extract_search_files(msgs: &[tl::enums::Message]) -> Vec<FileMetadata> {
    let mut files = Vec::new();
    for msg in msgs {
        if let tl::enums::Message::Message(m) = msg {
            if let Some(tl::enums::MessageMedia::Document(d)) = &m.media {
                if let Some(tl::enums::Document::Document(doc)) = &d.document {
                    let doc_name = doc.attributes.iter().find_map(|a| match a {
                        tl::enums::DocumentAttribute::Filename(f) => Some(f.file_name.clone()),
                        _ => None
                    }).unwrap_or("Unknown".to_string());
                    // Prefer the message caption over the built-in document filename
                    let name = if m.message.is_empty() { doc_name.clone() } else { m.message.clone() };
                    let size = doc.size as u64;
                    let mime = doc.mime_type.clone();
                    let ext = std::path::Path::new(&doc_name).extension().map(|os| os.to_str().unwrap_or("").to_string());
                    let folder_id = match &m.peer_id {
                        tl::enums::Peer::Channel(c) => Some(c.channel_id),
                        tl::enums::Peer::User(u) => Some(u.user_id),
                        tl::enums::Peer::Chat(c) => Some(c.chat_id),
                    };
                    files.push(FileMetadata {
                        id: m.id as i64, folder_id, name, size,
                        mime_type: Some(mime), file_ext: ext,
                        created_at: m.date.to_string(), icon_type: "file".into()
                    });
                }
            }
        }
    }
    files
}

#[tauri::command]
pub async fn cmd_search_global(
    query: String,
    state: State<'_, TelegramState>,
) -> Result<Vec<FileMetadata>, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() { 
        return Ok(Vec::new());
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
    
    log::info!("Searching global for: {}", query);

    let result = client.invoke(&tl::functions::messages::SearchGlobal {
        q: query,
        filter: tl::enums::MessagesFilter::InputMessagesFilterDocument,
        min_date: 0,
        max_date: 0,
        offset_rate: 0,
        offset_peer: tl::enums::InputPeer::Empty,
        offset_id: 0,
        limit: 50,
        folder_id: None,
        broadcasts_only: false,
        groups_only: false,
        users_only: false,
    }).await.map_err(map_error)?;

    let files = match result {
        tl::enums::messages::Messages::Messages(msgs) => extract_search_files(&msgs.messages),
        tl::enums::messages::Messages::Slice(msgs) => extract_search_files(&msgs.messages),
        _ => Vec::new(),
    };

    Ok(files)
}

#[tauri::command]
pub async fn cmd_scan_folders(
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<Vec<FolderMetadata>, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() { 
        // If not connected, return whatever is already in the database
        return crate::commands::folder_groups::cmd_get_enriched_folders(db_pool).await;
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
    
    let mut folders = Vec::new();
    let mut dialogs = client.iter_dialogs();
    let mut discovered = HashMap::new();
    
    log::info!("Starting Folder Scan...");

    while let Some(dialog) = dialogs.next().await.map_err(|e| e.to_string())? {
        // Populate peer cache for every dialog we encounter (free priming)
        match &dialog.peer {
            Peer::Channel(c) => {
                let id = c.raw.id;
                discovered.insert(id, dialog.peer.clone());

                let name = c.raw.title.clone();
                let access_hash = c.raw.access_hash.unwrap_or(0);
                
                log::debug!("[SCAN] Processing Channel: '{}' (ID: {})", name, id);

                // Strategy 1: Title
                if name.to_lowercase().contains("[td]") {
                    log::info!(" -> MATCH via Title: {}", name);
                    let display_name = name.replace(" [TD]", "").replace(" [td]", "").replace("[TD]", "").replace("[td]", "").trim().to_string();
                    let username = c.raw.username.clone();
                    let is_public = username.is_some();
                    folders.push(FolderMetadata { id, name: display_name, parent_id: None, username, is_public, group_id: None, display_order: 0 });
                    continue; 
                }

                // Strategy 2: About (Only if we are the creator to avoid rate limits on third-party channels)
                if c.raw.creator {
                    let input_chan = tl::enums::InputChannel::Channel(tl::types::InputChannel {
                        channel_id: c.raw.id,
                        access_hash,
                    });
                    
                    match client.invoke(&tl::functions::channels::GetFullChannel {
                        channel: input_chan,
                    }).await {
                        Ok(tl::enums::messages::ChatFull::Full(f)) => {
                            if let tl::enums::ChatFull::Full(cf) = f.full_chat {
                                 if cf.about.contains("[telestash-folder]") {
                                     log::info!(" -> MATCH via About: {}", name);
                                     let username = c.raw.username.clone();
                                     let is_public = username.is_some();
                                     folders.push(FolderMetadata { id, name: name.clone(), parent_id: None, username, is_public, group_id: None, display_order: 0 });
                                 }
                            }
                        },
                        Err(e) => log::warn!(" -> Failed to get full info: {}", e),
                    }
                }
            },
            Peer::User(u) => {
                discovered.insert(u.raw.id(), dialog.peer.clone());
                log::debug!("[SCAN] Cached User Peer: {}", u.raw.id());
            },
            peer => {
                log::debug!("[SCAN] Skipped Peer: {:?}", peer);
            }
        }
    }
    
    {
        let mut cache = state.peer_cache.write().await;
        cache.extend(discovered);
    }
    
    let cache_len = state.peer_cache.read().await.len();
    log::info!("Scan complete. Found {} folders. Peer cache size: {}.", folders.len(), cache_len);
    
    // Enrich folders via the local DB
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let enriched = crate::commands::folder_groups::get_enriched_folders_internal(&conn, folders)?;
    Ok(enriched)
}

/// Zip a folder's contents into a temp file and return the path.
/// The resulting zip preserves the relative directory structure.
#[tauri::command]
pub async fn cmd_zip_folder(
    folder_path: String,
) -> Result<String, String> {
    let src = std::path::Path::new(&folder_path)
        .canonicalize()
        .map_err(|e| format!("Invalid folder path: {}", e))?;
    if !src.is_dir() {
        return Err(format!("'{}' is not a directory", folder_path));
    }

    let folder_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "folder".to_string());

    let zip_path = std::env::temp_dir().join(format!("{}.zip", folder_name));
    let src_owned = src.clone();
    let out_path = zip_path.clone();

    // Run blocking I/O on a dedicated thread so we don't stall the async runtime
    let (zip_path_str, zip_size) = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::create(&out_path)
            .map_err(|e| format!("Failed to create zip file: {}", e))?;
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for entry in walkdir::WalkDir::new(&src_owned).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            let relative = path.strip_prefix(&src_owned).unwrap_or(path);

            if path.is_file() {
                let name = relative.to_string_lossy().to_string();
                zip_writer.start_file(&name, options)
                    .map_err(|e| format!("Failed to add '{}': {}", name, e))?;
                let mut f = std::fs::File::open(path)
                    .map_err(|e| format!("Failed to open '{}': {}", name, e))?;
                std::io::copy(&mut f, &mut zip_writer)
                    .map_err(|e| format!("Failed to write '{}': {}", name, e))?;
            } else if path.is_dir() && path != src_owned {
                let dir_name = format!("{}/", relative.to_string_lossy());
                zip_writer.add_directory(&dir_name, options)
                    .map_err(|e| format!("Failed to add dir '{}': {}", dir_name, e))?;
            }
        }

        zip_writer.finish().map_err(|e| format!("Failed to finalize zip: {}", e))?;
        let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        Ok::<(String, u64), String>((out_path.to_string_lossy().to_string(), size))
    })
    .await
    .map_err(|e| format!("Zip task panicked: {}", e))?
    .map_err(|e: String| e)?;

    log::info!("Zipped '{}' -> '{}' ({} bytes)", folder_name, zip_path_str, zip_size);

    Ok(zip_path_str)
}

/// Delete a temporary zip file created by cmd_zip_folder.
#[tauri::command]
pub async fn cmd_delete_temp_zip(
    path: String,
) -> Result<(), String> {
    let path_clone = path.clone();
    tokio::task::spawn_blocking(move || {
        let p = std::path::Path::new(&path_clone);
        if !p.exists() {
            return Ok(());
        }
        let canonical_p = p.canonicalize().map_err(|e| format!("Invalid path: {}", e))?;
        let tmp = std::env::temp_dir().canonicalize().map_err(|e| format!("Could not resolve temp directory: {}", e))?;
        if !canonical_p.starts_with(&tmp) {
            return Err("Refusing to delete file outside temp directory".to_string());
        }
        std::fs::remove_file(&canonical_p).map_err(|e| e.to_string())?;
        log::info!("Cleaned up temp zip: {}", path_clone);
        Ok(())
    })
    .await
    .map_err(|e| format!("Task panicked: {}", e))?
}

/// Toggle a folder (channel) between private and public.
/// When making public, a username is generated from the channel title.
/// When making private, the username is removed.
#[tauri::command]
pub async fn cmd_toggle_folder_visibility(
    folder_id: i64,
    make_public: bool,
    desired_username: Option<String>,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<FolderMetadata, String> {
    let client_opt = {
        state.client.lock().await.clone()
    };

    let mut folder = if client_opt.is_none() {
        log::info!("[MOCK] Toggle visibility for folder {}. Public: {}", folder_id, make_public);
        FolderMetadata {
            id: folder_id,
            name: "Mock Folder".to_string(),
            parent_id: None,
            username: if make_public { desired_username } else { None },
            is_public: make_public,
            group_id: None,
            display_order: 0,
        }
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

        let peer = resolve_peer(&client, Some(folder_id), &state.peer_cache).await?;
        let (channel_id, access_hash) = match &peer {
            Peer::Channel(c) => (c.raw.id, c.raw.access_hash.ok_or("No access hash for channel")?),
            _ => return Err("Only channels (folders) can be toggled.".to_string()),
        };

        let input_channel = tl::enums::InputChannel::Channel(tl::types::InputChannel {
            channel_id,
            access_hash,
        });

        // Extract channel name from the resolved peer for the return value
        let channel_name = match &peer {
            Peer::Channel(c) => {
                c.raw.title
                    .replace(" [TD]", "")
                    .replace(" [td]", "")
                    .trim()
                    .to_string()
            }
            _ => "Folder".to_string(),
        };

        if make_public {
            // Generate a username from the desired_username or channel title.
            // If desired_username is provided AND non-empty, use it directly;
            // otherwise auto-generate from the channel title.
            let username = if let Some(ref u) = desired_username {
                if !u.is_empty() {
                    Some(u.clone())
                } else {
                    None // empty string → fall through to auto-generation below
                }
            } else {
                None
            };

            let username = match username {
                Some(given) => {
                    // User-provided username: check availability first
                    let available = client
                        .invoke(&tl::functions::channels::CheckUsername {
                            channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                                channel_id,
                                access_hash,
                            }),
                            username: given.clone(),
                        })
                        .await
                        .map_err(|e| format!("Failed to check username availability: {}", map_error(e)))?;
                    if !available {
                        return Err(format!("Username '{}' is not available. Try a different one.", given));
                    }
                    given
                }
                None => {
                    // Auto-generate username from channel title
                    // channel_name already has [TD] stripped above
                    let mut base = channel_name.clone()
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_alphanumeric() || *c == '_')
                        .take(30)
                        .collect::<String>();
                    if base.len() < 5 {
                        let suffix: String = (0..6)
                            .map(|_| char::from(b'a' + (rand::random::<u8>() % 26)))
                            .collect();
                        base = format!("{}_{}", base, suffix);
                    }
                    // Try to find an available username
                    let mut candidate = base.clone();
                    for attempt in 1..=10 {
                        match client
                            .invoke(&tl::functions::channels::CheckUsername {
                                channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                                    channel_id,
                                    access_hash,
                                }),
                                username: candidate.clone(),
                            })
                            .await
                        {
                            Ok(true) => break,
                            _ => {
                                candidate = format!("{}{}", base, attempt);
                                if attempt == 10 {
                                    return Err("Could not find an available username after 10 attempts".to_string());
                                }
                            }
                        }
                    }
                    candidate
                }
            };

            log::info!("Setting channel {} username to '{}'", channel_id, username);
            client
                .invoke(&tl::functions::channels::UpdateUsername {
                    channel: input_channel,
                    username: username.clone(),
                })
                .await
                .map_err(|e| format!("Failed to set username: {}", map_error(e)))?;

            FolderMetadata {
                id: channel_id,
                name: channel_name,
                parent_id: None,
                username: Some(username),
                is_public: true,
                group_id: None,
                display_order: 0,
            }
        } else {
            // Make private: remove username
            log::info!("Removing username from channel {}", channel_id);
            client
                .invoke(&tl::functions::channels::UpdateUsername {
                    channel: input_channel,
                    username: String::new(),
                })
                .await
                .map_err(|e| format!("Failed to remove username: {}", map_error(e)))?;

            FolderMetadata {
                id: channel_id,
                name: channel_name,
                parent_id: None,
                username: None,
                is_public: false,
                group_id: None,
                display_order: 0,
            }
        }
    };

    // Update SQLite cache
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let mut stmt = conn
        .prepare("UPDATE folder_metadata SET username = ?, is_public = ? WHERE channel_id = ?")
        .map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((1, folder.username.as_deref())).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((2, if folder.is_public { 1 } else { 0 })).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((3, folder.id)).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.next().map_err(|e: sqlite::Error| e.to_string())?;

    // Retrieve group_id and display_order from DB to ensure they are returned correctly
    let mut fm_stmt = conn
        .prepare("SELECT group_id, display_order FROM folder_metadata WHERE channel_id = ?")
        .map_err(|e: sqlite::Error| e.to_string())?;
    fm_stmt.bind((1, folder.id)).map_err(|e: sqlite::Error| e.to_string())?;
    if let sqlite::State::Row = fm_stmt.next().map_err(|e: sqlite::Error| e.to_string())? {
        folder.group_id = fm_stmt.read::<Option<i64>, _>("group_id").ok().flatten().map(|id| id as i32);
        folder.display_order = fm_stmt.read::<i64, _>("display_order").map_err(|e: sqlite::Error| e.to_string())? as i32;
    }

    Ok(folder)
}

/// Export a Telegram invite link for a folder (channel).
/// For public channels, returns the t.me/username link directly.
/// For private channels, exports a hash-based invite link via the API.
#[derive(Debug, Serialize)]
pub struct FolderInviteInfo {
    pub link: String,
    pub is_public: bool,
    pub username: Option<String>,
}

#[tauri::command]
pub async fn cmd_export_folder_invite(
    folder_id: i64,
    state: State<'_, TelegramState>,
) -> Result<FolderInviteInfo, String> {
    let client_opt = {
        state.client.lock().await.clone()
    };

    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        log::info!("[MOCK] Export invite for folder {}", folder_id);
        return Ok(FolderInviteInfo {
            link: "https://t.me/joinchat/mock-invite-hash".to_string(),
            is_public: false,
            username: None,
        });
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    let peer = resolve_peer(&client, Some(folder_id), &state.peer_cache).await?;
    let (channel_id, access_hash) = match &peer {
        Peer::Channel(c) => (c.raw.id, c.raw.access_hash.ok_or("No access hash for channel")?),
        _ => return Err("Only channels (folders) can have invite links.".to_string()),
    };

    // Check if channel already has a public username (use the resolved peer directly)
    let username: Option<String> = match &peer {
        Peer::Channel(c) => c.raw.username.clone(),
        _ => None,
    };

    if let Some(ref uname) = username {
        // Public channel: return the t.me/username link
        Ok(FolderInviteInfo {
            link: format!("https://t.me/{}", uname),
            is_public: true,
            username: Some(uname.clone()),
        })
    } else {
        // Private channel: export an invite link
        let result = client
            .invoke(&tl::functions::messages::ExportChatInvite {
                peer: tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
                    channel_id,
                    access_hash,
                }),
                legacy_revoke_permanent: false,
                request_needed: false,
                expire_date: None,
                usage_limit: None,
                title: None,
                subscription_pricing: None,
            })
            .await
            .map_err(|e| format!("Failed to export invite: {}", map_error(e)))?;

        let link = match result {
            tl::enums::ExportedChatInvite::ChatInviteExported(c) => c.link,
            tl::enums::ExportedChatInvite::ChatInvitePublicJoinRequests => {
                return Err("Public join request channels do not have a custom private invite link. Share the public username directly instead.".to_string());
            }
        };

        Ok(FolderInviteInfo {
            link,
            is_public: false,
            username: None,
        })
    }
}

#[derive(Clone, serde::Serialize)]
struct RemoteProgressPayload {
    id: String,
    phase: &'static str,
    percent: u8,
    speed: u64,
    uploaded_bytes: u64,
    total_bytes: u64,
}

#[tauri::command]
pub async fn cmd_upload_from_url(
    url: String,
    folder_id: Option<i64>,
    transfer_id: String,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    bw_state: State<'_, Arc<BandwidthManager>>,
    net_config: State<'_, std::sync::Arc<TransferPolicy>>,
) -> Result<String, String> {
    let client_builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10));
    
    let client = client_builder.build().map_err(|e| e.to_string())?;

    let res = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let headers = res.headers();

    // Reject HTML pages — they're download gateways, not actual files
    if let Some(ct) = headers.get(reqwest::header::CONTENT_TYPE) {
        let ct_str = ct.to_str().unwrap_or_default().to_lowercase();
        if ct_str.contains("text/html") {
            return Err("URL returned an HTML page, not a downloadable file. The server may require a direct download link or authentication.".to_string());
        }
    }

    // Prefer Content-Disposition filename over URL path extraction
    let server_filename: Option<String> = headers.get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(|header_value| {
            // Parse RFC 6266/5987 Content-Disposition: attachment; filename="..." or filename*=UTF-8''...
            // Look for filename* first (RFC 5987), then filename
            if let Some(encoded) = header_value.split(';')
                .map(|p| p.trim())
                .find(|p| p.starts_with("filename*="))
                .and_then(|p| p.strip_prefix("filename*="))
            {
                // filename*=UTF-8''percent%20encoded
                if let Some((_charset, value)) = encoded.split_once('\'') {
                    let value = value.split('\'').last().unwrap_or(value);
                    urlencoding::decode(value).ok()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.into_owned())
                } else {
                    None
                }
            } else {
                header_value.split(';')
                    .map(|p| p.trim())
                    .find(|p| p.starts_with("filename="))
                    .and_then(|p| p.strip_prefix("filename="))
                    .map(|f| f.trim_matches('"').to_string())
                    .filter(|f| !f.is_empty())
            }
        });

    let known_size: Option<u64> = headers.get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let temp_dir = std::env::temp_dir();

    if let Some(sz) = known_size {
        let free_space = tokio::task::spawn_blocking({
            let temp_dir = temp_dir.clone();
            move || {
                let disks = sysinfo::Disks::new_with_refreshed_list();
                disks.iter()
                    .filter(|d| temp_dir.starts_with(d.mount_point()))
                    .map(|d| d.available_space())
                    .next()
                    .unwrap_or(u64::MAX)
            }
        }).await.map_err(|e| format!("Disk check panicked: {}", e))?;
        if free_space < sz + 52_428_800 {
            return Err("Insufficient disk space in temp directory.".to_string());
        }
        bw_state.try_reserve_down(sz)?;
        if let Err(e) = bw_state.try_reserve_up(sz) {
            bw_state.release_down(sz);
            return Err(e);
        }
    }

    let display_total = known_size.unwrap_or(0); // 0 = unknown size to frontend
    let _ = app_handle.emit("remote-upload-progress", RemoteProgressPayload {
        id: transfer_id.clone(),
        phase: "downloading",
        percent: 0,
        speed: 0,
        uploaded_bytes: 0,
        total_bytes: display_total,
    });

    let temp_file_path = temp_dir.join(format!("tg_drive_{}.tmp", transfer_id));
    let temp_file_str = temp_file_path.to_string_lossy().to_string();

    let mut downloaded = 0u64;
    let mut range_supported = false;

    if let Some(accept_ranges) = headers.get(reqwest::header::ACCEPT_RANGES) {
        if accept_ranges.to_str().unwrap_or_default() == "bytes" {
            range_supported = true;
        }
    }

    if temp_file_path.exists() {
        if range_supported && known_size.is_some() {
            if let Ok(metadata) = std::fs::metadata(&temp_file_path) {
                downloaded = metadata.len();
                let sz = known_size.unwrap();
                if downloaded >= sz {
                    downloaded = sz;
                }
            }
        } else {
            // No resumption without both range support and a known total size
            let _ = std::fs::remove_file(&temp_file_path);
        }
    }

    let need_download = known_size.map_or(true, |sz| downloaded < sz);

    let stream_res = if downloaded > 0 && need_download {
        let req = client.get(&url)
            .header(reqwest::header::RANGE, format!("bytes={}-", downloaded));
        match req.send().await {
            Ok(r) => r,
            Err(e) => {
                if let Some(sz) = known_size {
                    bw_state.release_down(sz);
                    bw_state.release_up(sz);
                }
                return Err(e.to_string());
            }
        }
    } else {
        res
    };

    let mut file = if downloaded > 0 && need_download {
        let status = stream_res.status();
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            match tokio::fs::OpenOptions::new()
                .write(true)
                .append(true)
                .open(&temp_file_path)
                .await {
                    Ok(f) => f,
                    Err(e) => {
                        if let Some(sz) = known_size {
                            bw_state.release_down(sz);
                            bw_state.release_up(sz);
                        }
                        return Err(e.to_string());
                    }
                }
        } else {
            downloaded = 0;
            match tokio::fs::File::create(&temp_file_path).await {
                Ok(f) => f,
                Err(e) => {
                    if let Some(sz) = known_size {
                        bw_state.release_down(sz);
                        bw_state.release_up(sz);
                    }
                    return Err(e.to_string());
                }
            }
        }
    } else if !need_download {
        match tokio::fs::OpenOptions::new()
            .read(true)
            .open(&temp_file_path)
            .await {
                Ok(f) => f,
                Err(e) => {
                    if let Some(sz) = known_size {
                        bw_state.release_down(sz);
                        bw_state.release_up(sz);
                    }
                    return Err(e.to_string());
                }
            }
    } else {
        match tokio::fs::File::create(&temp_file_path).await {
            Ok(f) => f,
            Err(e) => {
                if let Some(sz) = known_size {
                    bw_state.release_down(sz);
                    bw_state.release_up(sz);
                }
                return Err(e.to_string());
            }
        }
    };

    if need_download {
        let mut stream = stream_res.bytes_stream();
        let mut last_emit_time = std::time::Instant::now();
        let mut last_emit_bytes = downloaded;

        while let Some(chunk_result) = futures::StreamExt::next(&mut stream).await {
            if state.cancelled_transfers.read().await.contains(&transfer_id) {
                state.cancelled_transfers.write().await.remove(&transfer_id);
                drop(file);
                let _ = tokio::fs::remove_file(&temp_file_path).await;
                if let Some(sz) = known_size {
                    bw_state.release_down(sz);
                    bw_state.release_up(sz);
                }
                return Err("Transfer cancelled".to_string());
            }

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    if let Some(sz) = known_size {
                        bw_state.release_down(sz);
                        bw_state.release_up(sz);
                    }
                    return Err(e.to_string());
                }
            };

            if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await {
                if let Some(sz) = known_size {
                    bw_state.release_down(sz);
                    bw_state.release_up(sz);
                }
                return Err(e.to_string());
            }
            downloaded += chunk.len() as u64;

            let now = std::time::Instant::now();
            let dt = now.duration_since(last_emit_time).as_secs_f64();
            let emit_total = known_size.unwrap_or(downloaded);
            let emit_done = known_size.map_or(false, |sz| downloaded >= sz);
            if dt >= 0.25 || emit_done {
                let speed = if dt > 0.0 { ((downloaded - last_emit_bytes) as f64 / dt) as u64 } else { 0 };
                let percent = if let Some(sz) = known_size {
                    if sz > 0 { ((downloaded as f64 / sz as f64) * 100.0).min(99.0) as u8 } else { 0 }
                } else {
                    0u8
                };

                let _ = app_handle.emit("remote-upload-progress", RemoteProgressPayload {
                    id: transfer_id.clone(),
                    phase: "downloading",
                    percent,
                    speed,
                    uploaded_bytes: downloaded,
                    total_bytes: emit_total,
                });
                last_emit_time = now;
                last_emit_bytes = downloaded;
            }

        }

        if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut file).await {
            if let Some(sz) = known_size {
                bw_state.release_down(sz);
                bw_state.release_up(sz);
            }
            return Err(e.to_string());
        }
        if let Err(e) = file.sync_all().await {
            if let Some(sz) = known_size {
                bw_state.release_down(sz);
                bw_state.release_up(sz);
            }
            return Err(e.to_string());
        }
    }

    drop(file);
    if let Some(sz) = known_size {
        bw_state.release_down(sz);
        // Release the upfront upload reservation — we'll re-reserve based on actual size below
        bw_state.release_up(sz);
    }

    // Determine actual file size from disk (authoritative, works even without Content-Length)
    let actual_size = tokio::fs::metadata(&temp_file_path)
        .await
        .map_err(|e| format!("Failed to read downloaded file metadata: {}", e))?
        .len();

    if actual_size == 0 {
        let _ = tokio::fs::remove_file(&temp_file_path).await;
        return Err("Downloaded file is empty".to_string());
    }

    // Reserve upload bandwidth based on the real file size (handles both known and unknown upfront)
    if let Err(e) = bw_state.try_reserve_up(actual_size) {
        let _ = tokio::fs::remove_file(&temp_file_path).await;
        return Err(e);
    }

    let client_opt = { state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => {
            bw_state.release_up(actual_size);
            let _ = tokio::fs::remove_file(&temp_file_path).await;
            return Err("Client not connected".to_string());
        }
    };

    let _ = app_handle.emit("remote-upload-progress", RemoteProgressPayload {
        id: transfer_id.clone(),
        phase: "uploading",
        percent: 0,
        speed: 0,
        uploaded_bytes: 0,
        total_bytes: actual_size,
    });

    let file_name = server_filename.clone().unwrap_or_else(|| {
        reqwest::Url::parse(&url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|segs| segs.last())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "remote_file".to_string())
    });

    let mut limit = 0;
    let user_limit = net_config.upload_limit_bytes_per_sec();
    if user_limit > 0 {
        limit = user_limit;
    }
    if requires_split_upload(actual_size) {
        let result = upload_large_file_split(
            &temp_file_str,
            folder_id,
            file_name,
            actual_size,
            &transfer_id,
            &app_handle,
            state.inner(),
            &client,
            net_config.inner().as_ref(),
            limit,
        )
        .await;
        let _ = tokio::fs::remove_file(&temp_file_path).await;
        if result.is_err() {
            bw_state.release_up(actual_size);
        }
        return result;
    }

    let mut attempt = 0;
    let configured_attempts = net_config.retry_attempts();
    let max_attempts = upload_stream_retry_attempts(configured_attempts);
    let respect_flood = net_config.should_respect_flood_wait();
    let flood_wait_attempts = flood_wait_retry_attempts(configured_attempts);
    let base_ms = net_config.retry_base_backoff_ms();
    let max_ms = net_config.retry_max_backoff_ms();
    let mut last_err = String::new();
    let mut attempts_made = 0;
    let mut uploaded_file = None;
    let mut flood_wait_count = 0;

    while attempt <= max_attempts {
        if state.cancelled_transfers.read().await.contains(&transfer_id) {
            state.cancelled_transfers.write().await.remove(&transfer_id);
            bw_state.release_up(actual_size);
            let _ = tokio::fs::remove_file(&temp_file_path).await;
            return Err("Transfer cancelled".to_string());
        }

        let (mut reader, file_size, bytes_counter) = match ProgressReader::new_with_pause(&temp_file_str, limit, Some(state.paused_transfers.clone()), &transfer_id).await {
            Ok(res) => res,
            Err(e) => {
                bw_state.release_up(actual_size);
                let _ = tokio::fs::remove_file(&temp_file_path).await;
                return Err(e);
            }
        };

        let cancelled = state.cancelled_transfers.clone();
        let progress_tid = transfer_id.clone();
        let progress_handle = app_handle.clone();
        let progress_counter = bytes_counter.clone();
        let progress_task = tokio::spawn(async move {
            let mut last_bytes: u64 = 0;
            let mut last_time = std::time::Instant::now();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let current = progress_counter.load(std::sync::atomic::Ordering::Relaxed);
                let now = std::time::Instant::now();
                let dt = now.duration_since(last_time).as_secs_f64();
                let speed = if dt > 0.0 { ((current - last_bytes) as f64 / dt) as u64 } else { 0 };
                let percent = if file_size > 0 { ((current as f64 / file_size as f64) * 100.0).min(99.0) as u8 } else { 0 };

                let _ = progress_handle.emit("remote-upload-progress", RemoteProgressPayload {
                    id: progress_tid.clone(),
                    phase: "uploading",
                    percent,
                    speed,
                    uploaded_bytes: current,
                    total_bytes: file_size,
                });

                last_bytes = current;
                last_time = now;

                if current >= file_size { break; }
                if cancelled.read().await.contains(&progress_tid) { break; }
            }
        });

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        get_upload_cancellations().lock().unwrap().insert(transfer_id.clone(), cancel_tx);

        let client_clone = client.clone();
        let name_clone = file_name.clone();
        let mut upload_task = tokio::spawn(async move {
            client_clone.upload_stream(&mut reader, file_size as usize, name_clone).await
        });

        attempts_made = attempt + 1;

        let upload_result = {
            tokio::select! {
                res = &mut upload_task => {
                    get_upload_cancellations().lock().unwrap().remove(&transfer_id);
                    res.map_err(|e| format!("Task join error: {}", e))
                }
                _ = cancel_rx => {
                    log::info!("Aborting remote upload task for transfer ID: {}", transfer_id);
                    upload_task.abort();
                    state.cancelled_transfers.write().await.remove(&transfer_id);
                    progress_task.abort();
                    bw_state.release_up(actual_size);
                    let _ = tokio::fs::remove_file(&temp_file_path).await;
                    return Err("Transfer cancelled".to_string());
                }
            }
        };

        progress_task.abort();

        match upload_result {
            Ok(Ok(file)) => {
                uploaded_file = Some(file);
                break;
            }
            Ok(Err(e)) => {
                let err = map_error(e);
                log::warn!("Remote upload_stream attempt {}/{}: {}", attempt + 1, max_attempts + 1, err);
                last_err = format!("{}: {}", upload_error_kind(&err), err);
                record_transfer_log(
                    "Remote upload",
                    format!("Remote upload attempt {}/{} failed for {}", attempt + 1, max_attempts + 1, file_name),
                    Some(format!(
                        "transfer_id: {}\nfile_size: {}\nerror_kind: {}\nretry: {}\nerror: {}",
                        transfer_id,
                        file_size,
                        upload_error_kind(&err),
                        should_retry_upload_error(&err, attempt, configured_attempts),
                        err
                    )),
                );

                if respect_flood && err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        flood_wait_count += 1;
                        if flood_wait_count > flood_wait_attempts {
                            break;
                        }
                        let wait = secs.min(300);
                        log::info!("Respecting FLOOD_WAIT for remote upload ({}/{}): sleeping {}s", flood_wait_count, flood_wait_attempts, wait);
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                        continue;
                    }
                }

                if should_retry_upload_error(&err, attempt, configured_attempts) {
                    let delay = backoff_ms(attempt, base_ms, max_ms);
                    log::info!("Retrying remote upload in {}ms...", delay);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                } else {
                    break;
                }
            }
            Err(e) => {
                log::warn!("Remote upload_stream task failed attempt {}/{}: {}", attempt + 1, max_attempts + 1, e);
                last_err = e;
                record_transfer_log(
                    "Remote upload",
                    format!("Remote upload task attempt {}/{} failed for {}", attempt + 1, max_attempts + 1, file_name),
                    Some(format!(
                        "transfer_id: {}\nfile_size: {}\nretry: {}\nerror: {}",
                        transfer_id,
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
            bw_state.release_up(actual_size);
            let _ = tokio::fs::remove_file(&temp_file_path).await;
            return Err(format!("Remote upload failed after {} attempts: {}", attempts_made, last_err));
        }
    };

    let message = InputMessage::new().text(file_name.clone()).file(uploaded_file);

    let peer = match resolve_peer(&client, folder_id, &state.peer_cache).await {
        Ok(p) => p,
        Err(e) => {
            bw_state.release_up(actual_size);
            let _ = tokio::fs::remove_file(&temp_file_path).await;
            return Err(e);
        }
    };

    let configured_retries = net_config.retry_attempts();
    let base_ms = net_config.retry_base_backoff_ms();
    let max_ms = net_config.retry_max_backoff_ms();
    let respect_flood = net_config.should_respect_flood_wait();
    let max_retries = if respect_flood {
        flood_wait_retry_attempts(configured_retries)
    } else {
        configured_retries
    };
    let mut last_err = String::new();
    let mut send_success = false;
    let mut send_attempts_made = 0;

    for attempt in 0..=max_retries {
        send_attempts_made = attempt + 1;
        match client.send_message(&peer, message.clone()).await {
            Ok(_) => {
                send_success = true;
                break;
            }
            Err(e) => {
                let err = map_error(e);
                log::warn!("send_message attempt {}/{}: {}", attempt + 1, max_retries + 1, err);

                if respect_flood && err.starts_with("FLOOD_WAIT_") {
                    if let Ok(secs) = err.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        last_err = err;
                        if attempt < max_retries {
                            let wait = secs.min(300);
                            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                            continue;
                        }
                        break;
                    }
                }

                last_err = err;
                if attempt < configured_retries {
                    let delay = backoff_ms(attempt, base_ms, max_ms);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                } else {
                    break;
                }
            }
        }
    }

    let _ = tokio::fs::remove_file(&temp_file_path).await;

    if send_success {
        let _ = app_handle.emit("remote-upload-progress", RemoteProgressPayload {
            id: transfer_id,
            phase: "uploading",
            percent: 100,
            speed: 0,
            uploaded_bytes: actual_size,
            total_bytes: actual_size,
        });
        Ok("File uploaded successfully".to_string())
    } else {
        bw_state.release_up(actual_size);
        Err(format!("Upload failed after {} attempts: {}", send_attempts_made, last_err))
    }
}
