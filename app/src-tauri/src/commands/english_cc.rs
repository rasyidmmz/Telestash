use std::sync::Arc;
use std::path::{Path, PathBuf};
use tokio::sync::{Mutex, oneshot};
use serde::{Serialize, Deserialize};
use tauri::{State, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tokio::io::AsyncBufReadExt;
use grammers_client::media::Media;
use crate::TelegramState;
use crate::commands::streaming::StreamConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EnglishCcPhase {
    Idle,
    Extracting,
    Transcribing,
    Ready,
    Error,
    Cancelled,
}

#[derive(Serialize, Clone)]
pub struct EnglishCcStatus {
    pub file_key: String,
    pub phase: EnglishCcPhase,
    pub progress: Option<f32>,
    pub cached: bool,
    pub error: Option<String>,
}

pub struct ActiveJob {
    pub message_id: i32,
    pub folder_id: Option<i64>,
    pub phase: EnglishCcPhase,
    pub progress: Option<f32>,
    pub error: Option<String>,
    pub cancel_tx: Option<oneshot::Sender<()>>,
}

pub struct CcManagerState {
    pub active_job: Option<ActiveJob>,
}

pub struct EnglishCcManager {
    pub state: Arc<Mutex<CcManagerState>>,
}

impl EnglishCcManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(CcManagerState { active_job: None })),
        }
    }
}

pub fn get_srt_path(app_handle: &tauri::AppHandle, message_id: i32, folder_id: Option<i64>) -> PathBuf {
    let parent = app_handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("streaming")
        .join("captions");
    let _ = std::fs::create_dir_all(&parent);
    parent.join(format!("{}_{}.en.srt", folder_id.unwrap_or(0), message_id))
}

async fn get_video_duration(
    client: &grammers_client::Client,
    message_id: i32,
    folder_id: Option<i64>,
    state: &TelegramState,
) -> Option<f64> {
    let peer = crate::commands::utils::resolve_peer(client, folder_id, &state.peer_cache).await.ok()?;
    let messages = client.get_messages_by_id(peer, &[message_id]).await.ok()?;
    let msg = messages.into_iter().flatten().next()?;
    let media = msg.media()?;
    
    let size = match &media {
        Media::Document(d) => d.size().unwrap_or(0) as u64,
        _ => return None,
    };
    
    // Download first 2MB
    let max_bytes = std::cmp::min(2 * 1024 * 1024, size) as usize;
    let mut buffer: Vec<u8> = Vec::with_capacity(max_bytes);
    let mut download_iter = client.iter_download(&media);
    download_iter = download_iter.chunk_size(65536);
    
    while buffer.len() < max_bytes {
        if let Ok(Some(chunk)) = download_iter.next().await {
            let remaining = max_bytes.saturating_sub(buffer.len());
            let take = std::cmp::min(chunk.len(), remaining);
            buffer.extend_from_slice(&chunk[..take]);
        } else {
            break;
        }
    }
    
    // Check if MKV
    if let Some((mkv_dur, _, _)) = crate::mp4_utils::parse_mkv_metadata(&buffer) {
        if mkv_dur.is_some() {
            return mkv_dur;
        }
    }
    
    // Check if MP4
    let mut cursor = std::io::Cursor::new(&buffer);
    if let Ok(context) = mp4parse::read_mp4(&mut cursor) {
        if let Some(video_track) = context.tracks.iter().find(|t| t.track_type == mp4parse::TrackType::Video) {
            if let (Some(d), Some(ts)) = (&video_track.duration, &video_track.timescale) {
                return Some((d.0 as f64) / (ts.0 as f64));
            }
        }
    }
    
    None
}

