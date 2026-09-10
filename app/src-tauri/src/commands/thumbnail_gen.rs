//! Fallback video thumbnail generation for videos that Telegram itself did
//! not thumbnail (large documents). Extracts a single frame through the
//! already-bundled MPV sidecar (`--vo=image`) fed by the local streaming
//! server, so no new dependency and no full-file download — roughly 1–5 MB
//! of stream data per generation, cached as one small JPEG.
//!
//! Lightness guards: single global queue (one MPV at a time), hard timeout,
//! per-message single-flight, and an LRU byte cap on the generated cache.

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
/// Returns the cached JPEG's absolute path for the frontend to load via the
/// asset protocol (convertFileSrc), or "" when generation is not possible
/// (non-video file, MPV missing, timeout) so the UI keeps its icon fallback.
#[tauri::command]
pub async fn cmd_generate_video_thumbnail(
    message_id: i32,
    folder_id: Option<i64>,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
    config: State<'_, StreamConfig>,
) -> Result<String, String> {
    // Per-card: debug, not info. A grid of 200 videos would otherwise bury
    // real failures under its own success path.
    log::debug!("[thumbgen] invoked msg={} folder={:?}", message_id, folder_id);
    let folder_key = folder_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "home".to_string());
    let thumb_url = format!(
        "http://localhost:{}/thumb/generated/{}_{}.jpg?token={}",
        config.port, folder_key, message_id, config.token
    );

    let cache_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?
        .join("generated_thumbs");
    let cache_path = cache_dir.join(format!("{}_{}.jpg", folder_key, message_id));
    if tokio::fs::metadata(&cache_path).await.is_ok() {
        return Ok(thumb_url);
    }

    // Only videos are worth a frame extraction.
    let is_video = message_is_video(&state, folder_id, message_id).await.unwrap_or(false);
    log::debug!("[thumbgen] msg={} is_video={}", message_id, is_video);
    if !is_video {
        return Ok(String::new());
    }

    let mpv_bin = crate::commands::streaming::resolve_mpv_binary(&app_handle)
        .ok_or_else(|| "MPV binary not found".to_string())?;
    log::debug!("[thumbgen] msg={} mpv_bin={:?}", message_id, mpv_bin);

    let _permit = GEN_PERMIT.acquire().await.map_err(|e| e.to_string())?;

    // Another request may have finished generation while we waited for the
    // queue — re-check before spawning anything.
    if tokio::fs::metadata(&cache_path).await.is_ok() {
        return Ok(thumb_url);
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
        Ok(()) => {
            log::debug!("[thumbgen] msg={} frame cached at {}", message_id, cache_path.display());
            prune_generated_cache(&cache_dir).await;
            Ok(thumb_url)
        }
        // Soft-fail: the card keeps its icon. Warn — not debug — so a broken
        // mpv build is visible in the log instead of buried among card noise.
        Err(e) => {
            log::warn!("[thumbgen] msg={} skipped: {}", message_id, e);
            Ok(String::new())
        }
    }
}

/// Build the mpv argument list for a single-frame extraction.
///
/// Every flag here must exist in the *bundled* mpv build (v0.41 ships as the
/// sidecar). mpv treats an unknown option as fatal and exits before it ever
/// opens the stream, so a typo or an upstream rename kills the feature
/// silently — which is exactly how `--vo-image-quality` (renamed to
/// `--vo-image-jpeg-quality`) and `--no-subtitles` (never existed; `--sid=no`
/// is the real flag) broke thumbnail generation. `mpv_args_are_valid_for_041`
/// guards against that drift.
fn frame_extraction_args(outdir: &std::path::Path, stream_url: &str) -> Vec<String> {
    vec![
        "--no-config".to_string(),
        "--no-terminal".to_string(),
        "--vo=image".to_string(),
        "--vo-image-format=jpeg".to_string(),
        "--vo-image-jpeg-quality=85".to_string(),
        format!("--vo-image-outdir={}", outdir.display()),
        // Keyframe-relative seek 5s in: skips black intro frames without
        // decoding from the start, and --frames=1 exits after one frame.
        "--start=+5".to_string(),
        "--frames=1".to_string(),
        "--audio=no".to_string(),
        "--sid=no".to_string(),
        "--hwdec=no".to_string(),
        "--fullscreen=no".to_string(),
        stream_url.to_string(),
    ]
}

