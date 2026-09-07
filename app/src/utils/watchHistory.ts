import { invoke } from '@tauri-apps/api/core';
import { TelegramFile } from '../types';

export interface WatchHistoryEntry {
    id: string;
    file_id: number;
    file_name: string;
    folder_id: number | null;
    file_size: number;
    timestamp: string; // ISO string
    last_position_secs?: number;
    total_duration_secs?: number;
    status: 'started' | 'playing' | 'completed' | 'paused';
    quality_tag?: string;
}

export interface WatchLogEvent {
    id: string;
    timestamp: string;
    file_name: string;
    event_type: 'PLAY_START' | 'PAUSE' | 'RESUME' | 'SUBTITLE_GEN' | 'SEEK' | 'COMPLETED' | 'ERROR';
    details: string;
}

const STORAGE_KEY_HISTORY = 'telestash_recent_watch_v1';
const STORAGE_KEY_LOGS = 'telestash_watch_logs_v1';
const MAX_LOG_ENTRIES = 500;
const MAX_HISTORY_ENTRIES = 50;

/**
 * Watch history is persisted in SQLite via the backend; this module mirrors
 * the table in memory so callers keep their synchronous API. Mutations update
 * the mirror immediately and persist fire-and-forget.
 */
let storeLoaded = false;
let storeLoading: Promise<void> | null = null;
let memoryHistory: WatchHistoryEntry[] = [];

interface WatchHistoryRow {
    file_id: number;
    file_name: string;
    folder_id: number | null;
    file_size: number;
    timestamp: number; // epoch millis
    status: string;
    quality_tag?: string | null;
    last_position_secs?: number | null;
    total_duration_secs?: number | null;
}

const rowToEntry = (row: WatchHistoryRow): WatchHistoryEntry => ({
    id: `watch_${row.timestamp}_${row.file_id}`,
    file_id: row.file_id,
    file_name: row.file_name,
    folder_id: row.folder_id ?? null,
    file_size: row.file_size,
    timestamp: new Date(row.timestamp).toISOString(),
    status: (row.status as WatchHistoryEntry['status']) || 'started',
    quality_tag: row.quality_tag ?? undefined,
    last_position_secs: row.last_position_secs ?? undefined,
    total_duration_secs: row.total_duration_secs ?? undefined,
});

const entryToRow = (entry: WatchHistoryEntry): WatchHistoryRow => ({
    file_id: entry.file_id,
    file_name: entry.file_name,
    folder_id: entry.folder_id,
    file_size: entry.file_size,
    timestamp: new Date(entry.timestamp).getTime(),
    status: entry.status,
    quality_tag: entry.quality_tag ?? null,
    last_position_secs: entry.last_position_secs ?? null,
    total_duration_secs: entry.total_duration_secs ?? null,
});

const sortNewestFirst = (items: WatchHistoryEntry[]) =>
    items.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

const persistEntry = (entry: WatchHistoryEntry) => {
    invoke('cmd_watch_history_upsert', { entry: entryToRow(entry) })
        .catch((e) => console.error('Failed to persist watch entry:', e));
};

const readLegacyHistory = (): WatchHistoryEntry[] => {
    try {
        const raw = localStorage.getItem(STORAGE_KEY_HISTORY);
        if (!raw) return [];
        const items: WatchHistoryEntry[] = JSON.parse(raw);
        return Array.isArray(items) ? items : [];
    } catch {
        return [];
    }
};

/**
 * Load the SQLite-backed store into memory once and migrate any legacy
 * localStorage entries. Safe to call multiple times; the returned promise is
 * shared while loading.
 */
export function initWatchHistory(): Promise<void> {
    if (storeLoaded) return Promise.resolve();
    if (storeLoading) return storeLoading;

    storeLoading = (async () => {
        try {
            const rows = await invoke<WatchHistoryRow[]>('cmd_watch_history_list');
            const fromBackend = rows.map(rowToEntry);

            // One-time migration: push legacy localStorage entries into SQLite
            // (INSERT OR IGNORE keeps this harmless if already imported).
            const legacy = readLegacyHistory();
            let merged = fromBackend;
            if (legacy.length > 0) {
                try {
                    await invoke('cmd_watch_history_import', { entries: legacy.map(entryToRow) });
                    localStorage.removeItem(STORAGE_KEY_HISTORY);
                    const byId = new Map(fromBackend.map((e) => [e.file_id, e]));
                    for (const entry of legacy) {
                        if (!byId.has(entry.file_id)) byId.set(entry.file_id, entry);
                    }
                    merged = Array.from(byId.values());
                } catch (e) {
                    console.error('Watch history migration failed:', e);
                }
            }

            // Keep any entries recorded while the store was still loading.
            const byId = new Map(merged.map((e) => [e.file_id, e]));
            for (const entry of memoryHistory) {
                byId.set(entry.file_id, entry);
            }
            memoryHistory = sortNewestFirst(Array.from(byId.values())).slice(0, MAX_HISTORY_ENTRIES);
        } catch (e) {
            console.error('Failed to load watch history:', e);
        } finally {
            storeLoaded = true;
            storeLoading = null;
        }
    })();
    return storeLoading;
}

