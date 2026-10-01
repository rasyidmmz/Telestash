import { describe, expect, it } from 'vitest';
import { recoverDownloadQueue, recoverUploadQueue } from './queueRecovery';

describe('queue recovery validation', () => {
    it('keeps valid uploads paused and removes invalid duplicates', () => {
        const result = recoverUploadQueue([
            { id: 'a', path: 'C:/a.mkv', folderId: 1, status: 'pending' },
            { id: 'a', path: 'C:/duplicate.mkv', folderId: 1, status: 'paused' },
            { id: 'bad', path: '', folderId: 1, status: 'pending' },
        ]);
        expect(result).toHaveLength(1);
        expect(result[0].status).toBe('paused');
    });

    it('keeps valid downloads paused and rejects malformed ids', () => {
        const result = recoverDownloadQueue([
            { id: 'd', messageId: 42, filename: 'movie.mkv', folderId: null, status: 'pending', savePath: 'C:/movie.mkv' },
            { id: 'bad', messageId: '42', filename: 'bad.mkv', folderId: null, status: 'pending' },
        ]);
        expect(result).toHaveLength(1);
        expect(result[0].status).toBe('paused');
    });
});
