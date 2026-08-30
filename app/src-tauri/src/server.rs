use actix_web::{get, web, App, HttpServer, HttpResponse, Responder};
use actix_cors::Cors;
use crate::commands::TelegramState;
use crate::commands::fs::split_manifest_from_media;
use crate::commands::streaming::stream_token_header_name;
use crate::commands::utils::resolve_peer;
use grammers_client::media::Media;
use grammers_client::peer::Peer;
use crate::models::SplitManifest;
use crate::transfer_log::record_transfer_log;

use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Holds the per-session streaming token for Actix validation
pub struct StreamTokenData {
    pub token: String,
}

#[derive(serde::Deserialize)]
struct StreamQuery {
    token: Option<String>,
}

const SPLIT_STREAM_PART_CACHE_LIMIT: usize = 8;
const SPLIT_STREAM_PART_CACHE_TTL: Duration = Duration::from_secs(600);

#[derive(Clone)]
struct CachedSplitPart {
    media: Media,
    size: Option<u64>,
    stored_at: Instant,
}

static SPLIT_STREAM_PART_CACHE: OnceLock<Mutex<HashMap<String, CachedSplitPart>>> = OnceLock::new();

pub fn parse_range_header(header_val: &str, total_size: u64) -> Option<(u64, u64)> {
    if !header_val.starts_with("bytes=") {
        return None;
    }
    let s = &header_val["bytes=".len()..];
    let parts: Vec<&str> = s.split('-').collect();
    if parts.is_empty() {
        return None;
    }
    let start = parts[0].trim().parse::<u64>().ok()?;
    let end = if parts.len() > 1 && !parts[1].trim().is_empty() {
        let parsed_end = parts[1].trim().parse::<u64>().ok()?;
        std::cmp::min(parsed_end, total_size - 1)
    } else {
        total_size - 1
    };
    if start <= end {
        Some((start, end))
    } else {
        None
    }
}

