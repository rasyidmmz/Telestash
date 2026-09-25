use actix_web::{get, post, delete, patch, web, HttpRequest, HttpResponse, Responder};
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use tokio::io::{AsyncRead, AsyncWriteExt};
use actix_web::web::Bytes;
use std::task::{Context, Poll};
use crate::commands::TelegramState;
use crate::commands::utils::resolve_peer;
use crate::commands::fs::{
    delete_message_ids, split_manifest_from_media, upload_path_and_send,
    validate_split_parts_present,
};
use crate::commands::{create_folder_inner, delete_folder_inner, rename_folder_inner};
use crate::models::FolderMetadata;
use crate::bandwidth::BandwidthManager;
use crate::transfer_retry::ARCHIVE_MAX_BYTES;
use grammers_client::media::Media;
use grammers_client::peer::Peer;
use grammers_tl_types as tl;
use serde::Serialize;
use std::sync::Arc;
use std::collections::HashMap;
use std::io::Write;

/// Shared state for the API server — holds the key hash for auth checks
pub struct ApiState {
    pub key_hash: Option<String>,
}

/// Cache directory paths used by the API server for cleanup operations.
/// The preview cache lives on disk and can become stale when files are
/// moved (forwarded → new message IDs).
pub struct CacheDirs {
    pub preview_dir: std::path::PathBuf,
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

fn json_error(code: &str, message: &str, status: u16) -> HttpResponse {
    let body = ErrorBody {
        error: ErrorDetail {
            code: code.to_string(),
            message: message.to_string(),
        },
    };
    HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap())
        .json(body)
}

struct CleanupStream {
    file: tokio::fs::File,
    path: std::path::PathBuf,
}

impl futures::Stream for CleanupStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = [0u8; 16384];
        let mut read_buf = tokio::io::ReadBuf::new(&mut buf);
        // SAFETY: we project the pin to the file field — no other field is moved.
        let file_pin = unsafe { self.map_unchecked_mut(|s| &mut s.file) };
        match file_pin.poll_read(cx, &mut read_buf) {
            Poll::Ready(Ok(())) => {
                let filled = read_buf.filled();
                if filled.is_empty() {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Ok(Bytes::copy_from_slice(filled))))
                }
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for CleanupStream {
    fn drop(&mut self) {
        let path = self.path.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::remove_file(path).await;
        });
    }
}

fn peer_to_input_peer(peer: grammers_session::types::PeerRef) -> Result<tl::enums::InputPeer, String> {
    Ok(tl::enums::InputPeer::from(peer))
}

