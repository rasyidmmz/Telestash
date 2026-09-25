//! Folder file listing: the SQLite folder cache, cache-first reads, delta sync,
//! and the two search paths (local cache + Telegram-wide).
//!
//! Reopening a folder serves the cached list immediately; `cmd_sync_folder`
//! reconciles it with Telegram in the background.

use std::collections::HashSet;

use grammers_client::media::Media;
use grammers_tl_types as tl;
use sqlite;
use tauri::State;

use crate::commands::utils::{map_error, resolve_peer};
use crate::db::DbConnection;
use crate::models::{
    is_split_part_caption, FileMetadata, SPLIT_MANIFEST_SUFFIX, SPLIT_MANIFEST_UPLOAD_NAME,
};
use crate::TelegramState;

use super::split_manifest_from_media;

/// Pull a folder's full file list straight from Telegram. Shared by the
/// cache-first read and the background delta sync.
pub(crate) async fn fetch_files_from_telegram(
    folder_id: Option<i64>,
    state: &TelegramState,
) -> Result<Vec<FileMetadata>, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        log::info!("[MOCK] Returning mock files for folder {:?}", folder_id);
        return Ok(Vec::new()); // No mock files for now
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
    let mut files = Vec::new();

    let peer = resolve_peer(&client, folder_id, &state.peer_cache).await?;

    let mut msgs = client.iter_messages(peer);
    while let Some(msg) = msgs.next().await.map_err(|e| e.to_string())? {
        if let Some(doc) = msg.media() {
            let caption = msg.text();
            if crate::models::is_split_part_caption(caption) {
                continue;
            }
            // Parts can arrive without captions (e.g. restored messages); also
            // reject the .tdpart####of#### document filename pattern.
            if let Media::Document(d) = &doc {
                let doc_name = d.name().unwrap_or_default();
                if crate::models::is_split_part_filename(doc_name) {
                    continue;
                }
            }
            if let Some(manifest) = split_manifest_from_media(&client, &doc, caption).await {
                files.push(FileMetadata {
                    id: msg.id() as i64,
                    folder_id,
                    name: manifest.filename.clone(),
                    size: manifest.size,
                    mime_type: Some(manifest.mime_type),
                    file_ext: manifest.file_ext,
                    created_at: msg.date().to_string(),
                    icon_type: "file".into(),
                });
                continue;
            }
            let (name, size, mime, ext) = match doc {
                Media::Document(d) => {
                    let doc_name = d.name().unwrap_or_default().to_string();
                    // Prefer the message caption (set by rename via EditMessage) over the
                    // document's built-in filename attribute, so renames persist across refreshes.
                    let display_name = if caption.is_empty() { doc_name.clone() } else { caption.to_string() };
                    let s = d.size().unwrap_or(0);
                    let m = d.mime_type().map(|s| s.to_string());
                    // Extension always from the original document name for correct file-type icon
                    let e = std::path::Path::new(&doc_name).extension().map(|os| os.to_str().unwrap_or("").to_string());
                    (display_name, s, m, e)
                },
                Media::Photo(_) => ("Photo.jpg".to_string(), 0, Some("image/jpeg".into()), Some("jpg".into())),
                _ => ("Unknown".to_string(), 0, None, None),
            };
            files.push(FileMetadata {
                id: msg.id() as i64, folder_id, name, size: size as u64, mime_type: mime, file_ext: ext, created_at: msg.date().to_string(), icon_type: "file".into()
            });
        }
    }

    Ok(files)
}

/// Cache key for a folder's file list. Mirrors favorites: None (home) -> "home".
pub(crate) fn folder_cache_key(folder_id: Option<i64>) -> String {
    match folder_id {
        Some(id) => id.to_string(),
        None => "home".to_string(),
    }
}

fn parse_folder_cache_key(key: &str) -> Option<i64> {
    if key == "home" { None } else { key.parse::<i64>().ok() }
}

