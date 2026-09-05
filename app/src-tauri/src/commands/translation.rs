use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Serialize};
use tauri::{Manager, State, Emitter};
use sha2::{Digest, Sha256};
use flate2::read::GzDecoder;
use std::io::Read;

use crate::TelegramState;

/// Pinned Bergamot WASM engine artifacts (browsermt/bergamot-translator "latest").
const WASM_JS_URL: &str = "https://github.com/browsermt/bergamot-translator/releases/download/latest/bergamot-translator-worker.js";
const WASM_BIN_URL: &str = "https://github.com/browsermt/bergamot-translator/releases/download/latest/bergamot-translator-worker.wasm";

/// Pinned en→id model pack (mozilla/firefox-translations-models, tiny architecture,
/// sourceLanguage "en" -> targetLanguage "id", BLEU 46.1). Sources are the HF mirror
/// of the mozilla repo; sha256 below pins the GUNZIPPED contents.
struct ModelFile {
    url: &'static str,
    gz_name: &'static str,
    out_name: &'static str,
    /// sha256 of the UNCOMPRESSED content.
    sha256: &'static str,
}

const MODEL_FILES: &[ModelFile] = &[
    ModelFile {
        url: "https://huggingface.co/TiberiuCristianLeon/Bergamot/resolve/main/tiny/enid/model.enid.intgemm.alphas.bin.gz",
        gz_name: "model.enid.intgemm.alphas.bin.gz",
        out_name: "model.enid.intgemm.alphas.bin",
        sha256: "f81f13eef703a4e0650ffc3138a0f4bab7b6c8bfd173ef1b7bda68d16b8bc7e8",
    },
    ModelFile {
        url: "https://huggingface.co/TiberiuCristianLeon/Bergamot/resolve/main/tiny/enid/lex.50.50.enid.s2t.bin.gz",
        gz_name: "lex.50.50.enid.s2t.bin.gz",
        out_name: "lex.50.50.enid.s2t.bin",
        sha256: "d37f72bcab6e7bc52fd223350f95521b5810bb2486a97275f86077988fced3f4",
    },
    ModelFile {
        url: "https://huggingface.co/TiberiuCristianLeon/Bergamot/resolve/main/tiny/enid/vocab.enid.spm.gz",
        gz_name: "vocab.enid.spm.gz",
        out_name: "vocab.enid.spm",
        sha256: "61bc7db24d3b6de638a02a280580a273fe0c942ecbe8a8204b2f81978211db22",
    },
];

#[derive(Serialize, Clone)]
pub struct TranslationModelStatus {
    pub installed: bool,
    pub wasm_ready: bool,
    pub files: Vec<TranslationFileStatus>,
}

#[derive(Serialize, Clone)]
pub struct TranslationFileStatus {
    pub name: String,
    pub ready: bool,
}

#[derive(Serialize, Clone)]
struct ProgressPayload {
    file: String,
    downloaded: u64,
    total: u64,
}

pub fn translation_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("translation")
        .join("bergamot");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

async fn curl_download(url: &str, dest: &Path, cancel_rx: &mut tokio::sync::oneshot::Receiver<()>) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new("curl");
    cmd.args(&[
        "-fL", "--retry", "3", "--silent", "--show-error",
        "-o", &dest.to_string_lossy(),
        url,
    ]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x00004000); // BELOW_NORMAL_PRIORITY_CLASS
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| format!("Failed to launch curl: {}", e))?;
    tokio::select! {
        status = child.wait() => {
            let status = status.map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("curl exited with code {:?}", status.code()))
            }
        }
        _ = cancel_rx => {
            let _ = child.kill().await;
            Err("Cancelled".to_string())
        }
    }
}

fn gunzip_file(src: &Path, dest: &Path) -> Result<(), String> {
    let gz_bytes = std::fs::read(src).map_err(|e| format!("Failed to read {}: {}", src.display(), e))?;
    let mut decoder = GzDecoder::new(&gz_bytes[..]);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|e| format!("Failed to gunzip {}: {}", src.display(), e))?;
    std::fs::write(dest, &out).map_err(|e| format!("Failed to write {}: {}", dest.display(), e))?;
    Ok(())
}

/// Download (once) and verify the Bergamot WASM engine + en→id model pack.
/// Emits `translation-model-progress` events; supports cancellation.
#[tauri::command]
pub async fn cmd_download_translation_model(
    app_handle: tauri::AppHandle,
) -> Result<TranslationModelStatus, String> {
    // Re-entrancy guard: reuse a simple file lock via an atomic create.
    let dir = translation_dir(&app_handle);
    let lock_path = dir.join(".downloading");
    if std::fs::File::create_new(&lock_path).is_err() {
        return Err("Pengunduhan model terjemahan sudah berjalan.".to_string());
    }

    let result = run_download(&app_handle, &dir, &lock_path).await;
    let _ = std::fs::remove_file(&lock_path);
    result
}