/// Spawn a blocking task to delete stale preview cache entries for the given
/// message IDs in the given source folder.
/// Best-effort: failures are silently ignored since cache cleanup is non-critical.
fn spawn_cache_cleanup(
    prev_dir: std::path::PathBuf,
    ids: Vec<i32>,
    folder_key: String,
) {
    tokio::task::spawn_blocking(move || {
        for mid in &ids {
            let prefix = format!("{}_{}.", folder_key, mid);
            if let Ok(entries) = std::fs::read_dir(&prev_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() { continue; }
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        if fname.starts_with(&prefix) {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    });
}

/// Validate X-API-Key header against stored hash
fn check_auth(req: &HttpRequest, api_state: &web::Data<ApiState>) -> Result<(), HttpResponse> {
    let key_hash = match &api_state.key_hash {
        Some(h) => h,
        None => return Err(json_error("NO_KEY_CONFIGURED", "No API key has been configured. Generate one in Settings.", 401)),
    };

    let provided = req
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    match provided {
        Some(key) if crate::commands::api_settings::verify_key(key, key_hash) => Ok(()),
        Some(_) => Err(json_error("UNAUTHORIZED", "Invalid API key", 401)),
        None => Err(json_error("UNAUTHORIZED", "Missing X-API-Key header", 401)),
    }
}

// ──────────────────────────────── Endpoints ────────────────────────────────

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

#[get("/api/v1/health")]
async fn api_health() -> impl Responder {
    HttpResponse::Ok().json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[derive(serde::Deserialize, Clone)]
struct FilesQuery {
    #[allow(dead_code)]
    folder_id: Option<String>,
    page: Option<u32>,
    limit: Option<u32>,
    search: Option<String>,
    offset_id: Option<i32>,
    sort: Option<String>,
    order: Option<String>,
    mime_type: Option<String>,
    created_after: Option<String>,
    created_before: Option<String>,
    size_min: Option<u64>,
    size_max: Option<u64>,
}

#[derive(Serialize)]
struct FilesResponse {
    data: Vec<serde_json::Value>,
    page: u32,
    limit: u32,
    total: usize,
    pagination: PaginationInfo,
}

#[derive(Serialize)]
struct PaginationInfo {
    page: u32,
    limit: u32,
    total: usize,
    total_pages: u32,
    has_next: bool,
    has_prev: bool,
}

#[derive(Serialize, Clone)]
struct ApiFile {
    id: i64,
    folder_id: Option<i64>,
    name: String,
    size: u64,
    mime_type: Option<String>,
    created_at: String,
}

#[get("/api/v1/files")]
async fn api_list_files(
    req: HttpRequest,
    query: web::Query<FilesQuery>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let query_string = req.query_string();
    let has_folder_id = query_string.split('&').any(|p| p.starts_with("folder_id=") || p == "folder_id");

    let mut peers_to_scan = Vec::new();
    if !has_folder_id {
        // Return files from ALL folders: scan dialogs + root folder
        if let Ok(me_peer) = resolve_peer(&client, None, &tg_state.peer_cache).await {
            peers_to_scan.push((None, me_peer));
        }
        let mut dialogs = client.iter_dialogs();
        while let Some(dialog) = dialogs.next().await.ok().flatten() {
            if let Peer::Channel(ref c) = dialog.peer {
                let name = c.raw.title.clone();
                if name.to_lowercase().contains("[td]") {
                    if let Ok(Some(pr)) = dialog.peer.to_ref().await { peers_to_scan.push((Some(c.raw.id), pr)); }
                }
            }
        }
    } else {
        // Parse folder_id value
        let mut parsed_id: Option<i64> = None;
        for pair in query_string.split('&') {
            let mut parts = pair.split('=');
            if let Some(key) = parts.next() {
                if key == "folder_id" {
                    if let Some(val) = parts.next() {
                        if !val.is_empty() && val != "null" && val != "none" && val != "None" {
                            if let Ok(id) = val.parse::<i64>() {
                                parsed_id = Some(id);
                            }
                        }
                    }
                }
            }
        }
        
        let resolved = match resolve_peer(&client, parsed_id, &tg_state.peer_cache).await {
            Ok(p) => p,
            Err(e) => return json_error("PEER_ERROR", &e, 400),
        };
        peers_to_scan.push((parsed_id, resolved));
    }

    let mut all_files: Vec<ApiFile> = Vec::new();
    for (fid, peer) in &peers_to_scan {
        let mut msgs = client.iter_messages(*peer);
        if let Some(offset_id) = query.offset_id {
            msgs = msgs.offset_id(offset_id);
        }
        
        // When listing all, limit scan per folder to prevent rate limit timeouts
        if !has_folder_id {
            msgs = msgs.limit(100);
        } else if query.search.is_none() {
            let page = query.page.unwrap_or(1).clamp(1, u32::MAX);
            let limit = query.limit.unwrap_or(20).clamp(1, 100);
            if query.offset_id.is_some() {
                msgs = msgs.limit(limit as usize * 2);
            } else {
                msgs = msgs.limit(page as usize * limit as usize * 2);
            }
        } else {
            msgs = msgs.limit(2000);
        }

        while let Some(msg) = msgs.next().await.ok().flatten() {
            if let Some(doc) = msg.media() {
                let (name, size, mime) = match doc {
                    Media::Document(d) => {
                        let doc_name = d.name().unwrap_or_default().to_string();
                        // Prefer the message caption (set by rename via EditMessage)
                        let caption = msg.text();
                        let display_name = if caption.is_empty() { doc_name } else { caption.to_string() };
                        (display_name, d.size().unwrap_or(0), d.mime_type().map(|s| s.to_string()))
                    }
                    Media::Photo(_) => ("Photo.jpg".to_string(), 0, Some("image/jpeg".into())),
                    _ => ("Unknown".to_string(), 0, None),
                };

                all_files.push(ApiFile {
                    id: msg.id() as i64,
                    folder_id: *fid,
                    name,
                    size: size as u64,
                    mime_type: mime,
                    created_at: msg.date().to_string(),
                });
            }
        }
    }

    // Apply filters
    let mut filtered_files: Vec<ApiFile> = Vec::new();
    for file in all_files {
        if let Some(ref search) = query.search {
            if !file.name.to_lowercase().contains(&search.to_lowercase()) {
                continue;
            }
        }
        if let Some(ref mt) = query.mime_type {
            if let Some(ref fmt) = file.mime_type {
                if !fmt.to_lowercase().contains(&mt.to_lowercase()) {
                    continue;
                }
            } else {
                continue;
            }
        }
        if let Some(min) = query.size_min {
            if file.size < min {
                continue;
            }
        }
        if let Some(max) = query.size_max {
            if file.size > max {
                continue;
            }
        }
        if let Some(ref after) = query.created_after {
            if file.created_at < *after {
                continue;
            }
        }
        if let Some(ref before) = query.created_before {
            if file.created_at > *before {
                continue;
            }
        }
        filtered_files.push(file);
    }

    // Sort
    let sort_field = query.sort.as_deref().unwrap_or("created_at");
    let sort_order = query.order.as_deref().unwrap_or("asc");
    filtered_files.sort_by(|a, b| {
        let cmp = match sort_field {
            "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            "size" => a.size.cmp(&b.size),
            _ => a.created_at.cmp(&b.created_at),
        };
        if sort_order.to_lowercase() == "desc" {
            cmp.reverse()
        } else {
            cmp
        }
    });

    // Pagination
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let total = filtered_files.len();
    let total_pages = (total.div_ceil(limit as usize)) as u32;
    let start = ((page - 1) * limit) as usize;

    let paginated_files: Vec<ApiFile> = filtered_files
        .into_iter()
        .skip(start)
        .take(limit as usize)
        .collect();

    let has_next = page < total_pages;
    let has_prev = page > 1;

    let final_data: Vec<serde_json::Value> = paginated_files
        .into_iter()
        .map(|file| serde_json::json!({
            "id": file.id,
            "folder_id": file.folder_id,
            "name": file.name,
            "size": file.size,
            "mime_type": file.mime_type,
            "created_at": file.created_at,
        }))
        .collect();

    let res_body = FilesResponse {
        data: final_data,
        page,
        limit,
        total,
        pagination: PaginationInfo {
            page,
            limit,
            total,
            total_pages,
            has_next,
            has_prev,
        },
    };

    HttpResponse::Ok().json(res_body)
}

#[derive(serde::Deserialize)]
struct FolderQuery {
    folder_id: Option<i64>,
}

#[get("/api/v1/files/{message_id}")]
async fn api_get_file(
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<FolderQuery>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let message_id = path.into_inner() as i32;
    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let peer = match resolve_peer(&client, query.folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => return json_error("PEER_ERROR", &e, 400),
    };

    match client.get_messages_by_id(peer, &[message_id]).await {
        Ok(messages) => {
            if let Some(Some(msg)) = messages.first() {
                if let Some(doc) = msg.media() {
                    let (name, size, mime) = match doc {
                        Media::Document(d) => {
                            let doc_name = d.name().unwrap_or_default().to_string();
                            let caption = msg.text();
                            let display_name = if caption.is_empty() { doc_name } else { caption.to_string() };
                            (display_name, d.size().unwrap_or(0), d.mime_type().map(|s| s.to_string()))
                        }
                        Media::Photo(_) => ("Photo.jpg".to_string(), 0, Some("image/jpeg".into())),
                        _ => ("Unknown".to_string(), 0, None),
                    };
                    return HttpResponse::Ok().json(ApiFile {
                        id: msg.id() as i64,
                        folder_id: query.folder_id,
                        name,
                        size: size as u64,
                        mime_type: mime,
                        created_at: msg.date().to_string(),
                    });
                }
            }
            json_error("NOT_FOUND", "File not found", 404)
        }
        Err(e) => json_error("FETCH_ERROR", &format!("Failed to fetch file: {}", e), 500),
    }
}

#[get("/api/v1/files/{message_id}/download")]
async fn api_download_file(
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<FolderQuery>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let message_id = path.into_inner() as i32;
    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let peer = match resolve_peer(&client, query.folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => return json_error("PEER_ERROR", &e, 400),
    };

    match client.get_messages_by_id(peer, &[message_id]).await {
        Ok(messages) => {
            if let Some(Some(msg)) = messages.first() {
                if let Some(media) = msg.media() {
                    let mime = match &media {
                        Media::Document(d) => d.mime_type().unwrap_or("application/octet-stream").to_string(),
                        _ => "application/octet-stream".to_string(),
                    };
                    let filename = match &media {
                        Media::Document(d) => d.name().unwrap_or_default().to_string(),
                        Media::Photo(_) => "Photo.jpg".to_string(),
                        _ => "download".to_string(),
                    };

                    return crate::server::build_media_response(
                        &client, &media, &req, &mime, Some(&filename),
                        crate::server::StreamingExtras {
                            extra_headers: vec![],
                            log_label: "API download",
                        },
                        Some((peer, message_id)),
                    );
                }
            }
            json_error("NOT_FOUND", "File not found", 404)
        }
        Err(e) => json_error("FETCH_ERROR", &format!("Failed to fetch file: {}", e), 500),
    }
}

#[derive(serde::Deserialize)]
struct BulkRequest {
    action: String,
    file_ids: Vec<serde_json::Value>,
    folder_id: Option<serde_json::Value>,
    payload: Option<BulkPayload>,
}

#[derive(serde::Deserialize)]
struct BulkPayload {
    folder_id: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct BulkResponse {
    success: bool,
    count: usize,
}

#[post("/api/v1/files/bulk")]
async fn api_bulk_files(
    req: HttpRequest,
    body: web::Json<BulkRequest>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
    cache_dirs: web::Data<CacheDirs>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let ids: Vec<i32> = body.file_ids.iter().filter_map(|val| {
        if let Some(i) = val.as_i64() {
            Some(i as i32)
        } else if let Some(s) = val.as_str() {
            s.parse::<i32>().ok()
        } else {
            None
        }
    }).collect();

    let source_folder: Option<i64> = body.folder_id.as_ref().and_then(|val| {
        if let Some(i) = val.as_i64() {
            Some(i)
        } else if let Some(s) = val.as_str() {
            s.parse::<i64>().ok()
        } else {
            None
        }
    });

    let target_folder: Option<i64> = body.payload.as_ref().and_then(|p| p.folder_id.as_ref()).and_then(|val| {
        if let Some(i) = val.as_i64() {
            Some(i)
        } else if let Some(s) = val.as_str() {
            s.parse::<i64>().ok()
        } else {
            None
        }
    });

    match body.action.as_str() {
        "delete" => {
            let peer = match resolve_peer(&client, source_folder, &tg_state.peer_cache).await {
                Ok(p) => p,
                Err(e) => return json_error("PEER_ERROR", &e, 400),
            };

            let messages = match client.get_messages_by_id(peer, &ids).await {
                Ok(messages) => messages,
                Err(e) => return json_error("FETCH_ERROR", &format!("Failed to inspect files before delete: {}", e), 500),
            };

            let mut delete_ids = Vec::new();
            for (message_id, msg) in ids.iter().zip(messages.into_iter()) {
                let msg = match msg {
                    Some(msg) => msg,
                    None => return json_error("MESSAGE_NOT_FOUND", &format!("Message {} not found", message_id), 404),
                };
                delete_ids.push(*message_id);
                if let Some(media) = msg.media() {
                    if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
                        if let Err(e) = validate_split_parts_present(&client, peer, &manifest, "API bulk split delete").await {
                            return json_error("SPLIT_INVALID", &e, 409);
                        }
                        delete_ids.extend(manifest.parts.iter().map(|part| part.message_id));
                    }
                }
            }

            if let Err(e) = delete_message_ids(&client, peer, &delete_ids, "API bulk delete").await {
                return json_error("DELETE_FAILED", &e, 500);
            }

            // Clean up stale preview cache entries for deleted messages.
            let source_folder_key = source_folder
                .map(|id| id.to_string())
                .unwrap_or_else(|| "home".to_string());
            spawn_cache_cleanup(
                cache_dirs.preview_dir.clone(),
                ids.clone(),
                source_folder_key,
            );
        }
        "move" => {
            let source_peer = match resolve_peer(&client, source_folder, &tg_state.peer_cache).await {
                Ok(p) => p,
                Err(e) => return json_error("PEER_ERROR", &e, 400),
            };
            let target_peer = match resolve_peer(&client, target_folder, &tg_state.peer_cache).await {
                Ok(p) => p,
                Err(e) => return json_error("PEER_ERROR", &e, 400),
            };
            if source_folder != target_folder {
                let messages = match client.get_messages_by_id(source_peer, &ids).await {
                    Ok(messages) => messages,
                    Err(e) => return json_error("FETCH_ERROR", &format!("Failed to inspect files before move: {}", e), 500),
                };
                for (message_id, msg) in ids.iter().zip(messages.into_iter()) {
                    let msg = match msg {
                        Some(msg) => msg,
                        None => return json_error("MESSAGE_NOT_FOUND", &format!("Message {} not found", message_id), 404),
                    };
                    if let Some(media) = msg.media() {
                        if split_manifest_from_media(&client, &media, msg.text()).await.is_some() {
                            return json_error("SPLIT_MOVE_UNSUPPORTED", "Split files cannot be moved through the REST API yet. Use the desktop file move.", 409);
                        }
                    }
                }

                if let Err(e) = client.forward_messages(target_peer, &ids, source_peer).await {
                    return json_error("MOVE_FORWARD_FAILED", &format!("Forward failed: {}", e), 500);
                }
                if let Err(e) = delete_message_ids(&client, source_peer, &ids, "API bulk move source cleanup").await {
                    return json_error("MOVE_DELETE_FAILED", &format!("Delete original failed: {}", e), 500);
                }

                // Clean up stale preview cache entries for the old message IDs.
                // After a move (forward+delete), messages get new IDs in the target folder,
                // so any cached previews under the old IDs are orphaned.
                let source_folder_key = source_folder
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "home".to_string());
                spawn_cache_cleanup(
                    cache_dirs.preview_dir.clone(),
                    ids.clone(),
                    source_folder_key,
                );
            }
        }
        "archive" => {
            let peer = match resolve_peer(&client, source_folder, &tg_state.peer_cache).await {
                Ok(p) => p,
                Err(e) => return json_error("PEER_ERROR", &e, 400),
            };

            // Download all files in async context, then delegate zip I/O
            // to spawn_blocking so we never block an Actix worker thread.
            let mut entries: Vec<(String, Vec<u8>)> = Vec::new();                        let mut total_bytes: u64 = 0;
                        let max_bytes = ARCHIVE_MAX_BYTES;

                        for mid in &ids {
                let messages = match client.get_messages_by_id(peer, &[*mid]).await {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if let Some(m) = messages.into_iter().flatten().next() {
                    if let Some(media) = m.media() {
                        let filename = match &media {
                            Media::Document(d) => d.name().unwrap_or_default().to_string(),
                            Media::Photo(_) => format!("photo_{}.jpg", mid),
                            _ => format!("file_{}.bin", mid),
                        };

                        let mut data = Vec::new();
                        let mut download_iter = client.iter_download(&media);
                        while let Some(chunk) = download_iter.next().await.ok().flatten() {
                            total_bytes += chunk.len() as u64;
                            if max_bytes > 0 && total_bytes > max_bytes {
                                return json_error(
                                    "ARCHIVE_TOO_LARGE",
                                    &format!(
                                        "Archive exceeds the {} MiB limit",
                                        max_bytes / (1024 * 1024)
                                    ),
                                    413,
                                );
                            }
                            data.extend_from_slice(&chunk);
                        }
                        entries.push((filename, data));
                    }
                }
            }

            let temp_zip_path = std::env::temp_dir().join(format!("archive_{}_{}.zip", rand::random::<u32>(), rand::random::<u32>()));
            let zip_path_for_task = temp_zip_path.clone();

            // All zip I/O runs on a blocking thread — never touches Actix workers.
            let archive_result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                let write_zip = || -> Result<(), String> {
                    let zip_file = std::fs::File::create(&zip_path_for_task)
                        .map_err(|e| format!("ZIP_CREATE_FAILED: {}", e))?;
                    let mut zip = zip::ZipWriter::new(zip_file);
                    let options = zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated);

                    for (filename, data) in &entries {
                        zip.start_file(filename, options)
                            .map_err(|e| format!("ZIP_ADD_FAILED: {}", e))?;
                        zip.write_all(data)
                            .map_err(|e| format!("ZIP_WRITE_FAILED: {}", e))?;
                    }

                    zip.finish()
                        .map_err(|e| format!("ZIP_FINISH_FAILED: {}", e))?;
                    Ok(())
                };

                match write_zip() {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let _ = std::fs::remove_file(&zip_path_for_task);
                        Err(e)
                    }
                }
            }).await;

            match archive_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return json_error("ARCHIVE_FAILED", &e, 500),
                Err(e) => return json_error("ARCHIVE_PANIC", &e.to_string(), 500),
            }

            let file = match tokio::fs::File::open(&temp_zip_path).await {
                Ok(f) => f,
                Err(e) => return json_error("OPEN_ZIP_FAILED", &e.to_string(), 500),
            };

            let stream = CleanupStream {
                file,
                path: temp_zip_path,
            };

            return HttpResponse::Ok()
                .content_type("application/zip")
                .insert_header((
                    actix_web::http::header::CONTENT_DISPOSITION,
                    "attachment; filename=\"archive.zip\"",
                ))
                .streaming(stream);
        }
        _ => return json_error("INVALID_ACTION", "Unsupported bulk action", 400),
    }

    HttpResponse::Ok().json(BulkResponse {
        success: true,
        count: ids.len(),
    })
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: Option<String>,
    #[allow(dead_code)]
    folder_id: Option<String>,
    #[allow(dead_code)]
    recursive: Option<bool>,
}

