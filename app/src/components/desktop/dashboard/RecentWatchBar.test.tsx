import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RecentWatchBar } from './RecentWatchBar';
import type { WatchHistoryEntry } from '../../../utils/watchHistory';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../../utils/watchHistory', () => ({ removeWatchEntry: vi.fn(), clearWatchHistory: vi.fn() }));

const entry = (folderId: number, fileId: number, fileName: string): WatchHistoryEntry => ({
  id: `${folderId}:${fileId}`, folder_id: folderId, file_id: fileId, file_name: fileName,
  file_size: 100, timestamp: '2026-01-01T00:00:00Z', status: 'paused',
});

describe('continue watching', () => {
  it('groups unrecognized episode names in a folder explicitly marked Series', () => {
    const entries = [entry(10, 1, 'First unlabelled video.mkv'), entry(10, 2, 'Second unlabelled video.mkv')];
    render(<RecentWatchBar entries={entries} currentFiles={[]} activeFolderId={10}
      folderPresentation={{ '10': { kind: 'series' } }} onPlay={() => {}} onRefresh={() => {}} />);
    expect(screen.getByRole('button', { name: 'Resume First unlabelled video.mkv' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Resume Second unlabelled video.mkv' })).toBeNull();
  });

  it('keeps movies with the same message number in different folders distinct', () => {
    const entries = [entry(20, 7, 'Movie A.mp4'), entry(21, 7, 'Movie B.mp4')];
    const onPlay = vi.fn();
    render(<RecentWatchBar entries={entries} currentFiles={[]} activeFolderId={20}
      folderPresentation={{ '20': { kind: 'movies' }, '21': { kind: 'movies' } }}
      onPlay={onPlay} onRefresh={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: 'Resume Movie B.mp4' }));
    expect(screen.getByRole('button', { name: 'Resume Movie A.mp4' })).toBeTruthy();
    expect(onPlay).toHaveBeenCalledWith(entries[1]);
  });
});