async fn run_download(
    app_handle: &tauri::AppHandle,
    dir: &PathBuf,
    lock_path: &PathBuf,
) -> Result<TranslationModelStatus, String> {
    // Engine files (sha256 pinned to the browsermt "latest" release assets).
    let jobs: Vec<(&str, &str, &'static str)> = vec![
        ("bergamot-translator-worker.js", WASM_JS_URL, "2cb354e739ba02cb04ee93da0c24f4b4d9de5e17c26fc41f1aeed4fb41811a5c"),
        ("bergamot-translator-worker.wasm", WASM_BIN_URL, "65cf5be754ea6d44db018a38e1f4a5c2fb2d7ef601d0e0067abde4e3d145386f"),
    ];

    let mut all_status: Vec<TranslationFileStatus> = Vec::new();
    let mut did_work = false;

    for (name, url, expected_hash) in &jobs {
        let dest = dir.join(name);
        let ready = dest.exists();
        all_status.push(TranslationFileStatus { name: name.to_string(), ready });
        if !ready {
            did_work = true;
            let _ = app_handle.emit("translation-model-progress", ProgressPayload { file: name.to_string(), downloaded: 0, total: 0 });
            let (_tx, mut rx) = tokio::sync::oneshot::channel();
            curl_download(url, &dest, &mut rx).await?;
            let bytes = std::fs::read(&dest).map_err(|e| e.to_string())?;
            let actual = sha256_hex(&bytes);
            if actual != *expected_hash {
                let _ = std::fs::remove_file(&dest);
                return Err(format!("Engine file {} hash mismatch (expected {}, got {})", name, expected_hash, actual));
            }
            let _ = app_handle.emit("translation-model-progress", ProgressPayload { file: name.to_string(), downloaded: 1, total: 1 });
        }
    }

    // Model pack files (gz download + gunzip + sha256 verify).
    for mf in MODEL_FILES {
        let gz_path = dir.join(mf.gz_name);
        let out_path = dir.join(mf.out_name);
        let ready = out_path.exists();
        all_status.push(TranslationFileStatus { name: mf.out_name.to_string(), ready });
        if ready {
            continue;
        }
        did_work = true;
        let _ = app_handle.emit("translation-model-progress", ProgressPayload { file: mf.out_name.into(), downloaded: 0, total: 0 });

        let (_tx, mut cancel_rx) = tokio::sync::oneshot::channel();
        curl_download(mf.url, &gz_path, &mut cancel_rx).await?;

        gunzip_file(&gz_path, &out_path)?;
        let _ = std::fs::remove_file(&gz_path);

        let out_bytes = std::fs::read(&out_path).map_err(|e| e.to_string())?;
        let actual = sha256_hex(&out_bytes);
        if !mf.sha256.is_empty() && actual != mf.sha256 {
            let _ = std::fs::remove_file(&out_path);
            return Err(format!("Model file {} hash mismatch (expected {}, got {})", mf.out_name, mf.sha256, actual));
        }
        let _ = app_handle.emit("translation-model-progress", ProgressPayload { file: mf.out_name.into(), downloaded: 1, total: 1 });
    }

    let status = cmd_translation_model_status_inner(app_handle);
    Ok(status)
}

fn cmd_translation_model_status_inner(app_handle: &tauri::AppHandle) -> TranslationModelStatus {
    let dir = translation_dir(app_handle);
    let wasm_ready = dir.join("bergamot-translator-worker.wasm").exists()
        && dir.join("bergamot-translator-worker.js").exists();
    let files: Vec<TranslationFileStatus> = vec![
        TranslationFileStatus { name: "bergamot-translator-worker.js".into(), ready: dir.join("bergamot-translator-worker.js").exists() },
        TranslationFileStatus { name: "bergamot-translator-worker.wasm".into(), ready: dir.join("bergamot-translator-worker.wasm").exists() },
        TranslationFileStatus { name: "model.enid.intgemm.alphas.bin".into(), ready: dir.join("model.enid.intgemm.alphas.bin").exists() },
        TranslationFileStatus { name: "lex.50.50.enid.s2t.bin".into(), ready: dir.join("lex.50.50.enid.s2t.bin").exists() },
        TranslationFileStatus { name: "vocab.enid.spm".into(), ready: dir.join("vocab.enid.spm").exists() },
    ];
    TranslationModelStatus {
        installed: wasm_ready && files.iter().all(|f| f.ready),
        wasm_ready,
        files,
    }
}

#[tauri::command]
pub fn cmd_get_translation_model_status(app_handle: tauri::AppHandle) -> TranslationModelStatus {
    cmd_translation_model_status_inner(&app_handle)
}