/// Extra headers to inject into streaming responses (e.g. Cache-Control, Content-Disposition).
pub struct StreamingExtras {
    pub extra_headers: Vec<(&'static str, String)>,
    pub log_label: &'static str,
}

/// Build a streaming HTTP response for a Telegram media file with optional byte-range support.
/// This is the single shared implementation used by the streaming server, REST API, and share routes.
pub fn build_media_response(
    client: &grammers_client::Client,
    media: &Media,
    req: &actix_web::HttpRequest,
    mime: &str,
    filename: Option<&str>,
    extras: StreamingExtras,
) -> HttpResponse {
    let size = match media {
        Media::Document(d) => d.size().unwrap_or(0) as u64,
        Media::Photo(_) => 0,
        _ => 0,
    };

    // Parse Range header
    let mut start_byte = 0u64;
    let mut end_byte = if size > 0 { size - 1 } else { 0 };
    let mut is_range = false;

    if size > 0 {
        if let Some(range_header) = req.headers().get(actix_web::http::header::RANGE) {
            if let Ok(range_str) = range_header.to_str() {
                if let Some((start, end)) = parse_range_header(range_str, size) {
                    start_byte = start;
                    end_byte = end;
                    is_range = true;
                }
            }
        }
    }

    let content_length = if is_range {
        end_byte - start_byte + 1
    } else {
        size
    };

    // Chunk alignment for Telegram's upload.getFile offset requirement.
    //
    // CRITICAL: Without the `precise` flag (which grammers-client does not
    // expose), Telegram may route the request through a CDN that rounds the
    // offset down to a CDN chunk boundary (commonly 512 KB = 524288 bytes).
    // If our requested offset is not aligned to this boundary, the CDN
    // silently returns data starting from the rounded-down position.
    //
    // Example: requesting offset 111935488 (213.48 × 512 KB) gets rounded
    // to 111673344 (213 × 512 KB), introducing a 262 KB shift. This
    // misalignment accumulates across successive Range requests and
    // eventually corrupts the MP4 box parsing (triggering the "ORrI" error).
    //
    // Fix: always align to 512 KB boundaries, then slice off the leading
    // bytes to serve the exact byte range the client requested.
    let mut download_iter = client.iter_download(media);
    let mut bytes_to_skip: usize = 0;

    if start_byte > 0 {
        /// MTProto chunk size (must be divisible by grammers' MIN_CHUNK_SIZE).
        /// 65536 is safe — it is the default and widely tested.
        const CHUNK_SIZE: i32 = 65536;
        /// Telegram CDN alignment boundary. 512 KB is the largest observed
        /// CDN chunk size; aligning to this boundary prevents ANY rounding.
        const CDN_ALIGNMENT: u64 = 524288; // 512 KB

        // 1) Round the requested start down to a CDN-safe boundary.
        let cdn_aligned_start = (start_byte / CDN_ALIGNMENT) * CDN_ALIGNMENT;

        // 2) Compute how many 64 KB chunks to skip to reach that boundary.
        let chunk_index = (cdn_aligned_start / CHUNK_SIZE as u64) as i32;

        // Always set chunk size for predictable download behaviour.
        download_iter = download_iter.chunk_size(CHUNK_SIZE);
        if chunk_index > 0 {
            download_iter = download_iter.skip_chunks(chunk_index);
        }

        // 3) Leading bytes between the CDN-aligned offset and the client's
        //    actual requested start must be discarded.
        bytes_to_skip = (start_byte - cdn_aligned_start) as usize;

        // Safety: cdn_aligned_start ≤ start_byte by construction.
        debug_assert!(
            cdn_aligned_start <= start_byte,
            "CDN alignment invariant violated: aligned {} > requested {}",
            cdn_aligned_start, start_byte
        );

        log::debug!(
            "Range alignment: requested={}, cdn_aligned={}, chunk_index={}, bytes_to_skip={}",
            start_byte, cdn_aligned_start, chunk_index, bytes_to_skip,
        );
    }

    let label = extras.log_label;
    let stream = async_stream::stream! {
        let mut skipped: usize = 0;
        let mut total_yielded: u64 = 0;

        while let Some(chunk) = download_iter.next().await.transpose() {
            match chunk {
                Ok(data) => {
                    let mut bytes = web::Bytes::from(data);

                    if skipped < bytes_to_skip {
                        let to_skip = bytes_to_skip - skipped;
                        if bytes.len() <= to_skip {
                            skipped += bytes.len();
                            continue;
                        } else {
                            bytes = bytes.slice(to_skip..);
                            skipped = bytes_to_skip;
                        }
                    }

                    if total_yielded + bytes.len() as u64 > content_length {
                        let allowed = (content_length - total_yielded) as usize;
                        if allowed > 0 {
                            let sub = bytes.slice(..allowed);
                            total_yielded += allowed as u64;
                            yield Ok::<_, actix_web::Error>(sub);
                        }
                        break;
                    } else {
                        let len = bytes.len() as u64;
                        total_yielded += len;
                        yield Ok::<_, actix_web::Error>(bytes);
                        if total_yielded >= content_length {
                            break;
                        }
                    }
                }
                Err(e) => {
                    log::error!("{} stream error: {}", label, e);
                    break;
                }
            }
        }
        log::debug!("{} stream completed (yielded: {})", label, total_yielded);
    };

    let mut resp = if is_range {
        let mut r = HttpResponse::PartialContent();
        r.insert_header(("Content-Range", format!("bytes {}-{}/{}", start_byte, end_byte, size)));
        r.insert_header(("Content-Length", content_length.to_string()));
        r
    } else {
        let mut r = HttpResponse::Ok();
        r.insert_header(("Content-Length", size.to_string()));
        r
    };

    resp.insert_header(("Content-Type", mime.to_owned()));
    resp.insert_header(("Accept-Ranges", "bytes"));

    if let Some(fname) = filename {
        resp.insert_header((
            "Content-Disposition",
            format!("inline; filename=\"{}\"", fname),
        ));
    }

    for (key, val) in &extras.extra_headers {
        resp.insert_header((*key, val.clone()));
    }

    resp.streaming(stream)
}

fn build_split_media_response(
    client: &grammers_client::Client,
    peer: grammers_session::types::PeerRef,
    manifest: SplitManifest,
    cache_scope: String,
    req: &actix_web::HttpRequest,
) -> HttpResponse {
    let size = manifest.size;
    let mut start_byte = 0u64;
    let mut end_byte = if size > 0 { size - 1 } else { 0 };
    let mut is_range = false;

    if size > 0 {
        if let Some(range_header) = req.headers().get(actix_web::http::header::RANGE) {
            if let Ok(range_str) = range_header.to_str() {
                if let Some((start, end)) = parse_range_header(range_str, size) {
                    start_byte = start;
                    end_byte = end;
                    is_range = true;
                }
            }
        }
    }

    let content_length = if is_range { end_byte - start_byte + 1 } else { size };
    let client = client.clone();
    let mime = manifest.mime_type.clone();
    let filename = manifest.filename.replace('"', "'");
    let log_filename = filename.clone();
    let parts = manifest.parts.clone();
    let stream = async_stream::stream! {
        const CHUNK_SIZE: i32 = 65536;
        const CDN_ALIGNMENT: u64 = 524288;

        let mut part_global_start = 0u64;
        for part in parts {
            let part_global_end = part_global_start + part.size.saturating_sub(1);
            if part_global_end < start_byte {
                part_global_start += part.size;
                continue;
            }
            if part_global_start > end_byte {
                break;
            }

            let wanted_start = start_byte.max(part_global_start);
            let wanted_end = end_byte.min(part_global_end);
            let part_offset = wanted_start - part_global_start;
            let mut remaining = wanted_end - wanted_start + 1;

            let cache_key = split_part_cache_key(&cache_scope, part.message_id);
            let (media, cached_size) = match get_cached_split_part(&cache_key) {
                Some(cached) => cached,
                None => {
                    let messages = match client.get_messages_by_id(peer, &[part.message_id]).await {
                        Ok(m) => m,
                        Err(e) => {
                            let err = format!("Split stream failed to fetch part {}: {}", part.message_id, e);
                            log::error!("{}", err);
                            record_transfer_log("Split stream", err, Some(format!("file: {}", log_filename)));
                            yield Err::<web::Bytes, actix_web::Error>(actix_web::error::ErrorInternalServerError("split part fetch failed"));
                            break;
                        }
                    };
                    let msg = match messages.into_iter().flatten().next() {
                        Some(m) => m,
                        None => {
                            let err = format!("Split stream missing part {}", part.message_id);
                            log::error!("{}", err);
                            record_transfer_log("Split stream", err, Some(format!("file: {}", log_filename)));
                            yield Err::<web::Bytes, actix_web::Error>(actix_web::error::ErrorInternalServerError("split part missing"));
                            break;
                        }
                    };
                    let media = match msg.media() {
                        Some(m) => m,
                        None => {
                            let err = format!("Split stream part {} has no media", part.message_id);
                            log::error!("{}", err);
                            record_transfer_log("Split stream", err, Some(format!("file: {}", log_filename)));
                            yield Err::<web::Bytes, actix_web::Error>(actix_web::error::ErrorInternalServerError("split part has no media"));
                            break;
                        }
                    };
                    let size = media_size(&media);
                    put_cached_split_part(&cache_key, media.clone(), size);
                    (media, size)
                }
            };
            if let Some(actual_size) = cached_size {
                if actual_size != part.size {
                    let err = format!(
                        "Split stream part {} size mismatch: expected {}, got {}",
                        part.message_id,
                        part.size,
                        actual_size
                    );
                    log::error!("{}", err);
                    record_transfer_log("Split stream", err, Some(format!("file: {}", log_filename)));
                    yield Err::<web::Bytes, actix_web::Error>(actix_web::error::ErrorInternalServerError("split part size mismatch"));
                    break;
                }
            }

            let aligned_start = (part_offset / CDN_ALIGNMENT) * CDN_ALIGNMENT;
            let chunk_index = (aligned_start / CHUNK_SIZE as u64) as i32;
            let mut bytes_to_skip = (part_offset - aligned_start) as usize;
            let mut iter = client.iter_download(&media).chunk_size(CHUNK_SIZE);
            if chunk_index > 0 {
                iter = iter.skip_chunks(chunk_index);
            }

            while remaining > 0 {
                let next = iter.next().await.transpose();
                let chunk = match next {
                    Some(Ok(c)) => c,
                    Some(Err(e)) => {
                        remove_cached_split_part(&cache_key);
                        let err = format!("Split stream part {} error: {}", part.message_id, e);
                        log::error!("{}", err);
                        record_transfer_log("Split stream", err, Some(format!("file: {}", log_filename)));
                        yield Err::<web::Bytes, actix_web::Error>(actix_web::error::ErrorInternalServerError("split stream part error"));
                        break;
                    }
                    None => break,
                };

                let mut data = chunk;
                if bytes_to_skip > 0 {
                    if data.len() <= bytes_to_skip {
                        bytes_to_skip -= data.len();
                        continue;
                    }
                    data = data[bytes_to_skip..].to_vec();
                    bytes_to_skip = 0;
                }

                if data.len() as u64 > remaining {
                    data.truncate(remaining as usize);
                }
                remaining -= data.len() as u64;
                yield Ok::<_, actix_web::Error>(web::Bytes::from(data));
            }

            part_global_start += part.size;
        }
    };

    let mut resp = if is_range {
        let mut r = HttpResponse::PartialContent();
        r.insert_header(("Content-Range", format!("bytes {}-{}/{}", start_byte, end_byte, size)));
        r.insert_header(("Content-Length", content_length.to_string()));
        r
    } else {
        let mut r = HttpResponse::Ok();
        r.insert_header(("Content-Length", size.to_string()));
        r
    };
    resp.insert_header(("Content-Type", mime));
    resp.insert_header(("Accept-Ranges", "bytes"));
    resp.insert_header(("Cache-Control", "private, max-age=120"));
    resp.insert_header(("Content-Disposition", format!("inline; filename=\"{}\"", filename)));
    resp.streaming(stream)
}

fn media_size(media: &Media) -> Option<u64> {
    match media {
        Media::Document(d) => Some(d.size().unwrap_or(0) as u64),
        _ => None,
    }
}

fn split_part_cache_key(scope: &str, message_id: i32) -> String {
    format!("{}:{}", scope, message_id)
}

fn get_cached_split_part(key: &str) -> Option<(Media, Option<u64>)> {
    let mut cache = split_stream_part_cache().lock().unwrap();
    if let Some(entry) = cache.get(key) {
        if entry.stored_at.elapsed() <= SPLIT_STREAM_PART_CACHE_TTL {
            return Some((entry.media.clone(), entry.size));
        }
    }
    cache.remove(key);
    None
}

fn put_cached_split_part(key: &str, media: Media, size: Option<u64>) {
    let mut cache = split_stream_part_cache().lock().unwrap();
    // ponytail: tiny arbitrary eviction; use a real LRU only if seek-heavy streaming proves it matters.
    if cache.len() >= SPLIT_STREAM_PART_CACHE_LIMIT {
        if let Some(old_key) = cache.keys().next().cloned() {
            cache.remove(&old_key);
        }
    }
    cache.insert(key.to_string(), CachedSplitPart { media, size, stored_at: Instant::now() });
}

fn remove_cached_split_part(key: &str) {
    split_stream_part_cache().lock().unwrap().remove(key);
}

fn split_stream_part_cache() -> &'static Mutex<HashMap<String, CachedSplitPart>> {
    SPLIT_STREAM_PART_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn split_part_cache_key_includes_scope_and_message_id() {
        assert_eq!(super::split_part_cache_key("folder-1", 42), "folder-1:42");
    }
}

