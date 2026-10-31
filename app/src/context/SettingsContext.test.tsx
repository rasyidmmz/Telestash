import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { load } from '@tauri-apps/plugin-store';
import { SettingsProvider, useSettings } from './SettingsContext';

function mockStore(saved: unknown) {
    const store = {
        get: vi.fn(async () => saved),
        set: vi.fn(async () => {}),
        save: vi.fn(async () => {}),
    };
    vi.mocked(load).mockResolvedValue(store as unknown as Awaited<ReturnType<typeof load>>);
    return store;
}

describe('folder presentation settings', () => {
    beforeEach(() => vi.mocked(load).mockReset());

    it('loads old settings, combines consecutive updates, and persists them by folder ID', async () => {
        const store = mockStore({ viewMode: 'list' });
        const { result, unmount } = renderHook(() => useSettings(), { wrapper: SettingsProvider });
        await waitFor(() => expect(result.current.isLoaded).toBe(true));
        expect(result.current.settings.folderPresentation).toEqual({});
        expect(typeof result.current.updateFolderPresentation).toBe('function');

        act(() => {
            result.current.updateFolderPresentation(42, { kind: 'series' });
            result.current.updateFolderPresentation(42, { viewMode: 'posters' });
            result.current.updateFolderPresentation(null, { kind: 'movies' });
        });
        const expected = { '42': { kind: 'series', viewMode: 'posters' }, home: { kind: 'movies' } };
        expect(result.current.settings.folderPresentation).toEqual(expected);
        await waitFor(() => expect(store.set).toHaveBeenCalledWith('settings', expect.objectContaining({ folderPresentation: expected })));

        act(() => result.current.updateFolderPresentation(42, { viewMode: undefined }));
        expect(result.current.settings.folderPresentation['42']).toEqual({ kind: 'series' });
        unmount();
    });

    it('discards malformed local entries while retaining valid folder preferences', async () => {
        mockStore({ folderPresentation: {
            home: { kind: 'movies', viewMode: 'posters' },
            '42': { kind: 'series', viewMode: 'unknown' },
            '43': { kind: 'series', viewMode: 'grid' },
            wrong: { kind: 'general' },
        } });
        const { result, unmount } = renderHook(() => useSettings(), { wrapper: SettingsProvider });
        await waitFor(() => expect(result.current.isLoaded).toBe(true));
        expect(result.current.settings.folderPresentation).toEqual({
            home: { kind: 'movies', viewMode: 'posters' },
            '43': { kind: 'series', viewMode: 'grid' },
        });
        unmount();
    });
});
