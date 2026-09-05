use std::collections::HashMap;
use std::sync::Arc;

use grammers_client::media::Media;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::TelegramState;
use crate::transfer_policy::TransferPolicy;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DuplicateFileInfo {
    pub message_id: i64,
    pub folder_id: Option<i64>,
    pub name: String,
    pub size: u64,
    pub created_at: String,
    pub hash: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DuplicateGroup {
    pub size: u64,
    pub hash: String,
    pub files: Vec<DuplicateFileInfo>,
}

const MIN_SIZE: u64 = 1_000_000; // ignore tiny files
const HASH_BYTES: u64 = 256 * 1024; // prefix length for the confirmation hash
const MAX_GROUPS: usize = 200;
const MAX_GROUP_FILES: usize = 12;

/// sha256 of the first HASH_BYTES of a remote document.
async fn hash_file_prefix(
    client: &grammers_client::Client,
    media: &Media,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut iter = client.iter_download(media);
    iter = iter.chunk_size(64 * 1024);
    while downloaded < HASH_BYTES {
        let Some(chunk) = iter.next().await.map_err(|e| e.to_string())? else { break };
        let take = std::cmp::min(chunk.len() as u64, HASH_BYTES - downloaded) as usize;
        hasher.update(&chunk[..take]);
        downloaded += take as u64;
        if (chunk.len() as u64) < HASH_BYTES - downloaded {
            break; // short file
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Find duplicate documents across the given folders by exact size, then
/// confirm with a content hash of each file's first 256 KiB.
#[tauri::command]
pub async fn cmd_find_duplicates(
    folder_ids: Vec<Option<i64>>,
    state: State<'_, TelegramState>,
    _net_config: State<'_, Arc<TransferPolicy>>,
) -> Result<Vec<DuplicateGroup>, String> {
    let client = { state.client.lock().await.clone() }.ok_or("Telegram client not initialized")?;

    // 1. Collect all candidate documents, grouped by size
    let mut by_size: HashMap<u64, Vec<DuplicateFileInfo>> = HashMap::new();
    for folder_id in folder_ids {
        let peer = match crate::commands::utils::resolve_peer(&client, folder_id, &state.peer_cache).await {
            Ok(p) => p,
            Err(_) => continue,
        };
        let mut msgs = client.iter_messages(peer);
        loop {
            let msg = match msgs.next().await {
                Ok(Some(m)) => m,
                Ok(None) => break,
                Err(_) => break, // skip folders that fail to iterate
            };
            if let Some(Media::Document(d)) = msg.media() {
                let size = d.size().unwrap_or(0) as u64;
                if size < MIN_SIZE {
                    continue;
                }
                let name = d.name().unwrap_or_default().to_string();
                if name.starts_with("[telestash-part]") || name.starts_with("#telestash_sub:") {
                    continue;
                }
                by_size.entry(size).or_default().push(DuplicateFileInfo {
                    message_id: msg.id() as i64,
                    folder_id,
                    name,
                    size,
                    created_at: msg.date().to_string(),
                    hash: String::new(),
                });
            }
        }
    }

    // 2. Candidate groups (same exact size, more than one file)
    let mut candidates: Vec<(u64, Vec<DuplicateFileInfo>)> = by_size
        .into_iter()
        .filter(|(_, files)| files.len() > 1 && files.len() <= MAX_GROUP_FILES)
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.truncate(MAX_GROUPS);

    // 3. Confirm with content hash
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for (size, mut files) in candidates {
        for file in files.iter_mut() {
            let peer = match crate::commands::utils::resolve_peer(&client, file.folder_id, &state.peer_cache).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let messages = match client.get_messages_by_id(peer, &[file.message_id as i32]).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let Some(msg) = messages.into_iter().flatten().next() else { continue };
            let Some(media) = msg.media() else { continue };
            if let Ok(hash) = hash_file_prefix(&client, &media).await {
                file.hash = hash;
            }
            tokio::time::sleep(std::time::Duration::from_millis(120)).await; // gentle on MTProto
        }
        let mut by_hash: HashMap<String, Vec<DuplicateFileInfo>> = HashMap::new();
        for file in files {
            if file.hash.is_empty() {
                continue;
            }
            by_hash.entry(file.hash.clone()).or_default().push(file);
        }
        for (hash, group_files) in by_hash {
            if group_files.len() > 1 {
                groups.push(DuplicateGroup { size, hash, files: group_files });
            }
        }
    }

    groups.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(groups)
}