#[derive(serde::Serialize, Clone)]
pub struct StreamPlaybackEvent {
    pub file_id: i32,
    pub file_name: String,
    pub folder_id: Option<i64>,
    pub file_size: u64,
}

static LAST_STREAM_EVENT: OnceLock<Mutex<Option<(i32, Option<i64>, Instant)>>> = OnceLock::new();

fn notify_stream_playback_started(app_handle: &tauri::AppHandle, file_id: i32, file_name: String, folder_id: Option<i64>, file_size: u64) {
    let mutex = LAST_STREAM_EVENT.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = mutex.lock() {
        if let Some((last_id, last_folder, last_time)) = *guard {
            if last_id == file_id && last_folder == folder_id && last_time.elapsed().as_secs() < 5 {
                return; // Debounce rapid Range requests for the exact same file
            }
        }
        *guard = Some((file_id, folder_id, Instant::now()));
    }
    use tauri::Emitter;
    let _ = app_handle.emit("stream-playback-started", StreamPlaybackEvent {
        file_id,
        file_name,
        folder_id,
        file_size,
    });
}

async fn handle_stream_media_request(
    req: actix_web::HttpRequest,
    folder_id_str: String,
    message_id: i32,
    query: web::Query<StreamQuery>,
    data: web::Data<Arc<TelegramState>>,
    token_data: web::Data<StreamTokenData>,
    app_handle: web::Data<tauri::AppHandle>,
) -> impl Responder {
    let header_token = req
        .headers()
        .get(stream_token_header_name())
        .and_then(|value| value.to_str().ok());
    let request_token = query.token.as_deref().or(header_token);

    // Validate session token
    match request_token {
        Some(t) if t == token_data.token.as_str() => {
            log::debug!("Stream request: Token validated successfully for msg {}", message_id);
        },
        _ => {
            log::error!("Stream request failed: Invalid or missing stream token for msg {}", message_id);
            return HttpResponse::Forbidden().body("Invalid or missing stream token")
        },
    }
    
    // Parse folder ID
    let folder_id = if folder_id_str == "me" || folder_id_str == "home" || folder_id_str == "null" {
        log::debug!("Stream request: Using root folder for msg {}", message_id);
        None
    } else {
        match folder_id_str.parse::<i64>() {
            Ok(id) => {
                log::debug!("Stream request: Parsed folder ID {} for msg {}", id, message_id);
                Some(id)
            },
            Err(_) => {
                log::error!("Stream request failed: Invalid folder ID format '{}' for msg {}", folder_id_str, message_id);
                return HttpResponse::BadRequest().body("Invalid folder ID")
            },
        }
    };

    let client_opt = {
        data.client.lock().await.clone()
    };

    if let Some(client) = client_opt {
        log::debug!("Stream request: Client acquired, resolving peer for msg {}...", message_id);
        match resolve_peer(&client, folder_id, &data.peer_cache).await {
            Ok(peer) => {
                log::debug!("Stream request: Peer resolved, fetching message {}...", message_id);
                // Try to fetch message efficiently
                 match client.get_messages_by_id(peer, &[message_id]).await {
                    Ok(messages) => {
                        if let Some(Some(msg)) = messages.first() {
                            if let Some(media) = msg.media() {
                                log::debug!("Stream request: Message and media found for msg {}", message_id);
                                if let Some(manifest) = split_manifest_from_media(&client, &media, msg.text()).await {
                                    notify_stream_playback_started(&app_handle, message_id, manifest.filename.clone(), folder_id, manifest.size);
                                    return build_split_media_response(&client, peer.clone(), manifest, folder_id_str.clone(), &req);
                                }
                                let mime = mime_type_from_media(&media);
                                let doc_filename = match &media {
                                    Media::Document(d) => {
                                        let n = d.name();
                                        if !n.is_empty() {
                                            Some(n.to_string())
                                        } else {
                                            None
                                        }
                                    },
                                    _ => None,
                                };
                                let file_size = media_size(&media).unwrap_or(0);
                                let resolved_name = doc_filename.clone().unwrap_or_else(|| format!("file_{}", message_id));
                                notify_stream_playback_started(&app_handle, message_id, resolved_name, folder_id, file_size);
                                return build_media_response(
                                    &client, &media, &req, &mime, doc_filename.as_deref(),
                                    StreamingExtras {
                                        extra_headers: vec![("Cache-Control", "private, max-age=120".to_string())],
                                        log_label: "Stream",
                                    },
                                );
                            } else {
                                log::error!("Stream request failed: Media not found in message {}", message_id);
                            }
                        } else {
                            log::error!("Stream request failed: Message {} not found", message_id);
                        }
                        HttpResponse::NotFound().body("Message or media not found")
                    },
                    Err(e) => {
                        log::error!("Stream request failed: Error fetching message {}: {}", message_id, e);
                        HttpResponse::InternalServerError().body(format!("Failed to fetch message: {}", e))
                    },
                 }
            },
            Err(e) => {
                log::error!("Stream request failed: Peer resolution error for msg {}: {}", message_id, e);
                HttpResponse::BadRequest().body(format!("Peer resolution failed: {}", e))
            },
        }
    } else {
        log::error!("Stream request failed: Telegram client not connected for msg {}", message_id);
        HttpResponse::ServiceUnavailable().body("Telegram client not connected")
    }
}