#[get("/api/v1/files/search")]
async fn api_search_files(
    req: HttpRequest,
    query: web::Query<SearchQuery>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let search_q = match query.q.as_deref() {
        Some(q) if !q.trim().is_empty() => q,
        _ => return json_error("INVALID_QUERY", "Search query parameter 'q' is required and cannot be empty", 400),
    };

    let query_string = req.query_string();
    let has_folder_id = query_string.split('&').any(|p| p.starts_with("folder_id=") || p == "folder_id");

    let mut peers_to_scan = Vec::new();
    if !has_folder_id {
        if let Ok(me_peer) = resolve_peer(&client, None, &tg_state.peer_cache).await {
            peers_to_scan.push((None, me_peer));
        }
        let mut dialogs = client.iter_dialogs();
        while let Some(dialog) = dialogs.next().await.ok().flatten() {
            if let Peer::Channel(ref c) = dialog.peer {
                let name = c.raw.title.clone();
                if name.to_lowercase().contains("[td]") {
                    if let Ok(Some(pr)) = dialog.peer.to_ref().await { peers_to_scan.push((Some(c.raw.id), pr)); }
                }
            }
        }
    } else {
        let mut parsed_id: Option<i64> = None;
        for pair in query_string.split('&') {
            let mut parts = pair.split('=');
            if let Some(key) = parts.next() {
                if key == "folder_id" {
                    if let Some(val) = parts.next() {
                        if !val.is_empty() && val != "null" && val != "none" && val != "None" {
                            if let Ok(id) = val.parse::<i64>() {
                                parsed_id = Some(id);
                            }
                        }
                    }
                }
            }
        }
        
        let resolved = match resolve_peer(&client, parsed_id, &tg_state.peer_cache).await {
            Ok(p) => p,
            Err(e) => return json_error("PEER_ERROR", &e, 400),
        };
        peers_to_scan.push((parsed_id, resolved));
    }

    let mut matching_files = Vec::new();
    for (fid, peer) in &peers_to_scan {
        let mut msgs = client.iter_messages(*peer).limit(200);
        while let Some(msg) = msgs.next().await.ok().flatten() {
            if let Some(doc) = msg.media() {
                let (name, size, mime) = match doc {
                    Media::Document(d) => {
                        let doc_name = d.name().unwrap_or_default().to_string();
                        let caption = msg.text();
                        let display_name = if caption.is_empty() { doc_name } else { caption.to_string() };
                        (display_name, d.size().unwrap_or(0), d.mime_type().map(|s| s.to_string()))
                    }
                    Media::Photo(_) => ("Photo.jpg".to_string(), 0, Some("image/jpeg".into())),
                    _ => ("Unknown".to_string(), 0, None),
                };
                
                if name.to_lowercase().contains(&search_q.to_lowercase()) {
                    matching_files.push(ApiFile {
                        id: msg.id() as i64,
                        folder_id: *fid,
                        name,
                        size: size as u64,
                        mime_type: mime,
                        created_at: msg.date().to_string(),
                    });
                }
            }
        }
    }

    HttpResponse::Ok().json(matching_files)
}



