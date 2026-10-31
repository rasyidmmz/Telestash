import { describe, expect, it } from 'vitest';
import { groupRecentWatchEntries } from './seriesParser';
import type { WatchHistoryEntry } from './watchHistory';

const entry = (folderId: number, fileId: number, name: string): WatchHistoryEntry => ({
  id: `${folderId}:${fileId}`, folder_id: folderId, file_id: fileId,
  file_name: name, file_size: 100, timestamp: '2026-01-01T00:00:00Z', status: 'paused',
});

describe('continue watching folder identity', () => {
  it('groups series by folder and movies by folder and file', () => {
    const entries = [
      entry(10, 1, 'Show S01E02.mkv'),
      entry(10, 2, 'Show S01E01.mkv'),
      entry(11, 1, 'Show S01E01.mkv'),
      entry(20, 3, 'Movie A.mp4'),
      entry(20, 4, 'Movie B.mp4'),
      entry(21, 3, 'Movie C.mp4'),
    ];
    const presentations = {
      '10': { kind: 'series' as const }, '11': { kind: 'series' as const },
      '20': { kind: 'movies' as const }, '21': { kind: 'movies' as const },
    };

    expect(groupRecentWatchEntries(entries, presentations).map(e => e.id))
      .toEqual(['10:1', '11:1', '20:3', '20:4', '21:3']);
  });
});
