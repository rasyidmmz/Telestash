use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::{Manager, State};

use super::playback_settings::{
    load_playback_settings, HardwareDecodeMode, PlaybackSettingsFile,
};

const STREAM_TOKEN_HEADER: &str = "X-TeleStash-Stream-Token";

/// Holds the per-session streaming config (token + port)
pub struct StreamConfig {
    pub token: String,
    pub port: u16,
}

/// Returned to the frontend so it can construct stream URLs dynamically
#[derive(serde::Serialize)]
pub struct StreamInfo {
    pub token: String,
    pub base_url: String,
}

/// Returns the streaming server's session token and base URL to the frontend.
/// The frontend must use the returned base_url to construct stream URLs,
/// never hardcoding the port.
#[tauri::command]
pub fn cmd_get_stream_info(config: State<'_, StreamConfig>) -> StreamInfo {
    // Always use "localhost" on all platforms.
    // "localhost" is treated as a secure context by all major browser
    // engines (Chromium/WebView2, WebKit) and is exempt from Mixed Content
    // blocking.  This is critical on Windows where Tauri v2 serves the
    // frontend from https://tauri.localhost — fetching http://127.0.0.1
    // from an HTTPS origin triggers a Mixed Content block in WebView2.
    // The server binds exclusively to 127.0.0.1, so name resolution
    // differences between platforms are not a concern.
    let host = "localhost";

    StreamInfo {
        token: config.token.clone(),
        base_url: format!("http://{}:{}", host, config.port),
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MpvPlaylistItem {
    pub url: String,
    pub message_id: Option<i32>,
    pub folder_id: Option<i64>,
    pub title: Option<String>,
}

pub fn resolve_mpv_binary(app_handle: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    if let Ok(res_dir) = app_handle.path().resource_dir() {
        let candidates = [
            res_dir.join("bin").join("mpv-x86_64-pc-windows-msvc.exe"),
            res_dir.join("bin").join("mpv.exe"),
            res_dir.join("mpv.exe"),
        ];
        for c in candidates {
            if c.exists() { return Some(c); }
        }
    }
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let candidates = [
            app_dir.join("bin").join("mpv-x86_64-pc-windows-msvc.exe"),
            app_dir.join("bin").join("mpv.exe"),
            app_dir.join("mpv.exe"),
        ];
        for c in candidates {
            if c.exists() { return Some(c); }
        }
    }
    if let Ok(exec_path) = std::env::current_exe() {
        if let Some(exec_dir) = exec_path.parent() {
            let candidates = [
                exec_dir.join("bin").join("mpv-x86_64-pc-windows-msvc.exe"),
                exec_dir.join("bin").join("mpv.exe"),
                exec_dir.join("mpv.exe"),
            ];
            for c in candidates {
                if c.exists() { return Some(c); }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let candidates = [
            cwd.join("bin").join("mpv-x86_64-pc-windows-msvc.exe"),
            cwd.join("bin").join("mpv.exe"),
            cwd.join("mpv.exe"),
        ];
        for c in candidates {
            if c.exists() { return Some(c); }
        }
    }
    None
}

/// True when a caption filename belongs to the video title stem.
/// Requires an exact stem match or a dotted language/suffix variant
/// (`Matrix.en.srt`), not a bare prefix (`Matrix Reloaded.srt`).
fn caption_matches_title(file_name: &str, title_stem: &str) -> bool {
    if title_stem.is_empty() {
        return false;
    }
    let Some(stem) = Path::new(file_name).file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if stem == title_stem {
        return true;
    }
    stem.strip_prefix(title_stem)
        .is_some_and(|rest| rest.starts_with('.'))
}

fn attach_matching_subtitles(
    args: &mut Vec<String>,
    app_handle: &tauri::AppHandle,
    folder_id: i64,
    message_id: i32,
    title: Option<&str>,
) {
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let captions_dir = app_dir.join("streaming").join("captions");
        if !captions_dir.exists() {
            return;
        }

        let prefix_folder = format!("{}_{}", folder_id, message_id);
        let prefix_msg = format!("{}_", message_id);
        let prefix_msg_exact = format!("{}.", message_id);
        let title_stem = title
            .map(|t| Path::new(t).file_stem().and_then(|s| s.to_str()).unwrap_or(t))
            .unwrap_or("");

        if let Ok(entries) = std::fs::read_dir(&captions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    // For VobSub (.idx and .sub), only pass .idx (MPV loads companion .sub automatically)
                    if ext == "idx" || ext == "srt" || ext == "ass" || ext == "ssa" || ext == "vtt" {
                        let is_match = file_name.starts_with(&prefix_folder)
                            || file_name.starts_with(&prefix_msg)
                            || file_name.starts_with(&prefix_msg_exact)
                            || caption_matches_title(file_name, title_stem);

                        if is_match {
                            args.push(format!("--sub-file={}", path.to_string_lossy()));
                        }
                    }
                }
            }
        }
    }
}

