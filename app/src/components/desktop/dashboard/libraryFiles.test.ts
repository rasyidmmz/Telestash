import { describe, expect, it } from 'vitest';
import { TelegramFile } from '../../../types';
import { WatchHistoryEntry } from '../../../utils/watchHistory';
import { getFolderWatchProgress, getSeriesSections } from './libraryFiles';

const file = (id: number, name: string, folder_id: number | null = 7): TelegramFile => ({
    id, name, folder_id, size: 1, sizeStr: '1 B', type: 'file',
});
const history = (file_id: number, folder_id: number | null, file_name: string, timestamp: string): WatchHistoryEntry => ({
    id: `${folder_id}:${file_id}`, file_id, folder_id, file_name, timestamp, file_size: 1, status: 'paused',
});

describe('series library', () => {
    it('retains every file with one recognized episode, including sidecars and folders', () => {
        const episode = file(1, 'Show S01E01.mkv');
        const otherVideo = file(2, 'Behind the scenes.mkv');
        const subtitle = file(3, 'Show S01E01.srt');
        const folder = { ...file(4, 'Extras'), type: 'folder' as const };
        const files = [episode, otherVideo, subtitle, folder];
        const sections = getSeriesSections(files);
        expect(sections.find(s => s.seasonKey === 'all')?.files).toEqual(files);
        expect(sections.find(s => s.seasonKey === 's1')?.files).toEqual([episode]);
        expect(sections.find(s => s.seasonKey === 'other')?.files).toEqual([otherVideo, subtitle, folder]);
    });

    it('keeps all files accessible when no episode is recognized', () => {
        const files = [file(1, 'Untitled.mkv'), file(2, 'Notes.txt')];
        const sections = getSeriesSections(files);
        expect(sections.map(s => s.seasonKey)).toEqual(['all', 'other']);
        expect(sections[1].files).toEqual(files);
    });

    it('matches both folder and file and uses naturally ordered same-folder episodes', () => {
        const e1 = file(1, 'Show S01E01.mkv');
        const e2 = file(2, 'Show S01E02.mkv');
        const e10 = file(10, 'Show S01E10.mkv');
        const foreign = file(2, 'Show S01E02.mkv', 8);
        const entries = [
            history(1, 8, e1.name, '2026-10-29T10:00:00Z'),
            history(99, 7, e1.name, '2026-10-29T09:00:00Z'),
            history(1, 7, e1.name, '2026-10-29T08:00:00Z'),
        ];
        const result = getFolderWatchProgress(entries, [e10, foreign, e2, e1], 7);
        expect(result?.lastWatchedFile).toBe(e1);
        expect(result?.nextEpisodeFile).toBe(e2);
        expect(result?.orderedEpisodes).toEqual([e1, e2, e10]);
        expect(getFolderWatchProgress(entries.slice(0, 2), [e1, foreign], 7)).toBeNull();
    });

    it('matches the home folder without accepting another folder or a same-name fallback', () => {
        const home = file(4, 'Show S01E01.mkv', null);
        const next = file(5, 'Show S01E02.mkv', null);
        expect(getFolderWatchProgress([history(4, 8, home.name, '2026-10-29T10:00:00Z')], [home, next], null)).toBeNull();
        expect(getFolderWatchProgress([history(4, null, home.name, '2026-10-29T10:00:00Z')], [home, next], null)?.nextEpisodeFile).toBe(next);
    });
});
