import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { FileExplorer } from './FileExplorer';
import { TelegramFile } from '../../../types';
import { WatchHistoryEntry } from '../../../utils/watchHistory';

vi.mock('../../../context/SettingsContext', () => ({ useSettings: () => ({ settings: { language: 'en' } }) }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../../hooks/useVideoMetadata', () => ({ useVideoMetadata: () => ({ data: null, isLoading: false }) }));
vi.mock('../../../hooks/useVideoSubtitles', () => ({ useVideoSubtitles: () => ({ data: [] }) }));
vi.mock('./AttachSubtitlesModal', () => ({ AttachSubtitlesModal: () => null }));
vi.mock('./ManageSubtitlesModal', () => ({ ManageSubtitlesModal: () => null }));
vi.mock('./ContextMenu', () => ({ ContextMenu: () => null }));
vi.mock('@tanstack/react-virtual', () => ({
    useVirtualizer: ({ count, estimateSize }: { count: number; estimateSize: () => number }) => ({
        getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ index, key: index, start: index * estimateSize() })),
        getTotalSize: () => count * estimateSize(), scrollToOffset: () => {}, measure: () => {},
    }),
}));

const file = (id: number, name: string, folder_id: number | null = 7): TelegramFile => ({ id, name, folder_id, size: 1, sizeStr: '1 B', type: 'file' });
const entry = (file_id: number, folder_id: number, file_name: string): WatchHistoryEntry => ({
    id: `${folder_id}:${file_id}`, file_id, folder_id, file_name, file_size: 1, timestamp: '2026-10-29T10:00:00Z', status: 'paused',
});
const props = {
    loading: false, error: null, viewMode: 'list' as const, selectedIds: [], activeFolderId: 7,
    onFileClick: vi.fn(), onDelete: vi.fn(), onDownload: vi.fn(), onPreview: vi.fn(), onShowDetails: vi.fn(),
    onManualUpload: vi.fn(), onToggleSelection: vi.fn(), cardScale: 1, onCardScaleChange: vi.fn(),
    searchTerm: '', onClearSearch: vi.fn(), onRetry: vi.fn(),
};

beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async command => command === 'cmd_get_folder_view_prefs' ? { sort_field: 'name', sort_direction: 'asc' } : undefined);
    vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.clearAllMocks(); });

