import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { useFileDownload } from './useFileDownload';

vi.mock('../context/SettingsContext', () => ({
    useSettings: () => ({ settings: { maxConcurrentDownloads: 1 } }),
}));

const mockedInvoke = vi.mocked(invoke);
const mockedSave = vi.mocked(save);

function createStore() {
    return {
        get: vi.fn(async () => null),
        set: vi.fn(async () => {}),
        save: vi.fn(async () => {}),
    } as any;
}

describe('useFileDownload', () => {
    beforeEach(() => {
        mockedInvoke.mockReset();
        mockedSave.mockReset();
    });

    it('queues and completes a download selected through the save dialog', async () => {
        mockedSave.mockResolvedValue('C:/downloads/movie.mkv');
        mockedInvoke.mockResolvedValue(undefined as never);
        const { result } = renderHook(() => useFileDownload(createStore()));

        await act(async () => {
            result.current.queueDownload(42, 'movie.mkv', 7);
        });

        await waitFor(() => expect(result.current.downloadQueue[0]?.status).toBe('success'));
        expect(mockedInvoke).toHaveBeenCalledWith('cmd_download_file', {
            req: {
                message_id: 42,
                save_path: 'C:/downloads/movie.mkv',
                folder_id: 7,
                transfer_id: expect.any(String),
            },
        });
    });

    it('marks a failed download retryable and clears it on retry', async () => {
        mockedSave.mockResolvedValue('C:/downloads/bad.mkv');
        let attempts = 0;
        mockedInvoke.mockImplementation(async (cmd: string) => {
            if (cmd === 'cmd_download_file' && attempts++ === 0) throw new Error('network timeout');
            return undefined;
        });
        const { result } = renderHook(() => useFileDownload(createStore()));

        await act(async () => {
            result.current.queueDownload(9, 'bad.mkv', null);
        });
        await waitFor(() => expect(result.current.downloadQueue[0]?.status).toBe('error'));
        const id = result.current.downloadQueue[0].id;
        await act(async () => result.current.retryItem(id));
        expect(result.current.downloadQueue[0]).toMatchObject({ status: 'success', error: undefined });
    });
});