#[delete("/api/v1/files/{message_id}")]
async fn api_delete_file(
    req: HttpRequest,
    path: web::Path<i32>,
    query: web::Query<FolderQuery>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }
    let message_id = path.into_inner();
    let folder_id = query.folder_id;

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let peer = match resolve_peer(&client, folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => return json_error("PEER_ERROR", &e, 400),
    };

    let messages = match client.get_messages_by_id(peer, &[message_id]).await {
        Ok(messages) => messages,
        Err(e) => return json_error("DELETE_FAILED", &e.to_string(), 500),
    };
    let msg = match messages.into_iter().flatten().next() {
        Some(msg) => msg,
        None => return json_error("NOT_FOUND", "Message not found", 404),
    };

    let mut ids = vec![message_id];
    if let Some(media) = msg.media() {
        if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
            if let Err(e) = validate_split_parts_present(&client, peer, &manifest, "API split delete").await {
                return json_error("SPLIT_INVALID", &e, 409);
            }
            ids.extend(manifest.parts.iter().map(|part| part.message_id));
        }
    }

    match delete_message_ids(&client, peer, &ids, "API delete").await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => json_error("DELETE_FAILED", &e, 500),
    }
}

#[derive(serde::Deserialize)]
struct CopyRequest {
    folder_id: Option<i64>,
    source_folder_id: Option<i64>,
}