fn read_cached_files(
    db_pool: &DbConnection,
    folder_id: Option<i64>,
) -> Result<Vec<FileMetadata>, String> {
    let key = folder_cache_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT message_id, name, size, mime_type, file_ext, created_at, icon_type
             FROM folder_files WHERE folder_key = ?1 ORDER BY message_id DESC",
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let id = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())?;
        let name = stmt.read::<String, _>("name").map_err(|e| e.to_string())?;
        let size = stmt.read::<i64, _>("size").map_err(|e| e.to_string())? as u64;
        let mime_type = stmt.read::<Option<String>, _>("mime_type").ok().flatten();
        let file_ext = stmt.read::<Option<String>, _>("file_ext").ok().flatten();
        let created_at = stmt.read::<String, _>("created_at").map_err(|e| e.to_string())?;
        let icon_type = stmt.read::<String, _>("icon_type").map_err(|e| e.to_string())?;
        files.push(FileMetadata {
            id,
            folder_id,
            name,
            size,
            mime_type,
            file_ext,
            created_at,
            icon_type,
        });
    }
    Ok(files)
}

/// Upsert the freshly fetched list and prune rows that no longer exist on
/// Telegram — the delta half of the folder cache.
fn write_folder_cache(
    db_pool: &DbConnection,
    folder_id: Option<i64>,
    files: &[FileMetadata],
) -> Result<(), String> {
    let key = folder_cache_key(folder_id);
    let conn = db_pool.lock().map_err(|e| e.to_string())?;

    for f in files {
        let mut upsert = conn
            .prepare(
                "INSERT INTO folder_files
                    (folder_key, message_id, name, size, mime_type, file_ext, created_at, icon_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(folder_key, message_id) DO UPDATE SET
                    name = excluded.name,
                    size = excluded.size,
                    mime_type = excluded.mime_type,
                    file_ext = excluded.file_ext,
                    created_at = excluded.created_at,
                    icon_type = excluded.icon_type",
            )
            .map_err(|e| e.to_string())?;
        upsert.bind((1, key.as_str())).map_err(|e| e.to_string())?;
        upsert.bind((2, f.id)).map_err(|e| e.to_string())?;
        upsert.bind((3, f.name.as_str())).map_err(|e| e.to_string())?;
        upsert.bind((4, f.size as i64)).map_err(|e| e.to_string())?;
        upsert.bind((5, f.mime_type.as_deref())).map_err(|e| e.to_string())?;
        upsert.bind((6, f.file_ext.as_deref())).map_err(|e| e.to_string())?;
        upsert.bind((7, f.created_at.as_str())).map_err(|e| e.to_string())?;
        upsert.bind((8, f.icon_type.as_str())).map_err(|e| e.to_string())?;
        upsert.next().map_err(|e| e.to_string())?;
    }

    let live: HashSet<i64> = files.iter().map(|f| f.id).collect();
    let mut stale_ids: Vec<i64> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT message_id FROM folder_files WHERE folder_key = ?1")
            .map_err(|e| e.to_string())?;
        stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
        while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
            let id = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())?;
            if !live.contains(&id) {
                stale_ids.push(id);
            }
        }
    }
    for id in stale_ids {
        let mut del = conn
            .prepare("DELETE FROM folder_files WHERE folder_key = ?1 AND message_id = ?2")
            .map_err(|e| e.to_string())?;
        del.bind((1, key.as_str())).map_err(|e| e.to_string())?;
        del.bind((2, id)).map_err(|e| e.to_string())?;
        del.next().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Instant folder open: serve the local cache when present, otherwise pull
/// once from Telegram and populate it.
#[tauri::command]
pub async fn cmd_get_files(
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
    state: State<'_, TelegramState>,
) -> Result<Vec<FileMetadata>, String> {
    let cached = read_cached_files(db_pool.inner(), folder_id)?;
    if !cached.is_empty() {
        return Ok(cached);
    }
    let files = fetch_files_from_telegram(folder_id, state.inner()).await?;
    write_folder_cache(db_pool.inner(), folder_id, &files)?;
    Ok(files)
}

/// Background delta sync: pull the folder from Telegram and reconcile the cache.
#[tauri::command]
pub async fn cmd_sync_folder(
    folder_id: Option<i64>,
    db_pool: State<'_, DbConnection>,
    state: State<'_, TelegramState>,
) -> Result<Vec<FileMetadata>, String> {
    let files = fetch_files_from_telegram(folder_id, state.inner()).await?;
    write_folder_cache(db_pool.inner(), folder_id, &files)?;
    Ok(files)
}

/// Local search over the folder cache — instant and not capped at 50 results.
#[tauri::command]
pub fn cmd_search_cached_files(
    query: String,
    db_pool: State<'_, DbConnection>,
) -> Result<Vec<FileMetadata>, String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = format!("%{needle}%");
    let conn = db_pool.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT folder_key, message_id, name, size, mime_type, file_ext, created_at, icon_type
             FROM folder_files WHERE lower(name) LIKE ?1 ORDER BY created_at DESC LIMIT 1000",
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, pattern.as_str())).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let folder_key = stmt.read::<String, _>("folder_key").map_err(|e| e.to_string())?;
        let id = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())?;
        let name = stmt.read::<String, _>("name").map_err(|e| e.to_string())?;
        let size = stmt.read::<i64, _>("size").map_err(|e| e.to_string())? as u64;
        let mime_type = stmt.read::<Option<String>, _>("mime_type").ok().flatten();
        let file_ext = stmt.read::<Option<String>, _>("file_ext").ok().flatten();
        let created_at = stmt.read::<String, _>("created_at").map_err(|e| e.to_string())?;
        let icon_type = stmt.read::<String, _>("icon_type").map_err(|e| e.to_string())?;
        files.push(FileMetadata {
            id,
            folder_id: parse_folder_cache_key(&folder_key),
            name,
            size,
            mime_type,
            file_ext,
            created_at,
            icon_type,
        });
    }
    Ok(files)
}

