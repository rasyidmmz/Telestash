//! IPC path confinement guards (R4 item #1).
//!
//! Every Tauri command that touches the local filesystem on behalf of the
//! webview must pass its user-supplied path through one of these guards
//! first. The frontend always obtains these paths from a native file dialog
//! (download target, subtitle folder) or from a backend-produced preview
//! file, so requiring absolute, normalized, non-system paths never blocks a
//! legitimate flow — it only stops a compromised webview from steering the
//! backend at `C:\Windows`, executables, or device-namespace paths.
//!
//! Design notes (Windows 11 only):
//! - Normalization is lexical (no filesystem access) so download targets
//!   that do not exist yet can still be validated.
//! - Where the target (or its parent) already exists, the path is
//!   canonicalized so symlink/junction escapes are caught too. Canonicalized
//!   Windows paths carry the `\\?\` verbatim prefix; comparisons strip it.
//! - System-directory comparison is case-insensitive and separator-aware.
//! - `\\.\` / `\\?\` device-namespace paths are rejected outright.
//! - Windows reserved device basenames (`NUL`, `CON`, …) are rejected.
//!
//! The helpers stay `pub(crate)` on purpose: they are called from
//! `fs::download`, `subtitles`, and `lib.rs`, and a glob re-export would
//! widen nothing but add noise.

use std::path::{Component, Path, PathBuf};

/// Extensions that `cmd_open_file_externally` may always open, wherever the
/// file lives. Media, images, PDF, plain text, and subtitle files only —
/// nothing the OS would execute.
const OPEN_ALLOWLIST: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "m4v", "ts", "mpg", "mpeg", "flv", "wmv",
    "mp3", "flac", "wav", "ogg", "oga", "m4a", "opus", "wma", "aac",
    "jpg", "jpeg", "png", "gif", "webp", "bmp",
    "pdf", "txt", "md", "srt", "ass", "ssa", "vtt", "sub", "idx",
];

/// Extensions that must never be opened externally, even for backend-produced
/// preview files. `.lnk`/`.url` resolve to their target, the rest are code.
const OPEN_BLOCKLIST: &[&str] = &[
    "exe", "msi", "msp", "bat", "cmd", "com", "scr", "pif", "ps1", "psm1",
    "vbs", "vbe", "js", "jse", "wsf", "wsh", "msc", "reg", "lnk", "url",
    "jar", "cpl", "hta", "dll", "sys", "ocx", "inf",
];

/// Basenames Windows reserves for devices (`NUL`, `CON`, …), with or without
/// an extension. `clock$`/`config$` are reserved only without extension.
fn is_reserved_basename(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    let stem = lower.split('.').next().unwrap_or("");
    match stem {
        "con" | "prn" | "aux" | "nul" => true,
        "clock$" | "config$" => !lower.contains('.'),
        _ => {
            let b = stem.as_bytes();
            b.len() == 4 && (stem.starts_with("com") || stem.starts_with("lpt"))
                && (b[3] == b'1' || b[3] == b'2' || b[3] == b'3' || b[3] == b'4'
                    || b[3] == b'5' || b[3] == b'6' || b[3] == b'7'
                    || b[3] == b'8' || b[3] == b'9')
        }
    }
}

/// Roots a webview-supplied path must never resolve into. Read from the
/// environment with hardcoded fallbacks in case the environment is scrubbed.
fn blocked_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for var in ["SystemRoot", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(dir) = std::env::var(var) {
            if !dir.is_empty() {
                roots.push(PathBuf::from(dir));
            }
        }
    }
    roots.push(PathBuf::from(r"C:\Windows"));
    roots.push(PathBuf::from(r"C:\Program Files"));
    roots.push(PathBuf::from(r"C:\Program Files (x86)"));
    roots
}

/// Resolve `.`/`..` lexically (no filesystem access) so not-yet-existing
/// download targets can be validated. Never escapes past the root: popping an
/// empty buffer or a root is a no-op.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(comp.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Canonical comparison key: backslashes, lowercase, verbatim `\\?\` prefix
/// stripped (`\\?\UNC\server\share` becomes `\\server\share`).
fn comparison_key(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('/', "\\").to_lowercase();
    if let Some(rest) = s.strip_prefix(r"\\?\unc\") {
        s = format!(r"\\{rest}");
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_string();
    }
    s
}

fn is_within(path: &Path, root: &Path) -> bool {
    let p = comparison_key(path);
    let r = comparison_key(root).trim_end_matches('\\').to_string();
    p == r || p.starts_with(&format!("{r}\\"))
}