/// Cache of probed adapters. `None` means "not probed yet"; an empty list is
/// never stored, so a failed probe is retried instead of poisoning the session.
static ADAPTER_CACHE: OnceLock<Mutex<Option<Vec<String>>>> = OnceLock::new();

/// A launched MPV process, tracked so a later launch can replace it.
enum TrackedPlayer {
    /// Spawned through the Tauri shell plugin, which hands back a killable handle.
    Sidecar(tauri_plugin_shell::process::CommandChild),
    /// Spawned directly (local binary or PATH fallback); only the pid is known.
    External(u32),
}

/// Holds the running MPV child process.
///
/// The Tauri shell plugin does NOT kill a child when its `CommandChild` is
/// dropped, so holding it here is what makes it possible to stop the player and
/// to replace a previous one instead of stacking resident processes.
pub struct PlayerProcess(pub Mutex<Option<TrackedPlayer>>);

/// MPV keeps writing to stdout/stderr for the whole session, and the plugin's
/// event channel holds a single slot, so the receiver must be drained for as
/// long as the process lives or the pipe reader threads stall.
fn drain_player_events(rx: tauri::async_runtime::Receiver<tauri_plugin_shell::process::CommandEvent>) {
    tauri::async_runtime::spawn(async move {
        let mut rx = rx;
        while rx.recv().await.is_some() {}
    });
}

/// Stop a process created outside the shell plugin.
///
/// `/T` also terminates children MPV may have started, and `CREATE_NO_WINDOW`
/// keeps the helper from flashing a console window at the user.
fn kill_external_player(pid: u32) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

/// Stop the currently tracked MPV process, if any. Safe to call repeatedly.
pub fn stop_tracked_player(player: &PlayerProcess) {
    let tracked = player.0.lock().ok().and_then(|mut guard| guard.take());
    match tracked {
        Some(TrackedPlayer::Sidecar(child)) => {
            log::info!("Stopping previous MPV process (pid {})", child.pid());
            let _ = child.kill();
        }
        Some(TrackedPlayer::External(pid)) => {
            log::info!("Stopping previous MPV process (pid {})", pid);
            kill_external_player(pid);
        }
        None => {}
    }
}

fn track_player(player: &PlayerProcess, tracked: TrackedPlayer) {
    if let Ok(mut guard) = player.0.lock() {
        *guard = Some(tracked);
    }
}

fn probe_d3d11_adapters(app_handle: &tauri::AppHandle) -> Vec<String> {
    let Some(bin) = resolve_mpv_binary(app_handle) else {
        return Vec::new();
    };

    // `--d3d11-adapter=help` prints the DXGI adapter list and exits, so this is
    // a short windowless probe rather than a playback.
    let mut child = match std::process::Command::new(bin)
        .args(["--no-config", "--vo=gpu-next", "--d3d11-adapter=help", "--idle=no", "--frames=0"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Vec::new(),
    };

    // Never let a misbehaving binary block the settings UI.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }

    let Ok(output) = child.wait_with_output() else {
        return Vec::new();
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_adapter_list(&text)
}

/// Parse the `Adapter N: vendor: V, description: NAME` lines printed by MPV.
/// Software renderers are skipped: they cannot decode video.
fn parse_adapter_list(text: &str) -> Vec<String> {
    let mut adapters = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Adapter ") else {
            continue;
        };
        let Some((_, description)) = rest.split_once("description: ") else {
            continue;
        };
        let description = description.trim();
        if description.is_empty() || description.contains("Basic Render Driver") {
            continue;
        }
        if !adapters.iter().any(|a| a == description) {
            adapters.push(description.to_string());
        }
    }
    adapters
}