#[post("/api/v1/files/{message_id}/copy")]
async fn api_copy_file(
    req: HttpRequest,
    path: web::Path<i32>,
    body: web::Json<CopyRequest>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }
    let message_id = path.into_inner();
    let source_folder_id = body.source_folder_id;
    let target_folder_id = body.folder_id;

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let source_peer = match resolve_peer(&client, source_folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => return json_error("SOURCE_PEER_ERROR", &e, 400),
    };
    let target_peer = match resolve_peer(&client, target_folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => return json_error("TARGET_PEER_ERROR", &e, 400),
    };

    if let Ok(messages) = client.get_messages_by_id(source_peer, &[message_id]).await {
        if let Some(Some(msg)) = messages.first() {
            if let Some(media) = msg.media() {
                if split_manifest_from_media(&client, &media, msg.text()).await.is_some() {
                    return json_error("SPLIT_COPY_UNSUPPORTED", "Split files cannot be copied through the REST API yet.", 409);
                }
            }
        }
    }

    match client.forward_messages(target_peer, &[message_id], source_peer).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => json_error("COPY_FAILED", &e.to_string(), 500),
    }
}

#[derive(serde::Deserialize)]
struct UpdateFileRequest {
    name: Option<String>,
    folder_id: Option<i64>,
    source_folder_id: Option<i64>,
}

#[patch("/api/v1/files/{message_id}")]
async fn api_update_file(
    req: HttpRequest,
    path: web::Path<i32>,
    body: web::Json<UpdateFileRequest>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
    cache_dirs: web::Data<CacheDirs>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }
    let message_id = path.into_inner();

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    // Rename first — edits the original message's caption so the
    // updated name is carried over if a move (forward) follows.
    if let Some(ref new_name) = body.name {
        let rename_peer = match resolve_peer(&client, body.source_folder_id, &tg_state.peer_cache).await {
            Ok(p) => p,
            Err(e) => return json_error("PEER_ERROR", &e, 400),
        };

        // Verify the message exists before attempting to edit it.
        // This avoids a cryptic MESSAGE_ID_INVALID RPC error when the message
        // was moved or deleted since the file list was loaded.
        let messages = match client.get_messages_by_id(rename_peer, &[message_id]).await {
            Ok(msgs) => msgs,
            Err(e) => return json_error("FETCH_ERROR", &format!("Failed to fetch message for rename: {}", e), 500),
        };
        if messages.iter().flatten().next().is_none() {
            return json_error(
                "MESSAGE_NOT_FOUND",
                &format!(
                    "Message {} not found in folder {:?}. The file may have been moved or deleted. Please refresh.",
                    message_id, body.source_folder_id
                ),
                404,
            );
        }

        let input_peer = match peer_to_input_peer(rename_peer) {
            Ok(ip) => ip,
            Err(e) => return json_error("PEER_CONVERT_ERROR", &e, 400),
        };

        if let Err(e) = client.invoke(&tl::functions::messages::EditMessage {
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
        }).await {
            return json_error("RENAME_FAILED", &e.to_string(), 500);
        }
    }

    if let Some(target_folder_id) = body.folder_id {
        let source_folder_id = body.source_folder_id;
        if source_folder_id != body.folder_id {
            let source_peer = match resolve_peer(&client, source_folder_id, &tg_state.peer_cache).await {
                Ok(p) => p,
                Err(e) => return json_error("SOURCE_PEER_ERROR", &e, 400),
            };
            let target_peer = match resolve_peer(&client, Some(target_folder_id), &tg_state.peer_cache).await {
                Ok(p) => p,
                Err(e) => return json_error("TARGET_PEER_ERROR", &e, 400),
            };

            if let Ok(messages) = client.get_messages_by_id(source_peer, &[message_id]).await {
                if let Some(Some(msg)) = messages.first() {
                    if let Some(media) = msg.media() {
                        if split_manifest_from_media(&client, &media, msg.text()).await.is_some() {
                            return json_error("SPLIT_MOVE_UNSUPPORTED", "Split files cannot be moved through the REST API yet. Use the desktop file move.", 409);
                        }
                    }
                }
            }

            if let Err(e) = client.forward_messages(target_peer, &[message_id], source_peer).await {
                return json_error("MOVE_FORWARD_FAILED", &e.to_string(), 500);
            }
            if let Err(e) = client.delete_messages(source_peer, &[message_id]).await {
                return json_error("MOVE_DELETE_FAILED", &e.to_string(), 500);
            }

            // Clean up stale preview cache entries for the old message ID
            let source_folder_key = source_folder_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "home".to_string());
            spawn_cache_cleanup(
                cache_dirs.preview_dir.clone(),
                vec![message_id],
                source_folder_key,
            );
        }
    }

    HttpResponse::Ok().json(serde_json::json!({ "success": true }))
}