#[get("/stream/{folder_id}/{message_id}/{filename:.*}")]
async fn stream_media_named(
    req: actix_web::HttpRequest,
    path: web::Path<(String, i32, String)>,
    query: web::Query<StreamQuery>,
    data: web::Data<Arc<TelegramState>>,
    token_data: web::Data<StreamTokenData>,
    app_handle: web::Data<tauri::AppHandle>,
) -> impl Responder {
    let (folder_id_str, message_id, _filename) = path.into_inner();
    handle_stream_media_request(req, folder_id_str, message_id, query, data, token_data, app_handle).await
}

#[get("/stream/{folder_id}/{message_id}")]
async fn stream_media(
    req: actix_web::HttpRequest,
    path: web::Path<(String, i32)>,
    query: web::Query<StreamQuery>,
    data: web::Data<Arc<TelegramState>>,
    token_data: web::Data<StreamTokenData>,
    app_handle: web::Data<tauri::AppHandle>,
) -> impl Responder {
    let (folder_id_str, message_id) = path.into_inner();
    handle_stream_media_request(req, folder_id_str, message_id, query, data, token_data, app_handle).await
}

fn mime_type_from_media(media: &Media) -> String {
    match media {
        Media::Document(d) => d.mime_type().unwrap_or("application/octet-stream").to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

pub async fn start_server(
    state: Arc<TelegramState>,
    port: u16,
    token: String,
    db_pool: crate::db::DbConnection,
    app_handle: tauri::AppHandle,
) -> std::io::Result<actix_web::dev::Server> {
    let state_data = web::Data::new(state);
    let token_data = web::Data::new(StreamTokenData { token });
    let db_data = web::Data::new(db_pool);
    let app_handle_data = web::Data::new(app_handle);
    
    log::info!("Starting Streaming Server on port {}", port);

    // Bind the listener to 127.0.0.1 explicitly.
    // The streaming server is only accessed from the local frontend — binding
    // to 0.0.0.0 is unnecessary and can trigger firewall prompts on Windows.
    // 127.0.0.1 is the most universally reliable loopback address across all
    // platforms (Windows, macOS, Linux) and pairs correctly with the "localhost"
    // hostname used by the client (localhost → 127.0.0.1 is the standard mapping).
    let ipv4_addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&ipv4_addr) {
        Ok(l) => {
            log::info!("Streaming Server listening on {} (IPv4)", ipv4_addr);
            l
        }
        Err(e) => {
            log::warn!("IPv4 loopback bind failed ({}), falling back to IPv6 loopback", e);
            let ipv6_addr = format!("[::1]:{}", port);
            let l = TcpListener::bind(&ipv6_addr)?;
            log::info!("Streaming Server listening on {} (IPv6 loopback)", ipv6_addr);
            l
        }
    };

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin_fn(|origin, _req_head| {
                let origin_bytes = origin.as_bytes();
                origin_bytes.starts_with(b"tauri://")
                    || origin_bytes.starts_with(b"http://tauri.localhost")
                    || origin_bytes.starts_with(b"https://tauri.localhost")
                    || origin_bytes.starts_with(b"http://localhost")
                    || origin_bytes.starts_with(b"http://127.0.0.1")
                    || origin_bytes.starts_with(b"https://asset.localhost")
                    || origin_bytes.starts_with(b"http://asset.localhost")
                    || origin_bytes == b"null"
            })
            .allow_any_method()
            .allow_any_header();

        App::new()
            .wrap(cors)
            .app_data(state_data.clone())
            .app_data(token_data.clone())
            .app_data(db_data.clone())
            .app_data(app_handle_data.clone())
            .service(stream_media_named)
            .service(stream_media)
            .configure(crate::share_routes::configure_share_routes)
    })
    .listen(listener)?
    .run();

    log::info!("Streaming Server started successfully on port {}", port);

    Ok(server)
}
