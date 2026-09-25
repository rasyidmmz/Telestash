//! Folder (Telegram channel) lifecycle: create, delete, and rename.
//!
//! Each folder is a broadcast channel whose title carries a `[TD]` marker, so
//! scanning can find them again. The `*_inner` functions hold the Telegram work
//! and take the client explicitly; the `cmd_*` wrappers add the Tauri state and
//! keep the local `folder_metadata` table in sync.

use std::collections::HashMap;
use std::sync::Arc;

use grammers_client::peer::Peer;
use grammers_session::types::PeerRef;
use grammers_tl_types as tl;
use serde::Serialize;
use sqlite;
use tauri::State;

use crate::commands::utils::{map_error, resolve_peer};
use crate::db::DbConnection;
use crate::models::FolderMetadata;
use crate::TelegramState;

pub async fn create_folder_inner(
    name: &str,
    client: &grammers_client::Client,
    peer_cache: &Arc<tokio::sync::RwLock<HashMap<i64, PeerRef>>>,
) -> Result<FolderMetadata, String> {
    log::info!("Creating Telegram Channel: {}", name);

    let result = client
        .invoke(&tl::functions::channels::CreateChannel {
            broadcast: true,
            megagroup: false,
            title: format!("{} [TD]", name),
            about: "TeleStash Storage Folder\n[telestash-folder]".to_string(),
            geo_point: None,
            address: None,
            for_import: false,
            forum: false,
            ttl_period: None,
        })
        .await
        .map_err(map_error)?;

    let (chat_id, access_hash) = match &result {
        tl::enums::Updates::Updates(u) => {
            let chat = u.chats.first().ok_or("No chat in updates")?;
            match chat {
                tl::enums::Chat::Channel(c) => {
                    let peer_ref =
                        grammers_session::types::PeerRef::from(tl::enums::Chat::Channel(c.clone()));
                    peer_cache.write().await.insert(c.id, peer_ref);
                    (c.id, c.access_hash.unwrap_or(0))
                }
                _ => return Err("Created chat is not a channel".to_string()),
            }
        }
        _ => return Err("Unexpected response (not Updates::Updates)".to_string()),
    };

    let _ = client
        .invoke(&tl::functions::messages::SetHistoryTtl {
            peer: tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
                channel_id: chat_id,
                access_hash,
            }),
            period: 0,
        })
        .await;
    Ok(FolderMetadata {
        id: chat_id,
        name: name.to_string(),
        parent_id: None,
        username: None,
        is_public: false,
        group_id: None,
        display_order: 0,
    })
}