#[post("/api/v1/files")]
async fn api_upload_file(
    req: HttpRequest,
    mut payload: Multipart,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
    bw_manager: web::Data<Arc<BandwidthManager>>,
    app_handle: web::Data<tauri::AppHandle>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let temp_path = std::env::temp_dir().join(format!("upload_{}_{}", rand::random::<u32>(), rand::random::<u32>()));
    let mut file = match tokio::fs::File::create(&temp_path).await {
        Ok(f) => f,
        Err(e) => return json_error("TEMP_FILE_CREATE_FAILED", &e.to_string(), 500),
    };

    let mut folder_id: Option<i64> = None;
    let mut filename = "file".to_string();
    let mut field_mime: Option<String> = None;

    while let Ok(Some(mut field)) = payload.try_next().await {
        let content_disposition = field.content_disposition();
        let name = content_disposition.and_then(|cd| cd.get_name()).unwrap_or("");

        if name == "file" {
            if let Some(fname) = content_disposition.and_then(|cd| cd.get_filename()) {
                filename = fname.to_string();
            }
            field_mime = field.content_type().map(|m| m.to_string());
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return json_error("READ_ERROR", &e.to_string(), 400);
                    }
                };
                if let Err(e) = file.write_all(&data).await {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return json_error("WRITE_ERROR", &e.to_string(), 500);
                }
            }
        } else if name == "folder_id" {
            let mut bytes = Vec::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return json_error("READ_ERROR", &e.to_string(), 400);
                    }
                };
                bytes.extend_from_slice(&data);
            }
            let val_str = String::from_utf8_lossy(&bytes).trim().to_string();
            if !val_str.is_empty() && val_str != "null" && val_str != "none" {
                if let Ok(id) = val_str.parse::<i64>() {
                    folder_id = Some(id);
                }
            }
        }
    }

    if let Err(e) = file.flush().await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return json_error("WRITE_ERROR", &e.to_string(), 500);
    }
    drop(file);

    let file_size = match tokio::fs::metadata(&temp_path).await {
        Ok(m) => m.len(),
        Err(e) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return json_error("METADATA_ERROR", &e.to_string(), 500);
        }
    };

    if let Err(e) = bw_manager.try_reserve_up(file_size) {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return json_error("BANDWIDTH_LIMIT", &e, 400);
    }

    let peer = match resolve_peer(&client, folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => {
            bw_manager.release_up(file_size);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return json_error("PEER_ERROR", &e, 400);
        }
    };

    let temp_path_str = temp_path.to_string_lossy().to_string();

    // Shared upload + send path (retry / FLOOD_WAIT / backoff) — the same
    // helper as the local upload queue, so both ingestion paths stay consistent.
    let message_id = match upload_path_and_send(
        &client,
        peer.clone(),
        &temp_path_str,
        filename.clone(),
        filename.clone(),
        "",
        tg_state.get_ref().as_ref(),
        app_handle.get_ref(),
        0,
        file_size,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            bw_manager.release_up(file_size);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return json_error("UPLOAD_FAILED", &e, 500);
        }
    };

    let _ = tokio::fs::remove_file(&temp_path).await;

    // upload_path_and_send returns only the message id; fetch the sent message
    // for its timestamp to build the API response.
    let msg = match client.get_messages_by_id(peer, &[message_id]).await {
        Ok(msgs) => match msgs.into_iter().flatten().next() {
            Some(m) => m,
            None => {
                bw_manager.release_up(file_size);
                return json_error("FETCH_ERROR", "Uploaded message not found after send", 500);
            }
        },
        Err(e) => {
            bw_manager.release_up(file_size);
            return json_error("FETCH_ERROR", &e.to_string(), 500);
        }
    };

    let response_file = ApiFile {
        id: msg.id() as i64,
        folder_id,
        name: filename,
        size: file_size,
        mime_type: field_mime,
        created_at: msg.date().to_string(),
    };

    HttpResponse::Ok().json(response_file)
}

#[get("/api/v1/folders")]
async fn api_list_folders(
    req: HttpRequest,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let mut folders = Vec::new();
    let mut dialogs = client.iter_dialogs();
    let mut discovered = HashMap::new();

    while let Some(dialog) = dialogs.next().await.ok().flatten() {
        if let Peer::Channel(ref c) = dialog.peer {
            let id = c.raw.id;
            if let Ok(Some(pr)) = dialog.peer.to_ref().await { discovered.insert(id, pr); }
            let name = c.raw.title.clone();
            if name.to_lowercase().contains("[td]") {
                let display_name = name.replace(" [TD]", "").replace(" [td]", "").replace("[TD]", "").replace("[td]", "").trim().to_string();
                let username = c.raw.username.clone();
                let is_public = username.is_some();
                folders.push(FolderMetadata {
                    id,
                    name: display_name,
                    parent_id: None,
                    username,
                    is_public,
                    group_id: None,
                    display_order: 0,
                });
            }
        }
    }

    {
        let mut cache = tg_state.peer_cache.write().await;
        cache.extend(discovered);
    }

    HttpResponse::Ok().json(folders)
}

#[derive(serde::Deserialize)]
struct CreateFolderRequest {
    name: String,
}