async fn extract_audio_mpv(
    app_handle: &tauri::AppHandle,
    stream_url: &str,
    token: Option<&str>,
    output_wav: &Path,
    cancel_rx: &mut oneshot::Receiver<()>,
) -> Result<(), String> {
    let args = build_audio_extraction_args(stream_url, token, output_wav);

    // Resolve mpv sidecar or system
    use tauri_plugin_shell::ShellExt;
    if let Ok(sidecar) = app_handle.shell().sidecar("mpv") {
        let sidecar_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let (mut rx, child) = sidecar.args(sidecar_refs).spawn()
            .map_err(|e| format!("Failed to spawn mpv sidecar: {}", e))?;

        let mut diagnostics = Vec::new();
        tokio::select! {
            res = async {
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stderr(output) => append_mpv_diagnostic(&mut diagnostics, &output),
                        CommandEvent::Terminated(payload) => {
                            if payload.code == Some(0) {
                                return Ok(());
                            }
                            return Err(mpv_exit_error(payload.code, &diagnostics));
                        }
                        CommandEvent::Error(error) => append_mpv_diagnostic(&mut diagnostics, error.as_bytes()),
                        CommandEvent::Stdout(_) => {}
                        _ => {}
                    }
                }
                Err("mpv connection closed prematurely".to_string())
            } => res,
            _ = cancel_rx => {
                let _ = child.kill();
                Err("Cancelled".to_string())
            }
        }
    } else {
        let mut cmd = tokio::process::Command::new("mpv");
        cmd.args(&args)
           .stdout(std::process::Stdio::null())
           .stderr(std::process::Stdio::null())
           .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn system mpv: {}", e))?;

        tokio::select! {
            status = child.wait() => {
                let status = status.map_err(|e| e.to_string())?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("mpv system command failed: {:?}", status.code()))
                }
            }
            _ = cancel_rx => {
                let _ = child.kill().await;
                Err("Cancelled".to_string())
            }
        }
    }
}

fn build_audio_extraction_args(
    stream_url: &str,
    token: Option<&str>,
    output_wav: &Path,
) -> Vec<String> {
    let wav_path_str = output_wav.to_string_lossy().replace('\\', "/");
    let mut args = vec![
        "--no-video".to_string(),
        "--benchmark".to_string(),
        format!("--ao=pcm:file={}", wav_path_str),
        "--af=lavfi=[aresample=16000,pan=mono|c0=c0]".to_string(),
    ];
    if let Some(token) = token {
        args.push(format!("--http-header-fields=X-TeleStash-Stream-Token: {}", token));
    }
    // mpv applies per-file options to following inputs. Keep the authenticated
    // stream URL last so the header and extraction options apply to it.
    args.push(stream_url.to_string());
    args
}

fn append_mpv_diagnostic(diagnostics: &mut Vec<String>, output: &[u8]) {
    for line in String::from_utf8_lossy(output).lines() {
        let line = line.trim();
        if !line.is_empty() {
            diagnostics.push(line.chars().take(300).collect());
        }
    }
    if diagnostics.len() > 8 {
        diagnostics.drain(..diagnostics.len() - 8);
    }
}

fn mpv_exit_error(code: Option<i32>, diagnostics: &[String]) -> String {
    if diagnostics.is_empty() {
        format!("mpv exited with code {:?}", code)
    } else {
        format!("mpv exited with code {:?}: {}", code, diagnostics.join(" | "))
    }
}

pub fn parse_whisper_timestamp(line: &str) -> Option<f32> {
    let start_idx = line.find('[')?;
    let arrow_idx = line.find("-->")?;
    let end_idx = line.find(']')?;
    if start_idx < arrow_idx && arrow_idx < end_idx {
        let time_str = line[arrow_idx + 3..end_idx].trim();
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 3 {
            let hours: f32 = parts[0].parse().ok()?;
            let minutes: f32 = parts[1].parse().ok()?;
            let seconds: f32 = parts[2].parse().ok()?;
            return Some(hours * 3600.0 + minutes * 60.0 + seconds);
        }
    }
    None
}

