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
    /// false = name+size candidate only (no content download)
    /// true  = confirmed by sha256 of the first 256 KiB
    #[serde(default = "default_confirmed")]
    pub confirmed: bool,
}

fn default_confirmed() -> bool {
    false
}

const MIN_SIZE: u64 = 1_000_000; // ignore tiny files
const HASH_BYTES: u64 = 256 * 1024; // prefix length for the confirmation hash
const MAX_GROUPS: usize = 200;
const MAX_GROUP_FILES: usize = 12;

/// Normalize a filename for the fast duplicate lane:
/// case-fold, trim, strip common " (1)" / " copy" suffixes from the stem.
pub(crate) fn normalize_duplicate_name(name: &str) -> String {
    let trimmed = name.trim().to_ascii_lowercase();
    let Some(dot) = trimmed.rfind('.') else {
        return strip_copy_suffix(&trimmed).to_string();
    };
    let (stem, ext) = trimmed.split_at(dot);
    format!("{}{}", strip_copy_suffix(stem), ext)
}

fn strip_copy_suffix(stem: &str) -> &str {
    let t = stem.trim_end();
    // "movie (1)" / "movie(2)"
    if t.ends_with(')') {
        if let Some(open) = t.rfind(" (") {
            let inner = &t[open + 2..t.len().saturating_sub(1)];
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                return t[..open].trim_end();
            }
        }
    }
    // "movie - copy" / "movie copy"
    for suffix in [" - copy", " copy", "-copy"] {
        if let Some(base) = t.strip_suffix(suffix) {
            let base = base.trim_end();
            if !base.is_empty() {
                return base;
            }
        }
    }
    t
}

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

/// Find duplicate documents across the given folders.
///
/// Fast lane: group by exact size + normalized filename (no download).
/// Confirm lane (default): download 256 KiB of each candidate and group by hash.
/// Pass `confirm: false` to return name+size candidates only (much faster).
#[tauri::command]
pub async fn cmd_find_duplicates(
    folder_ids: Vec<Option<i64>>,
    confirm: Option<bool>,
    state: State<'_, TelegramState>,
    _net_config: State<'_, Arc<TransferPolicy>>,
) -> Result<Vec<DuplicateGroup>, String> {
    let confirm = confirm.unwrap_or(true);
    let client = { state.client.lock().await.clone() }.ok_or("Telegram client not initialized")?;

    // 1. Collect all candidate documents
    let mut collected: Vec<DuplicateFileInfo> = Vec::new();
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
                Err(_) => break,
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
                collected.push(DuplicateFileInfo {
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

    // 2. Fast lane: group by (normalized name, size)
    let mut by_name_size: HashMap<(String, u64), Vec<DuplicateFileInfo>> = HashMap::new();
    for file in collected {
        let key = (normalize_duplicate_name(&file.name), file.size);
        by_name_size.entry(key).or_default().push(file);
    }

    let mut candidates: Vec<Vec<DuplicateFileInfo>> = by_name_size
        .into_values()
        .filter(|files| files.len() > 1 && files.len() <= MAX_GROUP_FILES)
        .collect();
    candidates.sort_by(|a, b| b[0].size.cmp(&a[0].size));
    candidates.truncate(MAX_GROUPS);

    if !confirm {
        let groups = candidates
            .into_iter()
            .map(|files| {
                let size = files[0].size;
                DuplicateGroup {
                    size,
                    hash: String::new(),
                    files,
                    confirmed: false,
                }
            })
            .collect();
        return Ok(groups);
    }

    // 3. Confirm lane: content hash of first 256 KiB only for name+size candidates
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for mut files in candidates {
        let size = files[0].size;
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
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
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
                groups.push(DuplicateGroup {
                    size,
                    hash,
                    files: group_files,
                    confirmed: true,
                });
            }
        }
    }

    groups.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_copy_suffixes() {
        assert_eq!(normalize_duplicate_name("Movie.mkv"), "movie.mkv");
        assert_eq!(normalize_duplicate_name("Movie (1).mkv"), "movie.mkv");
        assert_eq!(normalize_duplicate_name("Movie.mkv (2)"), "movie.mkv");
        assert_eq!(normalize_duplicate_name("Movie - Copy.MKV"), "movie.mkv");
        assert_eq!(normalize_duplicate_name("Movie copy.mp4"), "movie.mp4");
        assert_eq!(normalize_duplicate_name("Different Name.mkv"), "different name.mkv");
    }

    #[test]
    fn keeps_distinct_names_apart() {
        assert_ne!(
            normalize_duplicate_name("Matrix.mkv"),
            normalize_duplicate_name("Matrix Reloaded.mkv")
        );
    }
}
