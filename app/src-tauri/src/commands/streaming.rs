use std::path::Path;
use tauri::{Manager, State};

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

fn resolve_mpv_binary(app_handle: &tauri::AppHandle) -> Option<std::path::PathBuf> {
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

#[tauri::command]
pub fn cmd_play_in_mpv(
    url: String,
    message_id: Option<i32>,
    folder_id: Option<i64>,
    title: Option<String>,
    playlist: Option<Vec<MpvPlaylistItem>>,
    start_index: Option<usize>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let watch_later_dir = app_handle
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("mpv-watch-later"));
    if let Some(dir) = &watch_later_dir {
        let _ = std::fs::create_dir_all(dir);
    }

    let mut args = vec![
        "--save-position-on-quit".to_string(),
        "--write-filename-in-watch-later-config=yes".to_string(),
        "--keep-open=no".to_string(),
        "--input-default-bindings=yes".to_string(),
    ];

    if let Some(dir) = &watch_later_dir {
        args.push(format!("--watch-later-dir={}", dir.display()));
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
            for item in items {
                let (stable_url, _) = strip_token_query(&item.url);
                args.push("--{".to_string());
                if let Some(t) = &item.title {
                    args.push(format!("--force-media-title={}", t));
                    args.push(format!("--title={}", t));
                    args.push(format!("--script-opts=osc-title={}", t));
                }
                if let (Some(msg_id), Some(f_id)) = (item.message_id, item.folder_id) {
                    if let Ok(app_dir) = app_handle.path().app_data_dir() {
                        let srt_path = app_dir.join("streaming").join("captions").join(format!("{}_{}.en.srt", f_id, msg_id));
                        if srt_path.exists() {
                            args.push(format!("--sub-file={}", srt_path.to_string_lossy()));
                        }
                    }
                }
                args.push(stable_url);
                args.push("--}".to_string());
            }
        } else {
            let item = &items[0];
            let (stable_url, _) = strip_token_query(&item.url);
            if let Some(t) = &item.title {
                args.push(format!("--force-media-title={}", t));
                args.push(format!("--title={}", t));
                args.push(format!("--script-opts=osc-title={}", t));
            }
            if let (Some(msg_id), Some(f_id)) = (item.message_id, item.folder_id) {
                if let Ok(app_dir) = app_handle.path().app_data_dir() {
                    let srt_path = app_dir.join("streaming").join("captions").join(format!("{}_{}.en.srt", f_id, msg_id));
                    if srt_path.exists() {
                        args.push(format!("--sub-file={}", srt_path.to_string_lossy()));
                    }
                }
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
        if let (Some(msg_id), Some(f_id)) = (message_id, folder_id) {
            if let Ok(app_dir) = app_handle.path().app_data_dir() {
                let srt_path = app_dir.join("streaming").join("captions").join(format!("{}_{}.en.srt", f_id, msg_id));
                if srt_path.exists() {
                    args.push(format!("--sub-file={}", srt_path.to_string_lossy()));
                }
            }
        }
        args.push(stable_url);
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    // 1. Try to launch bundled sidecar mpv via Tauri plugin
    use tauri_plugin_shell::ShellExt;
    if let Ok(sidecar) = app_handle.shell().sidecar("mpv") {
        if sidecar.args(arg_refs.clone()).spawn().is_ok() {
            return Ok(());
        }
    }

    // 2. Try to launch resolved local mpv binary
    if let Some(bin) = resolve_mpv_binary(&app_handle) {
        if std::process::Command::new(bin).args(&args).spawn().is_ok() {
            return Ok(());
        }
    }

    // 3. Fallback: Try to launch system-installed mpv from PATH
    std::process::Command::new("mpv")
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to launch MPV: {}. Ensure 'mpv' is installed.", e))?;
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
    use std::path::PathBuf;

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
}