#[tauri::command]
pub async fn cmd_create_folder(
    name: String,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<FolderMetadata, String> {
    let client_opt = { state.client.lock().await.clone() };

    let mut folder = if client_opt.is_none() {
        let mock_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        log::info!("[MOCK] Created folder '{}' with ID {}", name, mock_id);
        FolderMetadata {
            id: mock_id,
            name,
            parent_id: None,
            username: None,
            is_public: false,
            group_id: None,
            display_order: 0,
        }
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
        create_folder_inner(&name, &client, &state.peer_cache).await?
    };

    // Save to SQLite
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;

    // Calculate new display order
    let mut max_stmt = conn
        .prepare("SELECT MAX(display_order) FROM folder_metadata")
        .map_err(|e: sqlite::Error| e.to_string())?;
    let mut display_order = 0;
    if let sqlite::State::Row = max_stmt.next().map_err(|e: sqlite::Error| e.to_string())? {
        display_order = max_stmt
            .read::<Option<i64>, _>(0)
            .ok()
            .flatten()
            .unwrap_or(0)
            + 1;
    }

    let mut insert_stmt = conn
        .prepare("INSERT INTO folder_metadata (channel_id, name, username, is_public, display_order, group_id) VALUES (?, ?, ?, ?, ?, NULL)")
        .map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt
        .bind((1, folder.id))
        .map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt
        .bind((2, folder.name.as_str()))
        .map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt
        .bind((3, folder.username.as_deref()))
        .map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt
        .bind((4, if folder.is_public { 1 } else { 0 }))
        .map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt
        .bind((5, display_order))
        .map_err(|e: sqlite::Error| e.to_string())?;
    insert_stmt
        .next()
        .map_err(|e: sqlite::Error| e.to_string())?;

    folder.display_order = display_order as i32;
    Ok(folder)
}

pub async fn delete_folder_inner(
    folder_id: i64,
    client: &grammers_client::Client,
    peer_cache: &Arc<tokio::sync::RwLock<HashMap<i64, PeerRef>>>,
) -> Result<bool, String> {
    log::info!("Deleting folder/channel: {}", folder_id);

    let peer = resolve_peer(client, Some(folder_id), peer_cache).await?;

    let input_channel = match tl::enums::InputPeer::from(peer) {
        tl::enums::InputPeer::Channel(ic) => {
            tl::enums::InputChannel::Channel(tl::types::InputChannel {
                channel_id: ic.channel_id,
                access_hash: ic.access_hash,
            })
        }
        _ => return Err("Only channels (folders) can be deleted.".to_string()),
    };

    client
        .invoke(&tl::functions::channels::DeleteChannel {
            channel: input_channel,
        })
        .await
        .map_err(|e| format!("Failed to delete channel: {}", e))?;

    Ok(true)
}

#[tauri::command]
pub async fn cmd_delete_folder(
    folder_id: i64,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<bool, String> {
    let client_opt = { state.client.lock().await.clone() };

    if client_opt.is_none() {
        log::info!("[MOCK] Deleted folder ID {}", folder_id);
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
        delete_folder_inner(folder_id, &client, &state.peer_cache).await?;
    }

    // Delete from SQLite
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let mut stmt = conn
        .prepare("DELETE FROM folder_metadata WHERE channel_id = ?")
        .map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((1, folder_id))
        .map_err(|e: sqlite::Error| e.to_string())?;
    stmt.next().map_err(|e: sqlite::Error| e.to_string())?;

    Ok(true)
}

pub async fn rename_folder_inner(
    folder_id: i64,
    new_name: &str,
    client: &grammers_client::Client,
    peer_cache: &Arc<tokio::sync::RwLock<HashMap<i64, PeerRef>>>,
) -> Result<bool, String> {
    log::info!("Renaming folder/channel: {} to {}", folder_id, new_name);

    let peer = resolve_peer(client, Some(folder_id), peer_cache).await?;

    let input_channel = match tl::enums::InputPeer::from(peer) {
        tl::enums::InputPeer::Channel(ic) => {
            tl::enums::InputChannel::Channel(tl::types::InputChannel {
                channel_id: ic.channel_id,
                access_hash: ic.access_hash,
            })
        }
        _ => return Err("Only channels (folders) can be renamed.".to_string()),
    };

    client
        .invoke(&tl::functions::channels::EditTitle {
            channel: input_channel,
            title: format!("{} [TD]", new_name),
        })
        .await
        .map_err(|e| format!("Failed to rename channel: {}", e))?;

    Ok(true)
}

#[tauri::command]
pub async fn cmd_rename_folder(
    folder_id: i64,
    new_name: String,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<bool, String> {
    let client_opt = { state.client.lock().await.clone() };

    if client_opt.is_none() {
        log::info!("[MOCK] Renamed folder ID {} to {}", folder_id, new_name);
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;
        rename_folder_inner(folder_id, &new_name, &client, &state.peer_cache).await?;
    }

    // Update SQLite
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let mut stmt = conn
        .prepare("UPDATE folder_metadata SET name = ? WHERE channel_id = ?")
        .map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((1, new_name.as_str()))
        .map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((2, folder_id))
        .map_err(|e: sqlite::Error| e.to_string())?;
    stmt.next().map_err(|e: sqlite::Error| e.to_string())?;

    Ok(true)
}

/// Toggle a folder (channel) between private and public.
/// When making public, a username is generated from the channel title.
/// When making private, the username is removed.
#[tauri::command]
pub async fn cmd_toggle_folder_visibility(
    folder_id: i64,
    make_public: bool,
    desired_username: Option<String>,
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<FolderMetadata, String> {
    let client_opt = {
        state.client.lock().await.clone()
    };

    let mut folder = if client_opt.is_none() {
        log::info!("[MOCK] Toggle visibility for folder {}. Public: {}", folder_id, make_public);
        FolderMetadata {
            id: folder_id,
            name: "Mock Folder".to_string(),
            parent_id: None,
            username: if make_public { desired_username } else { None },
            is_public: make_public,
            group_id: None,
            display_order: 0,
        }
    } else {
        let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

        let peer = resolve_peer(&client, Some(folder_id), &state.peer_cache).await?;
        let (channel_id, access_hash) = match tl::enums::InputPeer::from(peer) {
            tl::enums::InputPeer::Channel(ic) => (ic.channel_id, ic.access_hash),
            _ => return Err("Only channels (folders) can be toggled.".to_string()),
        };

        let input_channel = tl::enums::InputChannel::Channel(tl::types::InputChannel {
            channel_id,
            access_hash,
        });

        // Extract channel name via the session-resolved peer
        let channel_name = match client.invoke(&tl::functions::channels::GetChannels {
            id: vec![input_channel.clone()],
        }).await.map_err(|e| e.to_string())? {
            tl::enums::messages::Chats::Chats(ch) => ch.chats.first().and_then(|c| match c {
                tl::enums::Chat::Channel(ch) => Some(ch.title.replace(" [TD]", "").replace(" [td]", "").trim().to_string()),
                _ => None,
            }),
            _ => None,
        }.unwrap_or_else(|| "Folder".to_string());

        if make_public {
            // Generate a username from the desired_username or channel title.
            // If desired_username is provided AND non-empty, use it directly;
            // otherwise auto-generate from the channel title.
            let username = if let Some(ref u) = desired_username {
                if !u.is_empty() {
                    Some(u.clone())
                } else {
                    None // empty string → fall through to auto-generation below
                }
            } else {
                None
            };

            let username = match username {
                Some(given) => {
                    // User-provided username: check availability first
                    let available = client
                        .invoke(&tl::functions::channels::CheckUsername {
                            channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                                channel_id,
                                access_hash,
                            }),
                            username: given.clone(),
                        })
                        .await
                        .map_err(|e| format!("Failed to check username availability: {}", map_error(e)))?;
                    if !available {
                        return Err(format!("Username '{}' is not available. Try a different one.", given));
                    }
                    given
                }
                None => {
                    // Auto-generate username from channel title
                    // channel_name already has [TD] stripped above
                    let mut base = channel_name.clone()
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_alphanumeric() || *c == '_')
                        .take(30)
                        .collect::<String>();
                    if base.len() < 5 {
                        let suffix: String = (0..6)
                            .map(|_| char::from(b'a' + (rand::random::<u8>() % 26)))
                            .collect();
                        base = format!("{}_{}", base, suffix);
                    }
                    // Try to find an available username
                    let mut candidate = base.clone();
                    for attempt in 1..=10 {
                        match client
                            .invoke(&tl::functions::channels::CheckUsername {
                                channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                                    channel_id,
                                    access_hash,
                                }),
                                username: candidate.clone(),
                            })
                            .await
                        {
                            Ok(true) => break,
                            _ => {
                                candidate = format!("{}{}", base, attempt);
                                if attempt == 10 {
                                    return Err("Could not find an available username after 10 attempts".to_string());
                                }
                            }
                        }
                    }
                    candidate
                }
            };

            log::info!("Setting channel {} username to '{}'", channel_id, username);
            client
                .invoke(&tl::functions::channels::UpdateUsername {
                    channel: input_channel,
                    username: username.clone(),
                })
                .await
                .map_err(|e| format!("Failed to set username: {}", map_error(e)))?;

            FolderMetadata {
                id: channel_id,
                name: channel_name,
                parent_id: None,
                username: Some(username),
                is_public: true,
                group_id: None,
                display_order: 0,
            }
        } else {
            // Make private: remove username
            log::info!("Removing username from channel {}", channel_id);
            client
                .invoke(&tl::functions::channels::UpdateUsername {
                    channel: input_channel,
                    username: String::new(),
                })
                .await
                .map_err(|e| format!("Failed to remove username: {}", map_error(e)))?;

            FolderMetadata {
                id: channel_id,
                name: channel_name,
                parent_id: None,
                username: None,
                is_public: false,
                group_id: None,
                display_order: 0,
            }
        }
    };

    // Update SQLite cache
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let mut stmt = conn
        .prepare("UPDATE folder_metadata SET username = ?, is_public = ? WHERE channel_id = ?")
        .map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((1, folder.username.as_deref())).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((2, if folder.is_public { 1 } else { 0 })).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.bind((3, folder.id)).map_err(|e: sqlite::Error| e.to_string())?;
    stmt.next().map_err(|e: sqlite::Error| e.to_string())?;

    // Retrieve group_id and display_order from DB to ensure they are returned correctly
    let mut fm_stmt = conn
        .prepare("SELECT group_id, display_order FROM folder_metadata WHERE channel_id = ?")
        .map_err(|e: sqlite::Error| e.to_string())?;
    fm_stmt.bind((1, folder.id)).map_err(|e: sqlite::Error| e.to_string())?;
    if let sqlite::State::Row = fm_stmt.next().map_err(|e: sqlite::Error| e.to_string())? {
        folder.group_id = fm_stmt.read::<Option<i64>, _>("group_id").ok().flatten().map(|id| id as i32);
        folder.display_order = fm_stmt.read::<i64, _>("display_order").map_err(|e: sqlite::Error| e.to_string())? as i32;
    }

    Ok(folder)
}