/// Extract FileMetadata entries from a list of Telegram messages returned by SearchGlobal.
fn extract_search_files(msgs: &[tl::enums::Message]) -> Vec<FileMetadata> {
    let mut files = Vec::new();
    for msg in msgs {
        if let tl::enums::Message::Message(m) = msg {
            // Ignore split parts and subtitle metadata messages
            if is_split_part_caption(&m.message) || m.message.starts_with("#telestash_sub:") {
                continue;
            }

            if let Some(tl::enums::MessageMedia::Document(d)) = &m.media {
                if let Some(tl::enums::Document::Document(doc)) = &d.document {
                    let doc_name = doc.attributes.iter().find_map(|a| match a {
                        tl::enums::DocumentAttribute::Filename(f) => Some(f.file_name.clone()),
                        _ => None
                    }).unwrap_or("Unknown".to_string());

                    // Ignore manifest files and split part documents by filename
                    if doc_name.ends_with(SPLIT_MANIFEST_SUFFIX)
                        || doc_name == SPLIT_MANIFEST_UPLOAD_NAME
                        || is_split_part_caption(&doc_name)
                        || crate::models::is_split_part_filename(&doc_name)
                    {
                        continue;
                    }

                    // Prefer the message caption over the built-in document filename
                    let name = if m.message.is_empty() { doc_name.clone() } else { m.message.clone() };

                    // Double-check resolved file name
                    if is_split_part_caption(&name) || name.starts_with("#telestash_sub:") {
                        continue;
                    }

                    let size = doc.size as u64;
                    let mime = doc.mime_type.clone();
                    let ext = std::path::Path::new(&doc_name).extension().map(|os| os.to_str().unwrap_or("").to_string());
                    let folder_id = match &m.peer_id {
                        tl::enums::Peer::Channel(c) => Some(c.channel_id),
                        tl::enums::Peer::User(u) => Some(u.user_id),
                        tl::enums::Peer::Chat(c) => Some(c.chat_id),
                    };
                    files.push(FileMetadata {
                        id: m.id as i64, folder_id, name, size,
                        mime_type: Some(mime), file_ext: ext,
                        created_at: m.date.to_string(), icon_type: "file".into()
                    });
                }
            }
        }
    }
    files
}

#[tauri::command]
pub async fn cmd_search_global(
    query: String,
    state: State<'_, TelegramState>,
) -> Result<Vec<FileMetadata>, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        return Ok(Vec::new());
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    log::info!("Searching global for: {}", query);

    let result = client.invoke(&tl::functions::messages::SearchGlobal {
        q: query,
        filter: tl::enums::MessagesFilter::InputMessagesFilterDocument,
        min_date: 0,
        max_date: 0,
        offset_rate: 0,
        offset_peer: tl::enums::InputPeer::Empty,
        offset_id: 0,
        limit: 50,
        folder_id: None,
        broadcasts_only: false,
        groups_only: false,
        users_only: false,
    }).await.map_err(map_error)?;

    let files = match result {
        tl::enums::messages::Messages::Messages(msgs) => extract_search_files(&msgs.messages),
        tl::enums::messages::Messages::Slice(msgs) => extract_search_files(&msgs.messages),
        _ => Vec::new(),
    };

    Ok(files)
}