/**
 * Get all recent watch history entries sorted by latest first
 */
export function getRecentWatchHistory(): WatchHistoryEntry[] {
    if (!storeLoaded) {
        initWatchHistory().catch(() => { /* load errors handled inside */ });
        return memoryHistory;
    }
    return memoryHistory;
}

/**
 * Record a playback start or update event
 */
export function recordWatchEvent(
    file: TelegramFile,
    status: WatchHistoryEntry['status'] = 'started',
    qualityTag?: string,
    lastPosSecs?: number,
    totalDurationSecs?: number
): WatchHistoryEntry {
    const existingIndex = memoryHistory.findIndex(item => item.file_id === file.id);

    const entry: WatchHistoryEntry = {
        id: existingIndex >= 0 ? memoryHistory[existingIndex].id : `watch_${Date.now()}_${file.id}`,
        file_id: file.id,
        file_name: file.name,
        folder_id: file.folder_id ?? null,
        file_size: file.size,
        timestamp: new Date().toISOString(),
        status,
        quality_tag: qualityTag || (existingIndex >= 0 ? memoryHistory[existingIndex]?.quality_tag : undefined),
        last_position_secs: lastPosSecs ?? (existingIndex >= 0 ? memoryHistory[existingIndex]?.last_position_secs : 0),
        total_duration_secs: totalDurationSecs ?? (existingIndex >= 0 ? memoryHistory[existingIndex]?.total_duration_secs : 0)
    };

    if (existingIndex >= 0) {
        memoryHistory[existingIndex] = entry;
    } else {
        memoryHistory.unshift(entry);
    }
    memoryHistory = sortNewestFirst(memoryHistory).slice(0, MAX_HISTORY_ENTRIES);

    persistEntry(entry);

    // Also add to watch logs
    addWatchLog(
        file.name,
        status === 'started' ? 'PLAY_START' : status === 'completed' ? 'COMPLETED' : 'RESUME',
        `Playback ${status} for ${file.name}${qualityTag ? ` [${qualityTag}]` : ''}`
    );

    return entry;
}

/**
 * Remove an entry from recent watch history
 */
export function removeWatchEntry(fileId: number): void {
    memoryHistory = memoryHistory.filter(item => item.file_id !== fileId);
    invoke('cmd_watch_history_remove', { fileId })
        .catch((e) => console.error('Failed to remove watch entry:', e));
}

/**
 * Clear all recent watch history
 */
export function clearWatchHistory(): void {
    memoryHistory = [];
    invoke('cmd_watch_history_clear')
        .catch((e) => console.error('Failed to clear watch history:', e));
}

/**
 * Get all watch activity logs
 */
export function getWatchLogs(): WatchLogEvent[] {
    try {
        const raw = localStorage.getItem(STORAGE_KEY_LOGS);
        if (!raw) return [];
        const logs: WatchLogEvent[] = JSON.parse(raw);
        return logs.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
    } catch {
        return [];
    }
}

/**
 * Add a new log entry to Watch History Logs (separated from error logs)
 */
export function addWatchLog(fileName: string, eventType: WatchLogEvent['event_type'], details: string): void {
    const logs = getWatchLogs();
    const newLog: WatchLogEvent = {
        id: `wlog_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`,
        timestamp: new Date().toISOString(),
        file_name: fileName,
        event_type: eventType,
        details
    };

    logs.unshift(newLog);
    const trimmed = logs.slice(0, MAX_LOG_ENTRIES);

    try {
        localStorage.setItem(STORAGE_KEY_LOGS, JSON.stringify(trimmed));
    } catch (e) {
        console.error('Failed to save watch log:', e);
    }
}

/**
 * Clear all watch logs
 */
export function clearWatchLogs(): void {
    try {
        localStorage.removeItem(STORAGE_KEY_LOGS);
    } catch (e) {
        console.error('Failed to clear watch logs:', e);
    }
}

/**
 * Export watch logs as a formatted JSON or text string
 */
export function exportWatchLogsText(): string {
    const logs = getWatchLogs();
    return logs.map(l => `[${l.timestamp}] [${l.event_type}] ${l.file_name} -> ${l.details}`).join('\n');
}