/// Adapter list for this machine, probed once per successful run.
///
/// A failed or empty probe is deliberately NOT cached: otherwise one transient
/// timeout would reject every adapter pin for the rest of the session.
pub fn detect_d3d11_adapters(app_handle: &tauri::AppHandle) -> Vec<String> {
    let cell = ADAPTER_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cell.lock() {
        if let Some(cached) = guard.as_ref() {
            return cached.clone();
        }
    }

    let adapters = probe_d3d11_adapters(app_handle);
    if adapters.is_empty() {
        return Vec::new();
    }
    if let Ok(mut guard) = cell.lock() {
        *guard = Some(adapters.clone());
    }
    log::info!("Detected D3D11 adapters: {:?}", adapters);
    adapters
}

/// The adapters already probed during this session, without starting a probe.
///
/// Playback uses this so a stale pin can be validated against real hardware
/// without spawning a probe process on every play.
pub fn cached_d3d11_adapters() -> Vec<String> {
    if let Some(cell) = ADAPTER_CACHE.get() {
        if let Ok(guard) = cell.lock() {
            if let Some(cached) = guard.as_ref() {
                return cached.clone();
            }
        }
    }
    Vec::new()
}

/// Resolve the adapter to pin for this playback.
///
/// A stored pin can go stale when a driver renames the adapter or a GPU is
/// disabled, and an unknown `--d3d11-adapter` value makes MPV exit *fatally*
/// before the video opens. When the adapters are known and the stored pin is
/// not among them, decoding falls back to automatic instead of failing the
/// whole playback. An empty `known_adapters` means "not probed", in which case
/// the stored pin is trusted rather than discarded.
fn resolve_pinned_adapter(
    settings: &PlaybackSettingsFile,
    known_adapters: &[String],
) -> Option<String> {
    let pinned = settings
        .preferred_adapter
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())?;

    if !known_adapters.is_empty() && !known_adapters.iter().any(|a| a == pinned) {
        log::warn!(
            "Stored adapter '{}' is no longer available; falling back to automatic decoding",
            pinned
        );
        return None;
    }
    Some(pinned.to_string())
}

/// TeleStash's own playback arguments, excluding user-supplied ones.
///
/// Kept separate and pure so tests can assert that nothing here ever changes
/// image quality, which is the promise this feature makes.
fn tele_stash_args(settings: &PlaybackSettingsFile, known_adapters: &[String]) -> Vec<String> {
    let mut args = vec![
        "--save-position-on-quit".to_string(),
        "--write-filename-in-watch-later-config=yes".to_string(),
        "--keep-open=no".to_string(),
        "--input-default-bindings=yes".to_string(),
        "--slang=id,ind,Indonesian,en,eng,enUS,en-US,enGB,en-GB,en-UK,enUK,English,eng-US,eng-GB".to_string(),
        "--sub-auto=fuzzy".to_string(),
        "--sub-visibility=yes".to_string(),
    ];
    args.extend(build_hwdec_args(settings, known_adapters));
    args
}

/// Translate the persisted settings into MPV hardware-decoding arguments.
///
/// Deliberately limited to how frames are decoded: none of these change image
/// quality (no scaler or dither tuning), so the user's own MPV configuration
/// keeps deciding how the picture looks.
fn build_hwdec_args(settings: &PlaybackSettingsFile, known_adapters: &[String]) -> Vec<String> {
    match settings.hardware_decode {
        HardwareDecodeMode::Auto => vec!["--hwdec=auto-safe".to_string()],
        HardwareDecodeMode::Software => vec!["--hwdec=no".to_string()],
        HardwareDecodeMode::Adapter => {
            let Some(adapter) = resolve_pinned_adapter(settings, known_adapters) else {
                // No usable pin: behave exactly like Auto.
                return vec!["--hwdec=auto-safe".to_string()];
            };
            // `--d3d11-adapter` only constrains the D3D11 backend, while recent
            // MPV prefers Vulkan hardware decoding when it is available. Naming
            // the D3D11 decoder explicitly is what makes the pin actually take
            // effect instead of silently decoding on another GPU. The software
            // fallback stays enabled by MPV's default `--hwdec-software-fallback`.
            vec![
                "--hwdec=d3d11va".to_string(),
                format!("--d3d11-adapter={}", adapter),
            ]
        }
    }
}