#[post("/api/v1/folders")]
async fn api_create_folder(
    req: HttpRequest,
    body: web::Json<CreateFolderRequest>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    match create_folder_inner(&body.name, &client, &tg_state.peer_cache).await {
        Ok(folder) => HttpResponse::Ok().json(folder),
        Err(e) => json_error("CREATE_FOLDER_FAILED", &e, 500),
    }
}

#[derive(serde::Deserialize)]
struct RenameFolderRequest {
    name: String,
}

#[patch("/api/v1/folders/{folder_id}")]
async fn api_rename_folder(
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<RenameFolderRequest>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }
    let folder_id = path.into_inner();

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    match rename_folder_inner(folder_id, &body.name, &client, &tg_state.peer_cache).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => json_error("RENAME_FOLDER_FAILED", &e, 500),
    }
}

#[delete("/api/v1/folders/{folder_id}")]
async fn api_delete_folder(
    req: HttpRequest,
    path: web::Path<i64>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }
    let folder_id = path.into_inner();

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    match delete_folder_inner(folder_id, &client, &tg_state.peer_cache).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => json_error("DELETE_FOLDER_FAILED", &e, 500),
    }
}

#[derive(Serialize)]
struct FolderStat {
    id: Option<i64>,
    name: String,
    file_count: usize,
    size_bytes: u64,
}

#[derive(Serialize)]
struct MimeStat {
    mime_type: String,
    file_count: usize,
    size_bytes: u64,
}

#[derive(Serialize)]
struct StorageStatsResponse {
    total_storage_used_bytes: u64,
    total_file_count: usize,
    folders: Vec<FolderStat>,
    mime_types: Vec<MimeStat>,
}

#[get("/api/v1/storage/stats")]
async fn api_storage_stats(
    req: HttpRequest,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let mut peers_to_scan = Vec::new();
    if let Ok(me_peer) = resolve_peer(&client, None, &tg_state.peer_cache).await {
        peers_to_scan.push((None, "Saved Messages".to_string(), me_peer));
    }
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await.ok().flatten() {
        if let Peer::Channel(ref c) = dialog.peer {
            let name = c.raw.title.clone();
            if name.to_lowercase().contains("[td]") {
                let display_name = name.replace(" [TD]", "").replace(" [td]", "").replace("[TD]", "").replace("[td]", "").trim().to_string();
                if let Ok(Some(pr)) = dialog.peer.to_ref().await { peers_to_scan.push((Some(c.raw.id), display_name, pr)); }
            }
        }
    }

    let mut total_storage_used_bytes: u64 = 0;
    let mut total_file_count: usize = 0;
    let mut folder_stats = Vec::new();
    let mut mime_map: HashMap<String, (usize, u64)> = HashMap::new();

    for (fid, folder_name, peer) in peers_to_scan {
        let mut file_count = 0;
        let mut size_bytes = 0;
        let mut msgs = client.iter_messages(peer).limit(200);
        while let Some(msg) = msgs.next().await.ok().flatten() {
            if let Some(doc) = msg.media() {
                let (size, mime) = match doc {
                    Media::Document(d) => (d.size().unwrap_or(0) as u64, d.mime_type().unwrap_or("application/octet-stream").to_string()),
                    Media::Photo(_) => (0, "image/jpeg".to_string()),
                    _ => continue,
                };
                file_count += 1;
                size_bytes += size;

                let mime_entry = mime_map.entry(mime).or_insert((0, 0));
                mime_entry.0 += 1;
                mime_entry.1 += size;
            }
        }

        total_storage_used_bytes += size_bytes;
        total_file_count += file_count;

        folder_stats.push(FolderStat {
            id: fid,
            name: folder_name,
            file_count,
            size_bytes,
        });
    }

    let mime_types = mime_map.into_iter().map(|(mime_type, (file_count, size_bytes))| MimeStat {
        mime_type,
        file_count,
        size_bytes,
    }).collect();

    HttpResponse::Ok().json(StorageStatsResponse {
        total_storage_used_bytes,
        total_file_count,
        folders: folder_stats,
        mime_types,
    })
}

#[derive(Serialize)]
struct DuplicateGroup {
    name: String,
    size: u64,
    files: Vec<ApiFile>,
}

#[get("/api/v1/storage/duplicates")]
async fn api_storage_duplicates(
    req: HttpRequest,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let mut peers_to_scan = Vec::new();
    if let Ok(me_peer) = resolve_peer(&client, None, &tg_state.peer_cache).await {
        peers_to_scan.push((None, me_peer));
    }
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await.ok().flatten() {
        if let Peer::Channel(ref c) = dialog.peer {
            let name = c.raw.title.clone();
            if name.to_lowercase().contains("[td]") {
                if let Ok(Some(pr)) = dialog.peer.to_ref().await { peers_to_scan.push((Some(c.raw.id), pr)); }
            }
        }
    }

    let mut file_groups: HashMap<(String, u64), Vec<ApiFile>> = HashMap::new();

    for (fid, peer) in peers_to_scan {
        let mut msgs = client.iter_messages(peer).limit(200);
        while let Some(msg) = msgs.next().await.ok().flatten() {
            if let Some(doc) = msg.media() {
                let (name, size, mime) = match doc {
                    Media::Document(d) => (d.name().unwrap_or_default().to_string(), d.size().unwrap_or(0) as u64, d.mime_type().map(|s| s.to_string())),
                    Media::Photo(_) => ("Photo.jpg".to_string(), 0, Some("image/jpeg".into())),
                    _ => continue,
                };

                file_groups.entry((name.clone(), size)).or_default().push(ApiFile {
                    id: msg.id() as i64,
                    folder_id: fid,
                    name,
                    size,
                    mime_type: mime,
                    created_at: msg.date().to_string(),
                });
            }
        }
    }

    let duplicates: Vec<DuplicateGroup> = file_groups
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|((name, size), files)| DuplicateGroup { name, size, files })
        .collect();

    HttpResponse::Ok().json(duplicates)
}

