//! Persisted playback (MPV) settings.
//!
//! Hardware decoding is hardware- and driver-specific, so TeleStash must not
//! hardcode a single acceleration strategy. On hybrid-graphics laptops the GPU
//! MPV picks by default is not always the one that can decode the file, which
//! is why `auto-safe` alone can silently fall back to CPU. This module stores
//! the user's choice in `playback_settings.json` next to the other app data
//! files, and the streaming command turns that choice into MPV arguments.
//!
//! None of the modes change image quality: they only decide how frames are
//! decoded (and, for `Adapter`, which GPU runs the decoder).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Hardware decoding strategy requested by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareDecodeMode {
    /// Let MPV probe its whitelisted methods, keeping the automatic software
    /// fallback. Never changes image quality.
    Auto,
    /// Decode on the CPU. Escape hatch for machines where acceleration misbehaves.
    Software,
    /// Force one specific D3D11 adapter (by exact name, as reported by MPV).
    Adapter,
}

impl Default for HardwareDecodeMode {
    fn default() -> Self {
        Self::Auto
    }
}

/// Persisted playback settings (written to playback_settings.json in the app data dir).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackSettingsFile {
    #[serde(default)]
    pub hardware_decode: HardwareDecodeMode,
    /// Exact DXGI adapter description to pin, used when `hardware_decode == Adapter`.
    #[serde(default)]
    pub preferred_adapter: Option<String>,
    /// Extra raw arguments appended to the MPV command line, for power users.
    /// Every argument must start with `-`; malformed entries are dropped.
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Close the previous MPV process before launching a new one.
    #[serde(default = "default_true")]
    pub single_player_instance: bool,
}

fn default_true() -> bool {
    true
}

impl Default for PlaybackSettingsFile {
    fn default() -> Self {
        Self {
            hardware_decode: HardwareDecodeMode::default(),
            preferred_adapter: None,
            extra_args: Vec::new(),
            single_player_instance: true,
        }
    }
}

/// What the frontend sees: persisted settings plus the adapters detected on this machine.
#[derive(Debug, Clone, Serialize)]
pub struct PlaybackSettingsResponse {
    pub hardware_decode: HardwareDecodeMode,
    pub preferred_adapter: Option<String>,
    pub extra_args: Vec<String>,
    pub single_player_instance: bool,
    /// DXGI adapter descriptions reported by MPV on this machine. Empty when
    /// detection failed, in which case the UI only offers Auto/Software.
    pub available_adapters: Vec<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("playback_settings.json"))
}

pub(crate) fn load_playback_settings(app: &AppHandle) -> PlaybackSettingsFile {
    let path = match settings_path(app) {
        Ok(p) => p,
        Err(_) => return PlaybackSettingsFile::default(),
    };
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => PlaybackSettingsFile::default(),
    }
}

fn save_playback_settings(app: &AppHandle, settings: &PlaybackSettingsFile) -> Result<(), String> {
    let path = settings_path(app)?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Keep only arguments that look like MPV flags.
///
/// Every legitimate MPV option starts with `-`, so this drops anything that
/// could be interpreted as a positional argument (a URL or a local file path)
/// coming from the settings file. A bare `-` or the `--` options terminator is
/// dropped too: neither carries an option.
///
/// Networking flags are refused as well. TeleStash connects to Telegram
/// directly, so proxying, VPN, and bandwidth-shaping options must not become
/// reachable through this box.
pub(crate) fn sanitize_extra_args(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| a.trim())
        .filter(|a| a.starts_with('-') && a.len() > 1 && *a != "--")
        .filter(|a| !is_network_flag(a))
        .map(|a| a.to_string())
        .take(32)
        .collect()
}

/// True for MPV options that route, tunnel, or reshape network traffic.
///
/// Matched by exact option name (the part before `=`), never by prefix, so a
/// filesystem option such as `--cache-dir` is not caught by `--cache`.
fn is_network_flag(arg: &str) -> bool {
    const BLOCKED: [&str; 8] = [
        "--http-proxy",
        "--proxy",
        "--network-timeout",
        "--cache",
        "--demuxer-max-bytes",
        "--demuxer-max-back-bytes",
        "--demuxer-readahead-secs",
        "--stream-buffer-size",
    ];
    let name = arg.split('=').next().unwrap_or(arg);
    BLOCKED.contains(&name)
}

/// Build the response the settings UI renders.
///
/// Side effect worth naming: this probes MPV for the machine's adapters the
/// first time it is called, which is why it takes the `AppHandle` rather than
/// being a pure conversion.
fn response_with_adapters(
    app: &AppHandle,
    settings: PlaybackSettingsFile,
) -> PlaybackSettingsResponse {
    PlaybackSettingsResponse {
        hardware_decode: settings.hardware_decode,
        preferred_adapter: settings.preferred_adapter,
        extra_args: settings.extra_args,
        single_player_instance: settings.single_player_instance,
        available_adapters: super::streaming::detect_d3d11_adapters(app),
    }
}

