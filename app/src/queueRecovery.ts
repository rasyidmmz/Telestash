import type { DownloadItem, QueueItem } from './types';

export function recoverUploadQueue(value: unknown): QueueItem[] {
    if (!Array.isArray(value)) return [];
    const seen = new Set<string>();
    return value.filter((item): item is QueueItem => {
        if (!isRecord(item) || typeof item.id !== 'string' || !item.id || seen.has(item.id)) return false;
        if (typeof item.path !== 'string' || item.path.trim().length === 0 || !Number.isInteger(item.folderId) && item.folderId !== null) return false;
        if (item.status !== 'pending' && item.status !== 'paused') return false;
        seen.add(item.id);
        return true;
    }).map(item => ({ ...item, status: 'paused' as const, error: undefined }));
}

export function recoverDownloadQueue(value: unknown): DownloadItem[] {
    if (!Array.isArray(value)) return [];
    const seen = new Set<string>();
    return value.filter((item): item is DownloadItem => {
        if (!isRecord(item) || typeof item.id !== 'string' || !item.id || seen.has(item.id)) return false;
        if (!Number.isInteger(item.messageId) || typeof item.filename !== 'string' || !item.filename.trim()) return false;
        if (!Number.isInteger(item.folderId) && item.folderId !== null) return false;
        if (item.status !== 'pending' && item.status !== 'paused') return false;
        if (item.savePath !== undefined && (typeof item.savePath !== 'string' || !item.savePath.trim())) return false;
        seen.add(item.id);
        return true;
    }).map(item => ({ ...item, status: 'paused' as const, error: undefined }));
}

function isRecord(value: unknown): value is Record<string, any> {
    return typeof value === 'object' && value !== null;
}
