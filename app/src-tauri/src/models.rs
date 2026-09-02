use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "status", content = "data")]
pub enum AuthState {
    LoggedOut,
    AwaitingCode { phone: String, phone_code_hash: String },
    AwaitingPassword { phone: String },
    LoggedIn,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthResult {
    pub success: bool,
    pub next_step: Option<String>, // "code", "password", "dashboard"
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileMetadata {
    pub id: i64,
    pub folder_id: Option<i64>,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub file_ext: Option<String>,
    pub created_at: String, 
    pub icon_type: String, 
}

pub const SPLIT_MANIFEST_VERSION: u8 = 1;
pub const SPLIT_MANIFEST_SUFFIX: &str = ".tdmanifest.json";
pub const SPLIT_MANIFEST_UPLOAD_NAME: &str = "telestash.tdmanifest.json";
pub const SPLIT_PART_CAPTION_PREFIX: &str = "[telestash-part]";
pub const LEGACY_PART_PREFIX_1: &str = "[teledrive-part]";
pub const LEGACY_PART_PREFIX_2: &str = "[telegram-drive-part]";
pub const LEGACY_PART_PREFIX_3: &str = "[tdrive-part]";
pub const LEGACY_PART_PREFIX_4: &str = "[tg-part]";

pub fn is_split_part_caption(caption: &str) -> bool {
    let c = caption.trim();
    c.starts_with(SPLIT_PART_CAPTION_PREFIX)
        || c.starts_with(LEGACY_PART_PREFIX_1)
        || c.starts_with(LEGACY_PART_PREFIX_2)
        || c.starts_with(LEGACY_PART_PREFIX_3)
        || c.starts_with(LEGACY_PART_PREFIX_4)
}

/// Detect a split-part document filename, e.g. "movie.mkv.tdpart0001of0005".
/// The caption prefix is the primary marker, but search results can arrive
/// without captions, so the filename suffix is the fallback signal.
pub fn is_split_part_filename(name: &str) -> bool {
    let trimmed = name.trim_end();
    let Some(idx) = trimmed.rfind(".tdpart") else { return false };
    let suffix = &trimmed[idx + 7..];
    if suffix.len() != 10 {
        return false;
    }
    let (digits_a, rest) = suffix.split_at(4);
    let (of, digits_b) = rest.split_at(2);
    digits_a.chars().all(|c| c.is_ascii_digit())
        && of == "of"
        && digits_b.chars().all(|c| c.is_ascii_digit())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SplitPart {
    pub message_id: i32,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SplitManifest {
    #[serde(alias = "teledrive_split", alias = "telegram_drive_split", alias = "split", alias = "version")]
    pub telestash_split: u8,
    pub filename: String,
    pub size: u64,
    pub mime_type: String,
    pub file_ext: Option<String>,
    pub part_size: u64,
    pub parts: Vec<SplitPart>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FolderMetadata {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    /// Telegram public username (e.g. "mychannel"). None if private.
    pub username: Option<String>,
    /// Whether the channel is public (has a username set).
    pub is_public: bool,
    // Local-first grouping & ordering metadata
    pub group_id: Option<i32>,
    pub display_order: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FolderGroup {
    pub id: i32,
    pub name: String,
    pub color_hex: String,
    pub display_order: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Drive {
    pub chat_id: i64,
    pub name: String,
    pub icon: Option<String>,
}