fn reject_device_namespace(raw: &str) -> Result<(), String> {
    let lowered = raw.replace('/', "\\").to_lowercase();
    if lowered.starts_with(r"\\.\") || lowered.starts_with(r"\\?\") {
        return Err("Path uses a device namespace, which is not allowed".to_string());
    }
    Ok(())
}

fn check_common(raw: &str) -> Result<PathBuf, String> {
    reject_device_namespace(raw)?;
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err("Path must be absolute".to_string());
    }
    let normalized = normalize_lexical(path);
    if let Some(name) = normalized.file_name().and_then(|n| n.to_str()) {
        if is_reserved_basename(name) {
            return Err(format!("\"{name}\" is a reserved system name"));
        }
    }
    Ok(normalized)
}

fn reject_blocked_roots(canonical: &Path) -> Result<(), String> {
    for root in blocked_roots() {
        if is_within(canonical, &root) {
            return Err("Path points inside a protected system folder".to_string());
        }
    }
    Ok(())
}

/// Validate a download target from `cmd_download_file`. The file itself may
/// not exist yet, but its parent folder must already exist (the frontend
/// always takes it from a native save dialog). Returns the target with its
/// parent canonicalized so junction escapes are resolved before writing.
pub(crate) fn guard_download_path(raw: &str) -> Result<PathBuf, String> {
    let normalized = check_common(raw)?;
    let parent = normalized
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| "Path has no parent folder".to_string())?;
    if !parent.is_dir() {
        return Err("Download folder does not exist".to_string());
    }
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| "Download folder cannot be accessed".to_string())?;
    reject_blocked_roots(&canonical_parent)?;
    let file_name = normalized
        .file_name()
        .ok_or_else(|| "Path has no file name".to_string())?;
    Ok(canonical_parent.join(file_name))
}

/// Validate a directory listing request from `cmd_list_directory_files`. The
/// folder comes from a native folder picker (subtitle scan). Returns the
/// canonicalized directory.
pub(crate) fn guard_list_directory(raw: &str) -> Result<PathBuf, String> {
    let normalized = check_common(raw)?;
    if !normalized.is_dir() {
        return Err("Folder does not exist".to_string());
    }
    let canonical = normalized
        .canonicalize()
        .map_err(|_| "Folder cannot be accessed".to_string())?;
    if !canonical.is_dir() {
        return Err("Folder does not exist".to_string());
    }
    reject_blocked_roots(&canonical)?;
    Ok(canonical)
}

pub(crate) fn is_allowed_open_extension(ext: &str) -> bool {
    OPEN_ALLOWLIST.contains(&ext.to_lowercase().as_str())
}

pub(crate) fn is_blocked_open_extension(ext: &str) -> bool {
    OPEN_BLOCKLIST.contains(&ext.to_lowercase().as_str())
}

