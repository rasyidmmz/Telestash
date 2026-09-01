export interface TelegramFile {
    id: number;
    name: string;
    size: number;
    sizeStr: string; // Formatted size
    created_at?: string;
    type?: 'folder' | 'file'; // implied icon_type
    folder_id?: number | null;
    subtitles?: VideoSubtitleInfo[];
}

export interface VideoSubtitleInfo {
    id: string;
    folder_id?: number | null;
    video_message_id: number;
    subtitle_message_id?: number | null;
    format: 'vobsub_idx' | 'vobsub_sub' | 'srt' | 'ass' | 'ssa' | 'vtt';
    language: string;
    label?: string | null;
    original_filename: string;
    is_paired_vobsub: boolean;
    paired_message_id?: number | null;
    created_at: number;
}

export interface TelegramFolder {
    id: number;
    name: string;
    parent_id?: number;
    username?: string;
    /** Whether the channel has a public username set */
    is_public?: boolean;
    group_id?: number | null;
    display_order?: number;
}

export interface FolderGroup {
    id: number;
    name: string;
    color_hex: string;
    display_order: number;
}

export interface FolderInviteInfo {
    link: string;
    is_public: boolean;
    username?: string;
}

export interface QueueItem {
    id: string;
    path: string;
    folderId: number | null;
    status: 'pending' | 'uploading' | 'paused' | 'success' | 'error' | 'cancelled';
    error?: string;
    progress?: number; // 0-100
    uploadedBytes?: number;
    totalBytes?: number;
    speedBytesPerSec?: number;
    tempZipPath?: string; // Set when the item holds a temp file that needs cleanup
}

export interface BandwidthStats {
    up_bytes: number;
    down_bytes: number;
}

export interface DownloadItem {
    id: string;
    messageId: number;
    filename: string;
    folderId: number | null;
    status: 'pending' | 'downloading' | 'paused' | 'success' | 'error' | 'cancelled';
    error?: string;
    progress?: number; // 0-100
    downloadedBytes?: number;
    totalBytes?: number;
    speedBytesPerSec?: number;
    savePath?: string;
}
export interface ShareInfo {
    id: string;
    folder_id: number | null;
    message_id: number;
    file_name: string;
    file_size: number;
    created_at: number;
    expires_at: number | null;
    revoked: boolean;
    has_password: boolean;
    link: string;
}

// ── Rust command return types ────────────────────────────────────────

export interface ArchiveEntry {
    filename: string;
    size: number;
    compressed_size: number;
    is_dir: boolean;
}

export interface VideoMetadata {
    duration_secs: number | null;
    video_codec: string | null;
    has_audio: boolean;
    track_count: number;
    width: number | null;
    height: number | null;
}