async fn extract_frame(
    mpv_bin: &std::path::Path,
    stream_url: &str,
    outdir: &std::path::Path,
    cache_path: &std::path::Path,
) -> Result<(), String> {
    let mut child = tokio::process::Command::new(mpv_bin)
        .args(frame_extraction_args(outdir, stream_url))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        // The 45s timeout below drops the wait future; without this a hung mpv
        // would keep running as an orphan instead of dying with the request.
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("MPV spawn failed: {}", e))?;

    // 45s is generous for a 1–5 MB range request plus one frame decode,
    // while guaranteeing no zombie MPV lingers in the tray.
    let output = tokio::time::timeout(std::time::Duration::from_secs(45), child.wait_with_output())
        .await
        .map_err(|_| "Frame extraction timed out".to_string())?
        .map_err(|e| format!("MPV wait failed: {}", e))?;

    if !output.status.success() {
        // Keep the reason: mpv reports bad options and stream errors on stderr,
        // and swallowing it made every failure look identical to a crash.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().last().unwrap_or("").trim();
        return Err(format!("MPV exited with {}: {}", output.status, detail));
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

    let frame_len = tokio::fs::metadata(&frame_path)
        .await
        .map_err(|e| format!("Frame stat failed: {}", e))?
        .len();
    if frame_len == 0 {
        return Err("Frame written empty".to_string());
    }
    // Cap absurd frame sizes (>2 MB would mean something decoded a
    // poster-sized frame — still fine, but not worth caching).
    if frame_len > 2 * 1024 * 1024 {
        return Err("Frame unexpectedly large".to_string());
    }

    // Cache under the same naming scheme cmd_get_thumbnail already probes.
    // The frame goes straight from MPV's outdir into the cache via rename-
    // through-temp — no bytes are ever buffered for a base64 round-trip.
    let mut tmp_cache = cache_path.to_path_buf();
    tmp_cache.set_extension("jpg.part");
    if tokio::fs::rename(&frame_path, &tmp_cache).await.is_err() {
        // Cross-device rename can fail; fall back to copy+delete.
        tokio::fs::copy(&frame_path, &tmp_cache)
            .await
            .map_err(|e| format!("Cache write failed: {}", e))?;
        let _ = tokio::fs::remove_file(&frame_path).await;
    }
    tokio::fs::rename(&tmp_cache, cache_path)
        .await
        .map_err(|e| format!("Cache rename failed: {}", e))?;

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Locks the mpv flag names against the bundled v0.41 build. mpv aborts on
    /// an unknown option before opening the stream, so a rename upstream (or a
    /// typo here) silently disables every generated thumbnail. This test is the
    /// tripwire that turns that silent failure into a red CI run.
    #[test]
    fn mpv_args_are_valid_for_041() {
        let args = frame_extraction_args(Path::new(r"C:\tmp\out"), "http://localhost:1/s");
        let joined = args.join(" ");

        // Renamed in mpv 0.41 — the old name is fatal.
        assert!(joined.contains("--vo-image-jpeg-quality=85"));
        assert!(!joined.contains("--vo-image-quality"));

        // Never existed — the real flag is --sid=no.
        assert!(joined.contains("--sid=no"));
        assert!(!joined.contains("--no-subtitles"));

        // Core options that must survive any future refactor.
        assert!(joined.contains("--vo=image"));
        assert!(joined.contains("--vo-image-format=jpeg"));
        assert!(joined.contains("--frames=1"));
        assert!(args.last().map(String::as_str) == Some("http://localhost:1/s"));
    }

    #[test]
    fn mpv_outdir_is_passed_through() {
        let args = frame_extraction_args(Path::new(r"C:\tmp\out"), "url");
        assert!(args.iter().any(|a| a == r"--vo-image-outdir=C:\tmp\out"));
    }
}