/// Validate a `cmd_open_file_externally` request. The file must exist.
/// Executables are always refused. Known media/document extensions are always
/// allowed. Any other extension is allowed only for backend-produced preview
/// files inside `app_cache_dir` (Telegram document previews keep their
/// original extension, e.g. `.bin`).
pub(crate) fn guard_open_externally(raw: &str, app_cache_dir: &Path) -> Result<(), String> {
    let normalized = check_common(raw)?;
    if !normalized.is_file() {
        return Err("File does not exist".to_string());
    }
    let canonical = normalized
        .canonicalize()
        .map_err(|_| "File cannot be accessed".to_string())?;
    reject_blocked_roots(&canonical)?;
    let ext = canonical
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if is_blocked_open_extension(&ext) {
        return Err(format!("Files of type \".{ext}\" cannot be opened externally"));
    }
    if is_allowed_open_extension(&ext) {
        return Ok(());
    }
    if !app_cache_dir.as_os_str().is_empty() {
        // Canonicalize the root too: it can arrive as a short (8.3) name or
        // through a junction, and comparing that against an already
        // canonicalized file path would silently disable this exception.
        let cache_root = app_cache_dir
            .canonicalize()
            .unwrap_or_else(|_| app_cache_dir.to_path_buf());
        if is_within(&canonical, &cache_root) {
            return Ok(());
        }
    }
    Err(format!(
        "Files of type \"{}\" cannot be opened externally",
        if ext.is_empty() { "(none)" } else { &ext }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_root() -> Option<PathBuf> {
        std::env::var("SystemRoot")
            .or_else(|_| std::env::var("windir"))
            .ok()
            .map(PathBuf::from)
    }

    #[test]
    fn relative_paths_are_rejected_everywhere() {
        assert!(guard_download_path("relative\\file.mp4").is_err());
        assert!(guard_list_directory("relative\\dir").is_err());
        assert!(guard_open_externally("relative\\file.mp4", Path::new("")).is_err());
    }

    #[test]
    fn parent_traversal_into_system_folder_is_rejected() {
        let Some(root) = system_root() else { return; };
        // Lexical normalization resolves the `..` back into the system root.
        let evil = root
            .join("telestash_guard_sub")
            .join("..")
            .join("evil.mp4")
            .to_string_lossy()
            .to_string();
        assert!(guard_download_path(&evil).is_err());
    }

    #[test]
    fn device_namespaces_are_rejected() {
        let tmp = std::env::temp_dir().join("telestash_guard_test.mp4");
        let verbatim = format!(r"\\?\{}", tmp.to_string_lossy());
        assert!(guard_download_path(&verbatim).is_err());
        let device = r"\\.\C:\Users\Public\file.mp4";
        assert!(guard_download_path(device).is_err());
    }

    #[test]
    fn reserved_device_basenames_are_rejected() {
        let tmp = std::env::temp_dir();
        for name in ["NUL", "con.txt", "COM1.mp4", "lpt9"] {
            let p = tmp.join(name).to_string_lossy().to_string();
            assert!(guard_download_path(&p).is_err(), "{name} should be rejected");
        }
    }

    #[test]
    fn download_into_existing_temp_dir_is_allowed() {
        let target = std::env::temp_dir()
            .join("telestash_guard_test.mp4")
            .to_string_lossy()
            .to_string();
        assert!(guard_download_path(&target).is_ok());
    }

    #[test]
    fn download_into_missing_folder_is_rejected() {
        let target = std::env::temp_dir()
            .join("telestash_no_such_dir_xyz")
            .join("file.mp4")
            .to_string_lossy()
            .to_string();
        assert!(guard_download_path(&target).is_err());
    }

    #[test]
    fn download_into_system_folder_is_rejected() {
        let Some(root) = system_root() else { return; };
        let target = root
            .join("telestash_guard_test.mp4")
            .to_string_lossy()
            .to_string();
        // Parent (the system root itself) exists, so this reaches the blocklist.
        assert!(guard_download_path(&target).is_err());
    }

    #[test]
    fn list_existing_temp_dir_is_allowed_and_missing_is_rejected() {
        let tmp = std::env::temp_dir().to_string_lossy().to_string();
        assert!(guard_list_directory(&tmp).is_ok());
        let missing = std::env::temp_dir()
            .join("telestash_no_such_dir_xyz")
            .to_string_lossy()
            .to_string();
        assert!(guard_list_directory(&missing).is_err());
    }

    #[test]
    fn list_system_folder_is_rejected() {
        let Some(root) = system_root() else { return; };
        let path = root.to_string_lossy().to_string();
        assert!(guard_list_directory(&path).is_err());
    }

    #[test]
    fn open_externally_allows_media_everywhere_but_never_executables() {
        let dir = std::env::temp_dir().join("telestash_guard_open");
        let _ = std::fs::create_dir_all(&dir);
        let media = dir.join("clip.mp4");
        let exe = dir.join("setup.exe");
        let _ = std::fs::write(&media, b"x");
        let _ = std::fs::write(&exe, b"x");
        let cache = Path::new("");
        assert!(guard_open_externally(&media.to_string_lossy(), cache).is_ok());
        assert!(guard_open_externally(&exe.to_string_lossy(), cache).is_err());
        // Executables stay blocked even inside the app cache dir.
        assert!(guard_open_externally(&exe.to_string_lossy(), &dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_externally_unknown_extension_only_inside_cache() {
        let dir = std::env::temp_dir().join("telestash_guard_cache");
        let _ = std::fs::create_dir_all(&dir);
        let odd = dir.join("preview.bin");
        let _ = std::fs::write(&odd, b"x");
        assert!(odd.is_file(), "test setup: preview file must exist");
        let raw = odd.to_string_lossy().to_string();
        // Unknown extension outside the cache dir: rejected.
        assert!(guard_open_externally(&raw, Path::new("")).is_err());
        // Same file treated as the cache dir itself: allowed.
        assert!(guard_open_externally(&raw, &dir).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_externally_missing_file_is_rejected() {
        let missing = std::env::temp_dir()
            .join("telestash_no_such_file_xyz.mp4")
            .to_string_lossy()
            .to_string();
        assert!(guard_open_externally(&missing, Path::new("")).is_err());
    }

    #[test]
    fn extension_lists_behave_case_insensitively() {
        assert!(is_allowed_open_extension("MP4"));
        assert!(!is_allowed_open_extension("exe"));
        assert!(is_blocked_open_extension("EXE"));
        assert!(is_blocked_open_extension("LNK"));
        assert!(!is_blocked_open_extension("mp4"));
    }
}