#[get("/api/v1/folders/empty")]
async fn api_empty_folders(
    req: HttpRequest,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let mut folders_to_check = Vec::new();
    let mut dialogs = client.iter_dialogs();
    while let Some(dialog) = dialogs.next().await.ok().flatten() {
        if let Peer::Channel(ref c) = dialog.peer {
            let name = c.raw.title.clone();
            if name.to_lowercase().contains("[td]") {
                let display_name = name.replace(" [TD]", "").replace(" [td]", "").replace("[TD]", "").replace("[td]", "").trim().to_string();
                if let Ok(Some(pr)) = dialog.peer.to_ref().await { folders_to_check.push((c.raw.id, display_name, pr)); }
            }
        }
    }

    let mut empty_folders = Vec::new();

    for (fid, display_name, peer) in folders_to_check {
        let mut msgs = client.iter_messages(peer).limit(1);
        let mut is_empty = true;
        if let Some(msg) = msgs.next().await.ok().flatten() {
            if msg.media().is_some() {
                is_empty = false;
            }
        }
        if is_empty {
            empty_folders.push(FolderMetadata {
                id: fid,
                name: display_name,
                parent_id: None,
                username: None,
                is_public: false,
                group_id: None,
                display_order: 0,
            });
        }
    }

    HttpResponse::Ok().json(empty_folders)
}

#[derive(Serialize)]
struct MediaInfoResponse {
    duration_secs: Option<f64>,
    width: Option<i32>,
    height: Option<i32>,
    audio_title: Option<String>,
    audio_performer: Option<String>,
}

#[get("/api/v1/files/{message_id}/media-info")]
async fn api_media_info(
    req: HttpRequest,
    path: web::Path<i32>,
    query: web::Query<FolderQuery>,
    tg_state: web::Data<Arc<TelegramState>>,
    api_state: web::Data<ApiState>,
) -> impl Responder {
    if let Err(e) = check_auth(&req, &api_state) {
        return e;
    }
    let message_id = path.into_inner();
    let folder_id = query.folder_id;

    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return json_error("NOT_CONNECTED", "Telegram client is not connected", 503),
    };

    let peer = match resolve_peer(&client, folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => return json_error("PEER_ERROR", &e, 400),
    };

    let messages = match client.get_messages_by_id(peer, &[message_id]).await {
        Ok(msgs) => msgs,
        Err(e) => return json_error("GET_MESSAGE_ERROR", &e.to_string(), 500),
    };

    let msg = match messages.into_iter().flatten().next() {
        Some(m) => m,
        None => return json_error("NOT_FOUND", "File message not found", 404),
    };

    let media = match msg.media() {
        Some(m) => m,
        None => return json_error("NO_MEDIA", "Message has no media", 400),
    };

    let mut info = MediaInfoResponse {
        duration_secs: None,
        width: None,
        height: None,
        audio_title: None,
        audio_performer: None,
    };

    if let Media::Document(d) = media {
        if let Some(tl::enums::Document::Document(doc)) = &d.raw.document {
            for attr in &doc.attributes {
                match attr {
                    tl::enums::DocumentAttribute::Video(v) => {
                        info.duration_secs = Some(v.duration);
                        info.width = Some(v.w);
                        info.height = Some(v.h);
                    }
                    tl::enums::DocumentAttribute::Audio(a) => {
                        info.duration_secs = Some(a.duration as f64);
                        info.audio_title = a.title.clone();
                        info.audio_performer = a.performer.clone();
                    }
                    _ => {}
                }
            }
        }
    }

    HttpResponse::Ok().json(info)
}

/// Register all API routes on the Actix App
pub fn configure_api(cfg: &mut web::ServiceConfig) {
    cfg.service(api_health)
       .service(api_list_files)
       .service(api_get_file)
       .service(api_download_file)
       .service(api_bulk_files)
       .service(api_search_files)
       .service(api_delete_file)
       .service(api_copy_file)
       .service(api_update_file)
       .service(api_upload_file)
       .service(api_list_folders)
       .service(api_create_folder)
       .service(api_rename_folder)
       .service(api_delete_folder)
       .service(api_storage_stats)
       .service(api_storage_duplicates)
       .service(api_empty_folders)
       .service(api_media_info);
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    /// Hash the same way `api_settings::hash_key` does, so tests can build a
    /// stored hash without exposing that private helper.
    fn sha256_hex(input: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn api_state_with_key(key: Option<&str>) -> web::Data<ApiState> {
        web::Data::new(ApiState {
            key_hash: key.map(sha256_hex),
        })
    }

    #[test]
    fn auth_rejects_when_no_key_is_configured() {
        // An unconfigured server must not be usable by anyone.
        let state = api_state_with_key(None);
        let req = TestRequest::default().to_http_request();

        let response = check_auth(&req, &state).expect_err("no key configured must reject");
        assert_eq!(response.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn auth_rejects_a_missing_header() {
        let state = api_state_with_key(Some("secret-key"));
        let req = TestRequest::default().to_http_request();

        let response = check_auth(&req, &state).expect_err("missing header must reject");
        assert_eq!(response.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn auth_rejects_a_wrong_key() {
        let state = api_state_with_key(Some("secret-key"));
        let req = TestRequest::default()
            .insert_header(("X-API-Key", "wrong-key"))
            .to_http_request();

        let response = check_auth(&req, &state).expect_err("wrong key must reject");
        assert_eq!(response.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn auth_accepts_the_matching_key() {
        let state = api_state_with_key(Some("secret-key"));
        let req = TestRequest::default()
            .insert_header(("X-API-Key", "secret-key"))
            .to_http_request();

        assert!(check_auth(&req, &state).is_ok());
    }

    #[test]
    fn auth_rejects_an_empty_key_even_when_configured() {
        // A blank header must not be treated as "no key required".
        let state = api_state_with_key(Some("secret-key"));
        let req = TestRequest::default()
            .insert_header(("X-API-Key", ""))
            .to_http_request();

        assert!(check_auth(&req, &state).is_err());
    }

    #[test]
    fn verify_key_matches_only_the_exact_plaintext() {
        let hash = sha256_hex("correct-horse");
        assert!(crate::commands::api_settings::verify_key("correct-horse", &hash));
        assert!(!crate::commands::api_settings::verify_key("correct-hors", &hash));
        assert!(!crate::commands::api_settings::verify_key("Correct-Horse", &hash));
        assert!(!crate::commands::api_settings::verify_key("", &hash));
    }
}
