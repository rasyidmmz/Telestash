//! Fallback video thumbnail generation for videos that Telegram itself did
//! not thumbnail (large documents). Extracts a single frame through the
//! already-bundled MPV sidecar (`--vo=image`) fed by the local streaming
//! server, so no new dependency and no full-file download — roughly 1–5 MB
//! of stream data per generation, cached as one small JPEG.
//!
//! Lightness guards: single global queue (one MPV at a time), hard timeout,
//! per-message single-flight, and an LRU byte cap on the generated cache.

use base64::{Engine as _, engine::general_purpose};
use tauri::{Manager, State};
use tokio::sync::Semaphore;

use crate::commands::streaming::StreamConfig;
use crate::TelegramState;

/// Max concurrent MPV frame-extraction processes.
static GEN_PERMIT: Semaphore = Semaphore::const_new(1);

/// Cache budget for the generated-thumbs dir (LRU by modified time).
const GEN_CACHE_MAX_BYTES: u64 = 100 * 1024 * 1024;
const GEN_CACHE_MAX_FILES: usize = 2_000;

/// Extract one frame from a video message through bundled MPV.
/// Returns a base64 JPEG data URL, or "" when generation is not possible
/// (non-video file, MPV missing, timeout) so the UI keeps its icon fallback.
#[tauri::command]
pub async fn cmd_generate_video_thumbnail(
    message_id: i32,
    folder_id: Option<i64>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    config: State<'_, StreamConfig>,
) -> Result<String, String> {
    let folder_key = folder_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "home".to_string());

    let cache_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?
        .join("generated_thumbs");
    let cache_path = cache_dir.join(format!("{}_{}.jpg", folder_key, message_id));
    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        return Ok(to_data_url(&bytes));
    }

    // Only videos are worth a frame extraction.
    let is_video = message_is_video(&state, folder_id, message_id).await.unwrap_or(false);
    if !is_video {
        return Ok(String::new());
    }

    let mpv_bin = crate::commands::streaming::resolve_mpv_binary(&app_handle)
        .ok_or_else(|| "MPV binary not found".to_string())?;

    let _permit = GEN_PERMIT.acquire().await.map_err(|e| e.to_string())?;

    // Another request may have finished generation while we waited for the
    // queue — re-check before spawning anything.
    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        return Ok(to_data_url(&bytes));
    }

    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| e.to_string())?;

    let stream_url = format!(
        "http://localhost:{}/stream/{}/{}?token={}",
        config.port, folder_key, message_id, config.token
    );

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
        ^ (message_id as u64);
    let outdir = std::env::temp_dir().join(format!("telestash-thumbgen-{}-{}", message_id, unique));
    let _ = tokio::fs::remove_dir_all(&outdir).await;
    tokio::fs::create_dir_all(&outdir)
        .await
        .map_err(|e| e.to_string())?;

    let result = extract_frame(&mpv_bin, &stream_url, &outdir, &cache_path).await;

    let _ = tokio::fs::remove_dir_all(&outdir).await;

    match result {
        Ok(bytes) => {
            prune_generated_cache(&cache_dir).await;
            Ok(to_data_url(&bytes))
        }
        // Soft-fail: the card keeps its icon; nothing logs as user-facing error.
        Err(e) => {
            log::debug!("Video thumbnail generation skipped for msg {}: {}", message_id, e);
            Ok(String::new())
        }
    }
}