fn resolve_whisper_binary(app_handle: &tauri::AppHandle, binary_name: &str) -> Option<PathBuf> {
    if let Ok(res_dir) = app_handle.path().resource_dir() {
        let candidates = [
            res_dir.join("resources").join("whisper").join(binary_name),
            res_dir.join("whisper").join(binary_name),
            res_dir.join(binary_name),
        ];
        for c in candidates {
            if c.exists() { return Some(c); }
        }
    }
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let candidates = [
            app_dir.join("resources").join("whisper").join(binary_name),
            app_dir.join("whisper").join(binary_name),
            app_dir.join(binary_name),
        ];
        for c in candidates {
            if c.exists() { return Some(c); }
        }
    }
    if let Ok(exec_path) = std::env::current_exe() {
        if let Some(exec_dir) = exec_path.parent() {
            let candidates = [
                exec_dir.join("resources").join("whisper").join(binary_name),
                exec_dir.join("whisper").join(binary_name),
                exec_dir.join(binary_name),
            ];
            for c in candidates {
                if c.exists() { return Some(c); }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let candidates = [
            cwd.join("resources").join("whisper").join(binary_name),
            cwd.join("whisper").join(binary_name),
            cwd.join(binary_name),
        ];
        for c in candidates {
            if c.exists() { return Some(c); }
        }
    }
    None
}

async fn run_whisper_transcription(
    app_handle: &tauri::AppHandle,
    wav_path: &Path,
    srt_base_path: &Path,
    duration_secs: f32,
    manager_state: Arc<Mutex<CcManagerState>>,
    cancel_rx: &mut oneshot::Receiver<()>,
) -> Result<(), String> {
    let whisper_cli = resolve_whisper_binary(app_handle, "whisper-cli.exe")
        .ok_or_else(|| "whisper-cli.exe was not found in application directory.".to_string())?;

    let model_path = resolve_whisper_binary(app_handle, "ggml-base.en.bin")
        .ok_or_else(|| "ggml-base.en.bin model was not found in application directory.".to_string())?;

    let threads = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2)
        .max(1)
        .min(2)
        .to_string();

    let mut cmd = tokio::process::Command::new(&whisper_cli);
    cmd.args(&[
        "-m", &model_path.to_string_lossy(),
        "-f", &wav_path.to_string_lossy(),
        "-osrt",
        "-of", &srt_base_path.to_string_lossy(),
        "-t", &threads,
        "--split-on-word",
        "-ml", "42"
    ]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x00004000); // BELOW_NORMAL_PRIORITY_CLASS
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| format!("Failed to launch whisper-cli: {}", e))?;
    let stderr = child.stderr.take().ok_or("Failed to grab stderr stream")?;
    let mut reader = tokio::io::BufReader::new(stderr).lines();

    let monitor_fut = async {
        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(secs) = parse_whisper_timestamp(&line) {
                if duration_secs > 0.0 {
                    let progress = ((secs / duration_secs) * 100.0).min(100.0);
                    let mut state = manager_state.lock().await;
                    if let Some(ref mut job) = state.active_job {
                        job.progress = Some(progress);
                    }
                }
            }
        }
        let status = child.wait().await.map_err(|e| format!("whisper-cli join failed: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err("whisper-cli exited with non-zero code".to_string())
        }
    };

    tokio::select! {
        res = monitor_fut => res,
        _ = cancel_rx => {
            let _ = child.kill().await;
            Err("Cancelled".to_string())
        }
    }
}