/// Export a Telegram invite link for a folder (channel).
/// For public channels, returns the t.me/username link directly.
/// For private channels, exports a hash-based invite link via the API.
#[derive(Debug, Serialize)]
pub struct FolderInviteInfo {
    pub link: String,
    pub is_public: bool,
    pub username: Option<String>,
}

#[tauri::command]
pub async fn cmd_export_folder_invite(
    folder_id: i64,
    state: State<'_, TelegramState>,
) -> Result<FolderInviteInfo, String> {
    let client_opt = {
        state.client.lock().await.clone()
    };

    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        log::info!("[MOCK] Export invite for folder {}", folder_id);
        return Ok(FolderInviteInfo {
            link: "https://t.me/joinchat/mock-invite-hash".to_string(),
            is_public: false,
            username: None,
        });
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    let peer = resolve_peer(&client, Some(folder_id), &state.peer_cache).await?;
    let (channel_id, access_hash) = match tl::enums::InputPeer::from(peer) {
        tl::enums::InputPeer::Channel(ic) => (ic.channel_id, ic.access_hash),
        _ => return Err("Only channels (folders) can have invite links.".to_string()),
    };

    // Check if channel already has a public username (fetched via the session-resolved peer)
    let input_channel = tl::enums::InputChannel::Channel(tl::types::InputChannel {
        channel_id,
        access_hash,
    });
    let username: Option<String> = match client.invoke(&tl::functions::channels::GetChannels {
        id: vec![input_channel.clone()],
    }).await.map_err(|e| e.to_string())? {
        tl::enums::messages::Chats::Chats(ch) => ch.chats.first().and_then(|c| match c {
            tl::enums::Chat::Channel(ch) => ch.username.clone(),
            _ => None,
        }),
        _ => None,
    };

    if let Some(ref uname) = username {
        // Public channel: return the t.me/username link
        Ok(FolderInviteInfo {
            link: format!("https://t.me/{}", uname),
            is_public: true,
            username: Some(uname.clone()),
        })
    } else {
        // Private channel: export an invite link
        let result = client
            .invoke(&tl::functions::messages::ExportChatInvite {
                peer: tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
                    channel_id,
                    access_hash,
                }),
                legacy_revoke_permanent: false,
                request_needed: false,
                expire_date: None,
                usage_limit: None,
                title: None,
                subscription_pricing: None,
            })
            .await
            .map_err(|e| format!("Failed to export invite: {}", map_error(e)))?;

        let link = match result {
            tl::enums::ExportedChatInvite::ChatInviteExported(c) => c.link,
            tl::enums::ExportedChatInvite::ChatInvitePublicJoinRequests => {
                return Err("Public join request channels do not have a custom private invite link. Share the public username directly instead.".to_string());
            }
        };

        Ok(FolderInviteInfo {
            link,
            is_public: false,
            username: None,
        })
    }
}