#[tauri::command]
pub async fn cmd_get_playback_settings(app: AppHandle) -> Result<PlaybackSettingsResponse, String> {
    Ok(response_with_adapters(&app, load_playback_settings(&app)))
}

#[tauri::command]
pub async fn cmd_update_playback_settings(
    hardware_decode: HardwareDecodeMode,
    preferred_adapter: Option<String>,
    extra_args: Vec<String>,
    single_player_instance: bool,
    app: AppHandle,
) -> Result<PlaybackSettingsResponse, String> {
    // An adapter pin only means something if the adapter actually exists:
    // MPV exits fatally on an unknown name, so reject it up front. A pin is
    // also never *required* here, because the UI can only offer the adapter
    // picker once Adapter mode is already selected, and because adapter
    // enumeration itself can fail on some machines. Playback falls back to
    // automatic decoding while no valid pin is stored.
    let pinned = if hardware_decode == HardwareDecodeMode::Adapter {
        let requested = preferred_adapter.unwrap_or_default().trim().to_string();
        if requested.is_empty() {
            None
        } else {
            let adapters = super::streaming::detect_d3d11_adapters(&app);
            // An unenumerable adapter list must not make the mode unusable.
            if !adapters.is_empty() && !adapters.iter().any(|a| a == &requested) {
                return Err(format!(
                    "Adapter '{}' is not available on this machine",
                    requested
                ));
            }
            Some(requested)
        }
    } else {
        // Drop a stale pin when the user leaves Adapter mode.
        None
    };

    let settings = PlaybackSettingsFile {
        hardware_decode,
        preferred_adapter: pinned,
        extra_args: sanitize_extra_args(&extra_args),
        single_player_instance,
    };
    save_playback_settings(&app, &settings)?;

    Ok(response_with_adapters(&app, settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_args_keep_flags_and_drop_paths() {
        let input = vec![
            "--profile=fast".to_string(),
            "  --no-osc  ".to_string(),
            "C:\\Users\\me\\movie.mkv".to_string(),
            "https://example.com/video.mp4".to_string(),
            "-".to_string(),
            "--".to_string(),
        ];
        let cleaned = sanitize_extra_args(&input);

        assert_eq!(
            cleaned,
            vec!["--profile=fast".to_string(), "--no-osc".to_string()]
        );
    }

    #[test]
    fn extra_args_drop_the_options_terminator() {
        // `--` would end option parsing and turn the next entry into a
        // positional argument (i.e. a file to play).
        let cleaned = sanitize_extra_args(&[
            "--".to_string(),
            "--no-osc".to_string(),
            "-".to_string(),
        ]);
        assert_eq!(cleaned, vec!["--no-osc".to_string()]);
    }

    #[test]
    fn extra_args_reject_networking_flags() {
        // TeleStash connects to Telegram directly, so proxy/VPN and bandwidth
        // shaping options must not be reachable from this box.
        let cleaned = sanitize_extra_args(&[
            "--http-proxy=http://127.0.0.1:8080".to_string(),
            "--proxy=foo".to_string(),
            "--cache=yes".to_string(),
            "--demuxer-max-bytes=500MiB".to_string(),
            "--stream-buffer-size=1MiB".to_string(),
            "--no-osc".to_string(),
        ]);
        assert_eq!(cleaned, vec!["--no-osc".to_string()]);
    }

    #[test]
    fn extra_args_do_not_over_block_similar_names() {
        // Guard against a prefix match that is too eager: these are unrelated
        // to networking and must survive.
        let cleaned = sanitize_extra_args(&[
            "--cache-dir-helper".to_string(),
            "--no-osc".to_string(),
        ]);
        assert_eq!(cleaned.len(), 2);
    }

    #[test]
    fn extra_args_are_capped() {
        let input: Vec<String> = (0..100).map(|i| format!("--opt{}", i)).collect();
        assert_eq!(sanitize_extra_args(&input).len(), 32);
    }

    #[test]
    fn defaults_to_auto_without_adapter_pin() {
        let defaults = PlaybackSettingsFile::default();
        assert_eq!(defaults.hardware_decode, HardwareDecodeMode::Auto);
        assert!(defaults.preferred_adapter.is_none());
        assert!(defaults.single_player_instance);
    }

    #[test]
    fn hardware_decode_mode_round_trips_as_snake_case() {
        let json = serde_json::to_string(&HardwareDecodeMode::Adapter).unwrap();
        assert_eq!(json, "\"adapter\"");
        let parsed: HardwareDecodeMode = serde_json::from_str("\"software\"").unwrap();
        assert_eq!(parsed, HardwareDecodeMode::Software);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // Forward/backward compatibility: an older file without the new keys
        // must still load instead of resetting the whole struct.
        let parsed: PlaybackSettingsFile = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.hardware_decode, HardwareDecodeMode::Auto);
        assert!(parsed.single_player_instance);
        assert!(parsed.extra_args.is_empty());
    }
}