#[tauri::command]
pub async fn cmd_generate_english_cc(
    message_id: i32,
    folder_id: Option<i64>,
    force: bool,
    manager: State<'_, EnglishCcManager>,
    app_handle: tauri::AppHandle,
) -> Result<EnglishCcStatus, String> {
    let file_key = format!("{}_{}", folder_id.unwrap_or(0), message_id);
    let srt_path = get_srt_path(&app_handle, message_id, folder_id);

    if !force && srt_path.exists() {
        return Ok(EnglishCcStatus {
            file_key,
            phase: EnglishCcPhase::Ready,
            progress: Some(100.0),
            cached: true,
            error: None,
        });
    }

    let mut state = manager.state.lock().await;
    if state.active_job.is_some() {
        return Err("Ada pekerjaan generate subtitle lain yang sedang berjalan.".to_string());
    }

    let (cancel_tx, mut cancel_rx) = oneshot::channel();
    
    state.active_job = Some(ActiveJob {
        message_id,
        folder_id,
        phase: EnglishCcPhase::Extracting,
        progress: Some(0.0),
        error: None,
        cancel_tx: Some(cancel_tx),
    });

    let manager_state_clone = manager.state.clone();
    let app_handle_clone = app_handle.clone();
    let job_file_key = file_key.clone();
    
    tauri::async_runtime::spawn(async move {
        let run_result = async {
            let state_tg = app_handle_clone.state::<TelegramState>();
            let client_opt = { state_tg.client.lock().await.clone() };
            let client = client_opt.ok_or_else(|| "Not connected to Telegram".to_string())?;

            // 1. Get video duration
            let duration_secs = get_video_duration(&client, message_id, folder_id, &state_tg)
                .await
                .unwrap_or(120.0);

            // 2. Build local stream URL
            let stream_config = app_handle_clone.state::<StreamConfig>();
            let folder_id_str = match folder_id {
                Some(id) => id.to_string(),
                None => "home".to_string(),
            };
            let stream_url = format!(
                "http://localhost:{}/stream/{}/{}?token={}",
                stream_config.port, folder_id_str, message_id, stream_config.token
            );
            let token = Some(stream_config.token.as_str());

            let temp_dir = std::env::temp_dir();
            let wav_path = temp_dir.join(format!("{}_temp.wav", job_file_key));
            let srt_temp_base = temp_dir.join(format!("{}_temp", job_file_key));
            let srt_temp_file = temp_dir.join(format!("{}_temp.srt", job_file_key));

            let _ = std::fs::remove_file(&wav_path);
            let _ = std::fs::remove_file(&srt_temp_file);

            // Audio Extraction Phase
            extract_audio_mpv(&app_handle_clone, &stream_url, token, &wav_path, &mut cancel_rx).await?;

            // Transcription Phase
            {
                let mut state = manager_state_clone.lock().await;
                if let Some(ref mut job) = state.active_job {
                    job.phase = EnglishCcPhase::Transcribing;
                    job.progress = Some(0.0);
                }
            }

            run_whisper_transcription(
                &app_handle_clone,
                &wav_path,
                &srt_temp_base,
                duration_secs as f32,
                manager_state_clone.clone(),
                &mut cancel_rx,
            ).await?;

            // Save atomic file locally and auto-upload to Telegram folder
            if srt_temp_file.exists() {
                let _ = std::fs::create_dir_all(srt_path.parent().unwrap());
                std::fs::copy(&srt_temp_file, &srt_path)
                    .map_err(|e| format!("Failed to save final subtitle: {}", e))?;

                // Auto-upload .en.srt to Telegram folder
                if let Ok(peer) = crate::commands::utils::resolve_peer(&client, folder_id, &state_tg.peer_cache).await {
                    if let Ok(messages) = client.get_messages_by_id(peer, &[message_id]).await {
                        if let Some(msg) = messages.into_iter().flatten().next() {
                            let video_name = match msg.media() {
                                Some(Media::Document(d)) => d.name().unwrap_or_default().to_string(),
                                _ => format!("{}_{}.mkv", folder_id.unwrap_or(0), message_id),
                            };
                            let stem = std::path::Path::new(&video_name)
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy();
                            let srt_name = format!("{}.en.srt", stem);

                            if let Ok(srt_bytes) = std::fs::read(&srt_temp_file) {
                                let srt_len = srt_bytes.len();
                                let mut cursor = std::io::Cursor::new(srt_bytes);
                                if let Ok(uploaded) = client.upload_stream(&mut cursor, srt_len, srt_name.clone()).await {
                                    let _ = client.send_message(peer, grammers_client::message::InputMessage::new().file(uploaded)).await;
                                    crate::transfer_log::record_transfer_log(
                                        "Subtitle Auto-Upload",
                                        &format!("Subtitle {} uploaded successfully to Telegram folder.", srt_name),
                                        None,
                                    );
                                }
                            }
                        }
                    }
                }
            } else {
                return Err("Temporary subtitle file not found after transcription.".to_string());
            }

            // Cleanup
            let _ = std::fs::remove_file(&wav_path);
            let _ = std::fs::remove_file(&srt_temp_file);
            Ok::<(), String>(())
        }.await;

        let mut state = manager_state_clone.lock().await;
        if let Some(ref mut job) = state.active_job {
            match run_result {
                Ok(_) => {
                    job.phase = EnglishCcPhase::Ready;
                    job.progress = Some(100.0);
                }
                Err(err) => {
                    if err == "Cancelled" {
                        job.phase = EnglishCcPhase::Cancelled;
                    } else {
                        job.phase = EnglishCcPhase::Error;
                        job.error = Some(err);
                    }
                }
            }
        }
    });

    Ok(EnglishCcStatus {
        file_key,
        phase: EnglishCcPhase::Extracting,
        progress: Some(0.0),
        cached: false,
        error: None,
    })
}