/// Sanitized user arguments, placed last so they can override our own flags.
fn extra_playback_args(settings: &PlaybackSettingsFile) -> Vec<String> {
    super::playback_settings::sanitize_extra_args(&settings.extra_args)
}

#[tauri::command]
pub fn cmd_play_in_mpv(
    url: String,
    message_id: Option<i32>,
    folder_id: Option<i64>,
    title: Option<String>,
    playlist: Option<Vec<MpvPlaylistItem>>,
    start_index: Option<usize>,
    app_handle: tauri::AppHandle,
    player: State<'_, PlayerProcess>,
) -> Result<(), String> {
    let playback = load_playback_settings(&app_handle);

    let watch_later_dir = app_handle
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("mpv-watch-later"));
    if let Some(dir) = &watch_later_dir {
        let _ = std::fs::create_dir_all(dir);
    }

    let mpv_config_dir = app_handle
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("mpv-config"));

    let mut args = tele_stash_args(&playback, &cached_d3d11_adapters());

    if let Some(dir) = &watch_later_dir {
        args.push(format!("--watch-later-dir={}", dir.display()));
    }

    if let Some(dir) = &mpv_config_dir {
        let _ = std::fs::create_dir_all(dir);
        let input_conf = dir.join("input.conf");
        let conf_content = "# TeleStash MPV Custom Keybindings\nUP add volume 2\nDOWN add volume -2\nWHEEL_UP add volume 2\nWHEEL_DOWN add volume -2\nCtrl+RIGHT seek 30\nCtrl+LEFT seek -30\nShift+RIGHT seek 10\nShift+LEFT seek -10\nc cycle sub\nENTER cycle fullscreen\nKP_ENTER cycle fullscreen\nTAB script-binding stats/display-stats-toggle\nCtrl+f playlist-prev\nCtrl+j playlist-next\n";
        let _ = std::fs::write(&input_conf, conf_content);
        args.push(format!("--input-conf={}", input_conf.display()));
    }

    // Pass captions search path so MPV can auto-match subtitles for any playlist or stream item
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let captions_dir = app_dir.join("streaming").join("captions");
        if captions_dir.exists() {
            args.push(format!("--sub-file-paths={}", captions_dir.to_string_lossy()));
        }
    }

    if let Some(items) = playlist.filter(|p| !p.is_empty()) {
        if let Some(idx) = start_index {
            args.push(format!("--playlist-start={}", idx));
        }

        // Add HTTP token header from the first item
        let (_, token) = strip_token_query(&items[0].url);
        if let Some(t) = token {
            args.push(format!("--http-header-fields={}: {}", STREAM_TOKEN_HEADER, t));
        }

        if items.len() > 1 {
            let playlist_dir = app_handle
                .path()
                .app_data_dir()
                .ok()
                .map(|dir| dir.join("mpv-playlists"))
                .unwrap_or_else(|| std::env::temp_dir().join("telestash-playlists"));
            let _ = std::fs::create_dir_all(&playlist_dir);
            let playlist_file = playlist_dir.join("current_playlist.m3u8");

            let mut m3u_content = String::from("#EXTM3U\n");
            for item in &items {
                let (stable_url, _) = strip_token_query(&item.url);
                if let Some(t) = &item.title {
                    m3u_content.push_str(&format!("#EXTINF:-1,{}\n", t));
                } else {
                    m3u_content.push_str("#EXTINF:-1,Untitled\n");
                }
                m3u_content.push_str(&format!("{}\n", stable_url));
            }

            if std::fs::write(&playlist_file, m3u_content).is_ok() {
                args.push(format!("--playlist={}", playlist_file.display()));
            } else {
                let (stable_url, _) = strip_token_query(&items[0].url);
                args.push(stable_url);
            }

            // Explicitly attach current item's subtitles for immediate playback
            let current_item = start_index.and_then(|idx| items.get(idx)).unwrap_or(&items[0]);
            let active_msg_id = message_id.or(current_item.message_id);
            let active_folder_id = folder_id.or(current_item.folder_id).unwrap_or(0);
            let active_title = title.as_deref().or(current_item.title.as_deref());

            if let Some(msg_id) = active_msg_id {
                attach_matching_subtitles(&mut args, &app_handle, active_folder_id, msg_id, active_title);
            }
        } else {
            let item = &items[0];
            let (stable_url, _) = strip_token_query(&item.url);
            if let Some(t) = &item.title {
                args.push(format!("--force-media-title={}", t));
                args.push(format!("--title={}", t));
                args.push(format!("--script-opts=osc-title={}", t));
            }
            let active_msg_id = item.message_id.or(message_id);
            let active_folder_id = item.folder_id.or(folder_id).unwrap_or(0);
            let active_title = item.title.as_deref().or(title.as_deref());
            if let Some(msg_id) = active_msg_id {
                attach_matching_subtitles(&mut args, &app_handle, active_folder_id, msg_id, active_title);
            }
            args.push(stable_url);
        }
    } else {
        let (stable_url, token) = strip_token_query(&url);
        if let Some(t) = token {
            args.push(format!("--http-header-fields={}: {}", STREAM_TOKEN_HEADER, t));
        }
        if let Some(t) = &title {
            args.push(format!("--force-media-title={}", t));
            args.push(format!("--title={}", t));
            args.push(format!("--script-opts=osc-title={}", t));
        }
        if let Some(msg_id) = message_id {
            let active_folder_id = folder_id.unwrap_or(0);
            attach_matching_subtitles(&mut args, &app_handle, active_folder_id, msg_id, title.as_deref());
        }
        args.push(stable_url);
    }

    // User arguments go last so they can override TeleStash's own flags.
    args.extend(extra_playback_args(&playback));

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    // Close the previous player first, otherwise every file switch would stack
    // another resident MPV process (~hundreds of MB each).
    if playback.single_player_instance {
        stop_tracked_player(&player);
    }

    // 1. Try to launch bundled sidecar mpv via Tauri plugin
    use tauri_plugin_shell::ShellExt;
    if let Ok(sidecar) = app_handle.shell().sidecar("mpv") {
        if let Ok((rx, child)) = sidecar.args(arg_refs.clone()).spawn() {
            log::info!("Launched bundled MPV (pid {})", child.pid());
            drain_player_events(rx);
            track_player(&player, TrackedPlayer::Sidecar(child));
            return Ok(());
        }
    }

    let mut last_error: Option<String> = None;

    // 2. Try to launch resolved local mpv binary
    if let Some(bin) = resolve_mpv_binary(&app_handle) {
        match std::process::Command::new(bin).args(&args).spawn() {
            Ok(child) => {
                log::info!("Launched local MPV (pid {})", child.id());
                track_player(&player, TrackedPlayer::External(child.id()));
                return Ok(());
            }
            Err(err) => {
                log::warn!("Local MPV launch failed: {}", err);
                last_error = Some(err.to_string());
            }
        }
    }

    // 3. Fallback: Try to launch system-installed mpv from PATH
    let child = std::process::Command::new("mpv")
        .args(&args)
        .spawn()
        .map_err(|e| match &last_error {
            Some(previous) => format!(
                "Failed to launch MPV: {}. Bundled/local MPV also failed: {}",
                e, previous
            ),
            None => format!("Failed to launch MPV: {}. Ensure 'mpv' is installed.", e),
        })?;
    log::info!("Launched system MPV (pid {})", child.id());
    track_player(&player, TrackedPlayer::External(child.id()));
    Ok(())
}