/// Scan Telegram dialogs for TeleStash folders (channels/groups marked with [TD]
/// or the "[telestash-folder]" about marker) and reconcile them with the local DB.
#[tauri::command]
pub async fn cmd_scan_folders(
    state: State<'_, TelegramState>,
    db_pool: State<'_, DbConnection>,
) -> Result<Vec<FolderMetadata>, String> {
    let client_opt = { state.client.lock().await.clone() };
    #[cfg(debug_assertions)]
    if client_opt.is_none() {
        // If not connected, return whatever is already in the database
        return crate::commands::folder_groups::cmd_get_enriched_folders(db_pool).await;
    }
    let client = client_opt.ok_or_else(|| "Client not connected".to_string())?;

    let mut folders = Vec::new();
    let mut dialogs = client.iter_dialogs();
    let mut discovered = HashMap::new();

    log::info!("Starting Folder Scan...");

    while let Some(dialog) = dialogs.next().await.map_err(|e| e.to_string())? {
        // Populate peer cache for every dialog we encounter (free priming)
        let channel_info = match &dialog.peer {
            Peer::Channel(c) => Some(&c.raw),
            Peer::Group(g) => match &g.raw {
                tl::enums::Chat::Channel(c) => Some(c),
                tl::enums::Chat::Chat(chat) => {
                    let id = chat.id;
                    if let Ok(Some(pr)) = dialog.peer.to_ref().await { discovered.insert(id, pr); }
                    let name = chat.title.clone();
                    log::debug!("[SCAN] Processing Group Chat: '{}' (ID: {})", name, id);
                    if name.to_lowercase().contains("[td]") {
                        log::info!(" -> MATCH via Title: {}", name);
                        let display_name = name.replace(" [TD]", "").replace(" [td]", "").replace("[TD]", "").replace("[td]", "").trim().to_string();
                        folders.push(FolderMetadata { id, name: display_name, parent_id: None, username: None, is_public: false, group_id: None, display_order: 0 });
                    }
                    None
                }
                _ => {
                    if let Ok(Some(pr)) = dialog.peer.to_ref().await {
                        if let Some(id) = dialog.peer.id().bare_id() {
                            discovered.insert(id, pr);
                        }
                    }
                    None
                }
            },
            Peer::User(u) => {
                if let Ok(Some(pr)) = dialog.peer.to_ref().await { discovered.insert(u.raw.id(), pr); }
                log::debug!("[SCAN] Cached User Peer: {}", u.raw.id());
                None
            },
        };

        if let Some(c) = channel_info {
            let id = c.id;
            if let Ok(Some(pr)) = dialog.peer.to_ref().await { discovered.insert(id, pr); }

            let name = c.title.clone();
            let access_hash = c.access_hash.unwrap_or(0);

            log::debug!("[SCAN] Processing Channel/Supergroup: '{}' (ID: {})", name, id);

            // Strategy 1: Title
            if name.to_lowercase().contains("[td]") {
                log::info!(" -> MATCH via Title: {}", name);
                let display_name = name.replace(" [TD]", "").replace(" [td]", "").replace("[TD]", "").replace("[td]", "").trim().to_string();
                let username = c.username.clone();
                let is_public = username.is_some();
                folders.push(FolderMetadata { id, name: display_name, parent_id: None, username, is_public, group_id: None, display_order: 0 });
                continue;
            }

            // Strategy 2: About (Only if we are the creator to avoid rate limits on third-party channels)
            if c.creator {
                let input_chan = tl::enums::InputChannel::Channel(tl::types::InputChannel {
                    channel_id: c.id,
                    access_hash,
                });

                match client.invoke(&tl::functions::channels::GetFullChannel {
                    channel: input_chan,
                }).await {
                    Ok(tl::enums::messages::ChatFull::Full(f)) => {
                        if let tl::enums::ChatFull::Full(cf) = f.full_chat {
                             if cf.about.contains("[telestash-folder]") {
                                 log::info!(" -> MATCH via About: {}", name);
                                 let username = c.username.clone();
                                 let is_public = username.is_some();
                                 folders.push(FolderMetadata { id, name: name.clone(), parent_id: None, username, is_public, group_id: None, display_order: 0 });
                             }
                        }
                    },
                    Err(e) => log::warn!(" -> Failed to get full info: {}", e),
                }
            }
        }
    }

    {
        let mut cache = state.peer_cache.write().await;
        cache.extend(discovered);
    }

    let cache_len = state.peer_cache.read().await.len();
    log::info!("Scan complete. Found {} folders. Peer cache size: {}.", folders.len(), cache_len);

    // Enrich folders via the local DB
    let conn = db_pool.lock().map_err(|_| "DB poisoned".to_string())?;
    let enriched = crate::commands::folder_groups::get_enriched_folders_internal(&conn, folders)?;
    Ok(enriched)
}