#[tauri::command]
pub async fn cmd_get_english_cc_status(
    message_id: i32,
    folder_id: Option<i64>,
    manager: State<'_, EnglishCcManager>,
) -> Result<EnglishCcStatus, String> {
    let mut state = manager.state.lock().await;
    let file_key = format!("{}_{}", folder_id.unwrap_or(0), message_id);
    let srt_exists = {
        if let Some(ref job) = state.active_job {
            job.message_id == message_id && job.folder_id == folder_id
        } else {
            false
        }
    };

    if srt_exists {
        let job = state.active_job.as_ref().unwrap();
        let status = EnglishCcStatus {
            file_key,
            phase: job.phase.clone(),
            progress: job.progress,
            cached: false,
            error: job.error.clone(),
        };
        // Reset state back to None if done/error/cancelled so new jobs can be run
        if job.phase == EnglishCcPhase::Ready || job.phase == EnglishCcPhase::Error || job.phase == EnglishCcPhase::Cancelled {
            state.active_job = None;
        }
        return Ok(status);
    }

    Ok(EnglishCcStatus {
        file_key,
        phase: EnglishCcPhase::Idle,
        progress: None,
        cached: false,
        error: None,
    })
}

#[tauri::command]
pub async fn cmd_cancel_english_cc(
    message_id: i32,
    folder_id: Option<i64>,
    manager: State<'_, EnglishCcManager>,
) -> Result<(), String> {
    let mut state = manager.state.lock().await;
    if let Some(ref mut job) = state.active_job {
        if job.message_id == message_id && job.folder_id == folder_id {
            if let Some(cancel_tx) = job.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
            job.phase = EnglishCcPhase::Cancelled;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_extraction_args() {
        let url = "http://localhost:14201/stream/home/10";
        let token = Some("mytoken");
        let output_wav = std::path::Path::new("temp.wav");
        let args = build_audio_extraction_args(url, token, output_wav);

        assert_eq!(args[0], "--no-video");
        assert_eq!(args[1], "--benchmark");
        assert_eq!(args[2], "--ao=pcm:file=temp.wav");
        assert_eq!(args[4], "--http-header-fields=X-TeleStash-Stream-Token: mytoken");
        assert_eq!(args.last().map(String::as_str), Some(url));
    }

    #[test]
    fn mpv_exit_error_includes_bounded_diagnostics() {
        let mut diagnostics = Vec::new();
        append_mpv_diagnostic(&mut diagnostics, b"[ffmpeg] HTTP error 403 Forbidden\n");

        assert_eq!(
            mpv_exit_error(Some(1), &diagnostics),
            "mpv exited with code Some(1): [ffmpeg] HTTP error 403 Forbidden"
        );
    }

    #[test]
    fn test_parse_whisper_timestamp() {
        let line = "   [01:15:32.500 --> 01:15:35.800]   Hello world";
        let seconds = parse_whisper_timestamp(line).unwrap();
        assert!((seconds - 4535.800).abs() < 0.001);
    }
}