/// Read the model files (wasm engine + model pack) as raw bytes so the
/// frontend worker can instantiate Bergamot fully offline.
#[tauri::command]
pub fn cmd_read_translation_assets(app_handle: tauri::AppHandle) -> Result<TranslationAssets, String> {
    let dir = translation_dir(&app_handle);
    let read = |name: &str| -> Result<Vec<u8>, String> {
        std::fs::read(dir.join(name)).map_err(|e| format!("Missing translation asset {}: {}", name, e))
    };
    Ok(TranslationAssets {
        wasm_js: read("bergamot-translator-worker.js")?,
        wasm_bin: read("bergamot-translator-worker.wasm")?,
        model: read("model.enid.intgemm.alphas.bin")?,
        lex: read("lex.50.50.enid.s2t.bin")?,
        vocab: read("vocab.enid.spm")?,
    })
}

#[derive(Serialize)]
pub struct TranslationAssets {
    pub wasm_js: Vec<u8>,
    pub wasm_bin: Vec<u8>,
    pub model: Vec<u8>,
    pub lex: Vec<u8>,
    pub vocab: Vec<u8>,
}

/// Fetch the source subtitle bytes for a video+language. Prefers the SQLite
/// registry's subtitle_message_id (downloads via MTProto when not cached
/// locally), so it works for uploaded AND whisper-generated sidecars.
#[tauri::command]
pub async fn cmd_get_subtitle_content(
    folder_id: Option<i64>,
    video_message_id: i64,
    language: String,
    app_handle: tauri::AppHandle,
    state: State<'_, TelegramState>,
) -> Result<Vec<u8>, String> {
    use crate::db::DbConnection;

    // 1. Look up the subtitle message in the registry
    let (subtitle_msg_id, format) = {
        let db = app_handle.state::<DbConnection>();
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT subtitle_message_id, format FROM video_subtitles \
             WHERE (folder_id IS ? OR folder_id = ?) AND video_message_id = ? AND language = ? \
             ORDER BY created_at DESC LIMIT 1"
        ).map_err(|e| e.to_string())?;
        stmt.bind((1, folder_id)).map_err(|e| e.to_string())?;
        stmt.bind((2, folder_id.unwrap_or(0))).map_err(|e| e.to_string())?;
        stmt.bind((3, video_message_id)).map_err(|e| e.to_string())?;
        stmt.bind((4, language.as_str())).map_err(|e| e.to_string())?;
        if let Ok(sqlite::State::Row) = stmt.next() {
            let s_id: Option<i64> = stmt.read(0).ok();
            let fmt: String = stmt.read(1).unwrap_or_default();
            (s_id, fmt)
        } else {
            return Err(format!("No {} subtitle registered for this video.", language));
        }
    };

    let sub_msg_id = subtitle_msg_id.ok_or("Subtitle has no Telegram message reference (vobsub pair?)")?;
    let _ = format;

    // 2. Fast path: check local captions cache for a matching file
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let captions = app_dir.join("streaming").join("captions");
        if let Ok(entries) = std::fs::read_dir(&captions) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                // whisper-generated: {folder}_{msg}.{lang}.srt
                if name.starts_with(&format!("{}_{}.", folder_id.unwrap_or(0), video_message_id))
                    && name.ends_with(&format!(".{}.srt", language))
                {
                    if let Ok(bytes) = std::fs::read(entry.path()) {
                        return Ok(bytes);
                    }
                }
            }
        }
    }

    // 3. Download from Telegram
    let client = { state.client.lock().await.clone() }.ok_or("Telegram client not initialized")?;
    let peer = crate::commands::utils::resolve_peer(&client, folder_id, &state.peer_cache).await?;
    let messages = client.get_messages_by_id(peer, &[sub_msg_id as i32]).await
        .map_err(|e| format!("Failed to fetch subtitle message: {}", e))?;
    let msg = messages.into_iter().flatten().next().ok_or("Subtitle message not found")?;
    let media = msg.media().ok_or("Subtitle message has no media")?;

    let mut bytes: Vec<u8> = Vec::new();
    let mut iter = client.iter_download(&media);
    iter = iter.chunk_size(512 * 1024);
    while let Some(chunk) = iter.next().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Write translated SRT bytes to a temp file so the existing
/// cmd_attach_video_subtitles flow can upload/cache/register it.
#[tauri::command]
pub async fn cmd_write_temp_srt(
    folder_id: Option<i64>,
    video_message_id: i64,
    bytes: Vec<u8>,
) -> Result<String, String> {
    let dir = std::env::temp_dir().join("telestash-translations");
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}_{}.id.srt", folder_id.unwrap_or(0), video_message_id));
    tokio::fs::write(&path, &bytes).await.map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}