describe('library presentations', () => {
    it('keeps selected rows and callbacks source-aware when message numbers collide', () => {
        const first = file(7, 'First movie.mkv', 10);
        const second = file(7, 'Second movie.mkv', 20);
        render(<FileExplorer {...props} folderKind="general" isAllFavoritesView
            selectedIds={['20:7']} files={[first, second]} />);
        const firstRow = screen.getByRole('button', { name: 'Select file First movie.mkv' });
        const secondRow = screen.getByRole('button', { name: 'Select file Second movie.mkv' });
        expect(firstRow.classList.contains('is-selected')).toBe(false);
        expect(secondRow.classList.contains('is-selected')).toBe(true);
        fireEvent.click(firstRow);
        expect(props.onFileClick).toHaveBeenCalledWith(expect.anything(), 7, 10);
    });

    it('keeps one episode and accompanying files accessible through series navigation', async () => {
        const files = [file(1, 'Show S01E01.mkv'), file(2, 'Extras.mkv'), file(3, 'Show S01E01.srt')];
        render(<FileExplorer {...props} folderKind="series" files={files} />);
        expect(screen.getByRole('button', { name: /All files \(3\)/ })).toBeTruthy();
        fireEvent.click(screen.getByRole('button', { name: /Season 1 \(1\)/ }));
        expect(screen.getByText('Show S01E01.mkv')).toBeTruthy();
        expect(screen.queryByText('Extras.mkv')).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: /Other files \(2\)/ }));
        expect(screen.getByText('Extras.mkv')).toBeTruthy();
        expect(screen.getByText('Show S01E01.srt')).toBeTruthy();
        fireEvent.click(screen.getByRole('button', { name: /All files \(3\)/ }));
        await waitFor(() => expect(screen.getByText('Show S01E01.mkv')).toBeTruthy());
    });

    it('never offers series controls in movies and exposes Play and Details', () => {
        const movie = file(1, 'Show S01E01.mkv');
        render(<FileExplorer {...props} folderKind="movies" files={[movie]} watchHistory={[entry(1, 7, movie.name)]} />);
        expect(screen.queryByText(/Season 1|Binge Series|Next Up/)).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: `Play ${movie.name}` }));
        expect(props.onPreview).toHaveBeenCalledWith(movie, [movie]);
        fireEvent.click(screen.getByRole('button', { name: `Details ${movie.name}` }));
        expect(props.onShowDetails).toHaveBeenCalledWith(movie);
    });

    it('never offers Resume or Next for a same-ID entry in another folder', () => {
        const episodes = [file(1, 'Show S01E01.mkv'), file(2, 'Show S01E02.mkv')];
        render(<FileExplorer {...props} folderKind="series" files={episodes} watchHistory={[entry(1, 8, episodes[0].name)]} />);
        expect(screen.queryByText(/Resume|Next Up/)).toBeNull();
    });

    it('uses the current folder episode order for Resume and Next', () => {
        const e1 = file(1, 'Show S01E01.mkv');
        const e2 = file(2, 'Show S01E02.mkv');
        const otherFolder = file(3, 'Show S01E03.mkv', 8);
        render(<FileExplorer {...props} folderKind="series" files={[e2, otherFolder, e1]} watchHistory={[entry(1, 7, e1.name)]} />);
        fireEvent.click(screen.getByRole('button', { name: /Resume S01E01/ }));
        expect(props.onPreview).toHaveBeenLastCalledWith(e1, [e1, e2]);
        fireEvent.click(screen.getByRole('button', { name: /Next Up/ }));
        expect(props.onPreview).toHaveBeenLastCalledWith(e2, [e1, e2]);
    });

    it('uses image previews for image files and a title fallback for videos in poster mode', async () => {
        vi.mocked(invoke).mockImplementation(async command => {
            if (command === 'cmd_get_folder_view_prefs') return { sort_field: 'name', sort_direction: 'asc' };
            if (command === 'cmd_get_preview') return 'http://localhost:1234/real-image';
            return undefined;
        });
        const poster = file(1, 'cover.jpg');
        const movie = file(2, 'Movie.mkv');
        const { container } = render(<FileExplorer {...props} viewMode="posters" folderKind="movies" files={[poster, movie]} />);
        await waitFor(() => expect(container.querySelector('img')?.getAttribute('src')).toBe('http://localhost:1234/real-image'));
        expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'cmd_get_preview')).toEqual([
            ['cmd_get_preview', { messageId: poster.id, folderId: 7 }],
        ]);
        expect(screen.getByText('Movie.mkv')).toBeTruthy();
        fireEvent.click(screen.getByRole('button', { name: 'Play Movie.mkv' }));
        expect(props.onPreview).toHaveBeenCalledWith(movie, [poster, movie]);
    });

    it('keeps keyboard selection, favorites, tags and filters available in series rows', () => {
        const episode = file(1, 'Show S01E01.mkv');
        const toggleFavorite = vi.fn();
        const filterFavorites = vi.fn();
        const filterTag = vi.fn();
        render(<FileExplorer {...props} folderKind="series" files={[episode]} favoriteIds={new Set([1])}
            onToggleFavorite={toggleFavorite} onFavoritesOnlyChange={filterFavorites} availableTags={['Drama']}
            activeTagFilter="Drama" onActiveTagFilterChange={filterTag} tagMap={{ 1: ['Drama'] }} />);
        fireEvent.keyDown(screen.getByRole('button', { name: 'Select file Show S01E01.mkv' }), { key: 'Enter' });
        expect(props.onFileClick).toHaveBeenCalledTimes(1);
        fireEvent.click(screen.getByRole('button', { name: 'Unfavorite Show S01E01.mkv' }));
        expect(toggleFavorite).toHaveBeenCalledWith(episode);
        expect(props.onFileClick).toHaveBeenCalledTimes(1);
        fireEvent.click(screen.getByRole('button', { name: 'Favorites' }));
        expect(filterFavorites).toHaveBeenCalledWith(true);
        fireEvent.click(screen.getByRole('button', { name: 'Drama' }));
        expect(filterTag).toHaveBeenCalledWith(null);
    });
});
