import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import type { TelegramFile } from '../types';

const mockedInvoke = vi.mocked(invoke);
const row = (folder: number | null, timestamp: number) => ({
    file_id: 12, file_name: 'video.mkv', folder_id: folder, file_size: 100,
    timestamp, status: 'started', quality_tag: null, last_position_secs: 5, total_duration_secs: 60,
});
const file = (folder: number | null): TelegramFile => ({
    id: 12, name: 'video.mkv', folder_id: folder, size: 100, sizeStr: '100 B',
});

describe('watch history folder identity', () => {
    beforeEach(() => {
        vi.resetModules();
        mockedInvoke.mockReset();
        localStorage.clear();
    });

    it('loads the same file ID independently for home and two folders', async () => {
        mockedInvoke.mockResolvedValue([row(null, 1000), row(20, 2000), row(30, 3000)]);
        const history = await import('./watchHistory');
        await history.initWatchHistory();
        expect(history.getRecentWatchHistory().map(item => item.folder_id)).toEqual([30, 20, null]);
        expect(new Set(history.getRecentWatchHistory().map(item => item.id)).size).toBe(3);
    });

    it('updates and removes only the matching folder and file pair', async () => {
        mockedInvoke.mockResolvedValue([]);
        const history = await import('./watchHistory');
        await history.initWatchHistory();
        history.recordWatchEvent(file(null), 'started', undefined, 5, 60);
        history.recordWatchEvent(file(20), 'started', undefined, 15, 60);
        history.recordWatchEvent(file(30), 'started', undefined, 25, 60);
        history.recordWatchEvent(file(20), 'paused', undefined, 17, 60);
        expect(history.getRecentWatchHistory().map(item => [item.folder_id, item.last_position_secs]).sort((a, b) => (a[0] ?? -1) - (b[0] ?? -1))).toEqual([
            [null, 5], [20, 17], [30, 25],
        ]);

        history.removeWatchEntry(12, 20);
        expect(history.getRecentWatchHistory().map(item => item.folder_id).sort((a, b) => (a ?? -1) - (b ?? -1))).toEqual([null, 30]);
        expect(mockedInvoke).toHaveBeenCalledWith('cmd_watch_history_remove', { fileId: 12, folderId: 20 });
    });

    it('merges legacy and backend entries by folder and file without dropping either', async () => {
        localStorage.setItem('telestash_recent_watch_v1', JSON.stringify([{
            id: 'old', file_id: 12, file_name: 'legacy.mkv', folder_id: 20, file_size: 100,
            timestamp: '1970-01-01T00:00:02.000Z', status: 'started',
        }]));
        mockedInvoke.mockImplementation(async command => command === 'cmd_watch_history_list' ? [row(null, 1000)] : 1);
        const history = await import('./watchHistory');
        await history.initWatchHistory();
        expect(history.getRecentWatchHistory().map(item => item.folder_id)).toEqual([20, null]);
        expect(localStorage.getItem('telestash_recent_watch_v1')).toBeNull();
    });
});