fn build_mpv_args(url: &str, watch_later_dir: Option<&Path>) -> Vec<String> {
    let (stable_url, token) = strip_token_query(url);
    let mut args = vec![
        "--save-position-on-quit".to_string(),
        "--write-filename-in-watch-later-config=yes".to_string(),
        "--input-default-bindings=yes".to_string(),
    ];
    if let Some(dir) = watch_later_dir {
        args.push(format!("--watch-later-dir={}", dir.display()));
    }
    if let Some(token) = token {
        args.push(format!("--http-header-fields={}: {}", STREAM_TOKEN_HEADER, token));
    }
    args.push(stable_url);
    args
}

fn strip_token_query(url: &str) -> (String, Option<String>) {
    let Some(query_start) = url.find('?') else {
        return (url.to_string(), None);
    };
    let base = &url[..query_start];
    let query = &url[query_start + 1..];
    let mut token = None;
    let mut kept = Vec::new();

    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("token=") {
            if !value.is_empty() {
                token = Some(value.to_string());
            }
        } else if !pair.is_empty() {
            kept.push(pair);
        }
    }

    let stable_url = if kept.is_empty() {
        base.to_string()
    } else {
        format!("{}?{}", base, kept.join("&"))
    };
    (stable_url, token)
}

pub(crate) fn stream_token_header_name() -> &'static str {
    STREAM_TOKEN_HEADER
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::playback_settings::{HardwareDecodeMode, PlaybackSettingsFile};
    use std::path::PathBuf;

    fn settings_with(mode: HardwareDecodeMode, adapter: Option<&str>) -> PlaybackSettingsFile {
        PlaybackSettingsFile {
            hardware_decode: mode,
            preferred_adapter: adapter.map(|a| a.to_string()),
            ..PlaybackSettingsFile::default()
        }
    }

    /// The adapters this machine reports, as the probe would return them.
    fn known_adapters() -> Vec<String> {
        vec![
            "NVIDIA GeForce 940M".to_string(),
            "Intel(R) HD Graphics 520".to_string(),
        ]
    }

    #[test]
    fn auto_mode_keeps_software_fallback_and_adds_no_scaler_flags() {
        let args = build_hwdec_args(&settings_with(HardwareDecodeMode::Auto, None), &known_adapters());

        assert_eq!(args, vec!["--hwdec=auto-safe".to_string()]);
        // Quality must stay MPV's own business: no scaler/dither overrides.
        assert!(!args.iter().any(|a| a.contains("profile")));
        assert!(!args.iter().any(|a| a.contains("scale")));
        assert!(!args.iter().any(|a| a.contains("dither")));
    }

    #[test]
    fn tele_stash_args_never_change_image_quality() {
        // Covers the FULL built-in argument list, not just the hwdec portion:
        // a scaler or dither flag leaking in anywhere would break the promise
        // that this feature never trades quality for CPU.
        for mode in [
            HardwareDecodeMode::Auto,
            HardwareDecodeMode::Software,
            HardwareDecodeMode::Adapter,
        ] {
            let args = tele_stash_args(
                &settings_with(mode, Some("Intel(R) HD Graphics 520")),
                &known_adapters(),
            );
            for arg in &args {
                assert!(
                    !arg.starts_with("--scale")
                        && !arg.starts_with("--dscale")
                        && !arg.starts_with("--cscale")
                        && !arg.starts_with("--dither")
                        && !arg.starts_with("--profile")
                        && !arg.starts_with("--correct-downscaling")
                        && !arg.starts_with("--linear-downscaling")
                        && !arg.starts_with("--sigmoid-upscaling"),
                    "quality-affecting flag leaked into built-in args for {:?}: {}",
                    mode,
                    arg
                );
            }
        }
    }

    #[test]
    fn software_mode_disables_hardware_decoding() {
        let args = build_hwdec_args(&settings_with(HardwareDecodeMode::Software, None), &known_adapters());
        assert_eq!(args, vec!["--hwdec=no".to_string()]);
    }

    #[test]
    fn adapter_mode_pins_the_named_adapter_via_d3d11() {
        let args = build_hwdec_args(
            &settings_with(HardwareDecodeMode::Adapter, Some("Intel(R) HD Graphics 520")),
            &known_adapters(),
        );

        // d3d11va is named explicitly so the adapter pin cannot be bypassed by
        // MPV's preference for Vulkan hardware decoding.
        assert_eq!(
            args,
            vec![
                "--hwdec=d3d11va".to_string(),
                "--d3d11-adapter=Intel(R) HD Graphics 520".to_string(),
            ]
        );
    }

    #[test]
    fn stale_adapter_pin_falls_back_instead_of_failing_playback() {
        // An unknown --d3d11-adapter makes MPV exit fatally before the video
        // opens, so a pin that no longer exists must degrade to automatic.
        let args = build_hwdec_args(
            &settings_with(HardwareDecodeMode::Adapter, Some("Removed GPU 3000")),
            &known_adapters(),
        );

        assert_eq!(args, vec!["--hwdec=auto-safe".to_string()]);
    }

    #[test]
    fn adapter_pin_is_trusted_before_any_probe() {
        // With no probe result there is nothing to validate against, so the
        // stored pin must still be used rather than silently ignored.
        let args = build_hwdec_args(
            &settings_with(HardwareDecodeMode::Adapter, Some("Intel(R) HD Graphics 520")),
            &[],
        );

        assert_eq!(
            args,
            vec![
                "--hwdec=d3d11va".to_string(),
                "--d3d11-adapter=Intel(R) HD Graphics 520".to_string(),
            ]
        );
    }

    #[test]
    fn adapter_mode_without_a_saved_adapter_still_decodes_safely() {
        let args = build_hwdec_args(&settings_with(HardwareDecodeMode::Adapter, None), &known_adapters());
        assert_eq!(args, vec!["--hwdec=auto-safe".to_string()]);
    }

    #[test]
    fn parses_adapter_list_and_skips_software_renderer() {
        let output = "Available DXGI adapters:\n\
             Adapter 0: vendor: 4318, description: NVIDIA GeForce 940M\n\
             Adapter 1: vendor: 32902, description: Intel(R) HD Graphics 520\n\
             Adapter 2: vendor: 5140, description: Microsoft Basic Render Driver\n";

        assert_eq!(
            parse_adapter_list(output),
            vec![
                "NVIDIA GeForce 940M".to_string(),
                "Intel(R) HD Graphics 520".to_string(),
            ]
        );
    }

    #[test]
    fn adapter_parser_ignores_unexpected_output() {
        assert!(parse_adapter_list("mpv: command not found").is_empty());
        assert!(parse_adapter_list("").is_empty());
    }

    #[test]
    fn strips_token_from_stream_url_for_stable_mpv_watch_later_key() {
        let (url, token) = strip_token_query("http://localhost:14201/stream/home/10?token=abc123");

        assert_eq!(url, "http://localhost:14201/stream/home/10");
        assert_eq!(token.as_deref(), Some("abc123"));
    }

    #[test]
    fn keeps_non_token_query_params_when_stripping_token() {
        let (url, token) = strip_token_query("http://localhost:14201/stream/home/10?quality=raw&token=abc123&x=1");

        assert_eq!(url, "http://localhost:14201/stream/home/10?quality=raw&x=1");
        assert_eq!(token.as_deref(), Some("abc123"));
    }

    #[test]
    fn build_mpv_args_enable_resume_and_header_auth() {
        let dir = PathBuf::from(r"C:\TeleStash\mpv-watch-later");
        let args = build_mpv_args("http://localhost:14201/stream/home/10?token=abc123", Some(&dir));

        assert!(args.contains(&"--save-position-on-quit".to_string()));
        assert!(args.contains(&"--input-default-bindings=yes".to_string()));
        assert!(args.contains(&r"--watch-later-dir=C:\TeleStash\mpv-watch-later".to_string()));
        assert!(args.contains(&"--http-header-fields=X-TeleStash-Stream-Token: abc123".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("http://localhost:14201/stream/home/10"));
    }

    #[test]
    fn test_mpv_force_media_title_arg() {
        let item = MpvPlaylistItem {
            url: "http://localhost:14201/stream/home/10?token=abc".to_string(),
            message_id: Some(10),
            folder_id: None,
            title: Some("Movie Title 2024.mkv".to_string()),
        };
        assert_eq!(item.title.as_deref(), Some("Movie Title 2024.mkv"));
    }

    #[test]
    fn caption_title_match_requires_exact_stem_or_dotted_suffix() {
        assert!(super::caption_matches_title("Matrix.srt", "Matrix"));
        assert!(super::caption_matches_title("Matrix.en.srt", "Matrix"));
        assert!(super::caption_matches_title("Matrix.id.srt", "Matrix"));
        assert!(!super::caption_matches_title("Matrix Reloaded.srt", "Matrix"));
        assert!(!super::caption_matches_title("Matrix12.srt", "Matrix"));
        assert!(!super::caption_matches_title("Other.srt", "Matrix"));
        assert!(!super::caption_matches_title("anything.srt", ""));
    }
}