async fn extract_frame(
    mpv_bin: &std::path::Path,
    stream_url: &str,
    outdir: &std::path::Path,
    cache_path: &std::path::Path,
) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncWriteExt;

    let mut child = tokio::process::Command::new(mpv_bin)
        .args([
            "--no-config",
            "--no-terminal",
            "--really-quiet",
            "--vo=image",
            "--vo-image-format=jpeg",
            "--vo-image-quality=85",
        ])
        .arg(format!("--vo-image-outdir={}", outdir.display()))
        .args([
            // Keyframe-relative seek 5s in: skips black intro frames without
            // decoding from the start, and --frames=1 exits after one frame.
            "--start=+5",
            "--frames=1",
            "--audio=no",
            "--no-subtitles",
            "--hwdec=no",
            "--fullscreen=no",
            stream_url,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("MPV spawn failed: {}", e))?;

    // 45s is generous for a 1–5 MB range request plus one frame decode,
    // while guaranteeing no zombie MPV lingers in the tray.
    let status = tokio::time::timeout(std::time::Duration::from_secs(45), child.wait())
        .await
        .map_err(|_| "Frame extraction timed out".to_string())?
        .map_err(|e| format!("MPV wait failed: {}", e))?;

    if !status.success() {
        return Err(format!("MPV exited with {}", status));
    }

    // MPV names extracted frames like 00000001.jpeg inside the outdir.
    let mut read_dir = tokio::fs::read_dir(outdir)
        .await
        .map_err(|e| format!("Outdir unreadable: {}", e))?;
    let mut frame_path: Option<std::path::PathBuf> = None;
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if path.is_file() && (ext == "jpg" || ext == "jpeg") {
            frame_path = Some(path);
            break;
        }
    }
    let frame_path = frame_path.ok_or_else(|| "No frame written by MPV".to_string())?;

    let mut bytes = tokio::fs::read(&frame_path)
        .await
        .map_err(|e| format!("Frame read failed: {}", e))?;
    if bytes.is_empty() {
        return Err("Frame written empty".to_string());
    }

    // Cache under the same naming scheme cmd_get_thumbnail already probes.
    let mut tmp_cache = cache_path.to_path_buf();
    tmp_cache.set_extension("jpg.part");
    let mut part = tokio::fs::File::create(&tmp_cache)
        .await
        .map_err(|e| format!("Cache write failed: {}", e))?;
    part.write_all(&bytes)
        .await
        .map_err(|e| format!("Cache write failed: {}", e))?;
    part.flush()
        .await
        .map_err(|e| format!("Cache write failed: {}", e))?;
    drop(part);
    tokio::fs::rename(&tmp_cache, cache_path)
        .await
        .map_err(|e| format!("Cache rename failed: {}", e))?;

    // mpv may emit jpegs with a JFIF-less header some webviews dislike;
    // data URL uses image/jpeg either way. Cap absurd frame sizes (>2 MB
    // would mean something decoded a poster-sized frame — still fine).
    if bytes.len() > 2 * 1024 * 1024 {
        bytes.truncate(0);
        return Err("Frame unexpectedly large".to_string());
    }
    Ok(bytes)
}

/// LRU prune of the generated cache by byte + file-count cap.
/// Never runs on the UI-critical path (called once after each generation).
async fn prune_generated_cache(cache_dir: &std::path::Path) {
    let dir = cache_dir.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        let mut files: Vec<(std::path::PathBuf, std::time::SystemTime, u64)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    files.push((path, meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH), meta.len()));
                }
            }
        }
        files.sort_by_key(|(_, modified, _)| *modified);
        let mut total: u64 = files.iter().map(|(_, _, len)| *len).sum();
        while files.len() > GEN_CACHE_MAX_FILES || total > GEN_CACHE_MAX_BYTES {
            if let Some((path, _, len)) = files.first().cloned() {
                let _ = std::fs::remove_file(&path);
                total = total.saturating_sub(len);
                files.remove(0);
            } else {
                break;
            }
        }
    })
    .await;
}

fn to_data_url(bytes: &[u8]) -> String {
    format!("data:image/jpeg;base64,{}", general_purpose::STANDARD.encode(bytes))
}

/// Probe whether the message media is a video document, without downloading.
async fn message_is_video(
    state: &State<'_, TelegramState>,
    folder_id: Option<i64>,
    message_id: i32,
) -> Result<bool, String> {
    use grammers_client::media::Media;

    let client_opt = { state.client.lock().await.clone() };
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
    let peer = crate::commands::utils::resolve_peer(&client, folder_id, &state.peer_cache).await?;
    let messages = client
        .get_messages_by_id(peer, &[message_id])
        .await
        .map_err(|e| e.to_string())?;
    let msg = messages
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| "Message not found".to_string())?;

    let is_video = match msg.media() {
        Some(Media::Document(d)) => {
            let mime = d.mime_type().unwrap_or("");
            let name = d.name().unwrap_or_default();
            let ext = std::path::Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            let video_exts = [
                "mp4", "mkv", "webm", "mov", "avi", "ts", "m2ts", "mts", "m4v", "mpg", "mpeg", "flv", "wmv",
            ];
            mime.starts_with("video/") || video_exts.iter().any(|v| *v == ext)
        }
        _ => false,
    };
    Ok(is_video)
}
