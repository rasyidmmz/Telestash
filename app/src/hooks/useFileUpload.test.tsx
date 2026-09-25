import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

import { useFileUpload } from './useFileUpload';
import type { QueueItem } from '../types';

vi.mock('../context/SettingsContext', () => ({
    useSettings: () => ({ settings: { maxConcurrentUploads: 1 } }),
}));

const mockedInvoke = vi.mocked(invoke);

function wrapper({ children }: { children: React.ReactNode }) {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
    });
    return React.createElement(QueryClientProvider, { client }, children);
}

/** A store stub: the hook only calls `get`/`set`/`save` while restoring. */
function createStore() {
    return {
        get: vi.fn(async () => null),
        set: vi.fn(async () => {}),
        save: vi.fn(async () => {}),
        delete: vi.fn(async () => {}),
        clear: vi.fn(async () => {}),
        entries: vi.fn(async () => []),
        keys: vi.fn(async () => []),
        values: vi.fn(async () => []),
        has: vi.fn(async () => false),
        onKeyChange: vi.fn(async () => () => {}),
        onChange: vi.fn(async () => () => {}),
        reset: vi.fn(async () => {}),
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } as any;
}

/** Queue items the way the hook itself does, through the public setter. */
function queueFiles(
    result: { current: ReturnType<typeof useFileUpload> },
    paths: string[],
    folderId: number | null = null,
) {
    const items: QueueItem[] = paths.map((path, i) => ({
        id: `item-${i}-${path}`,
        path,
        folderId,
        status: 'pending' as const,
    }));
    result.current.setUploadQueue(prev => [...prev, ...items]);
    return items;
}

describe('useFileUpload', () => {
    beforeEach(() => {
        mockedInvoke.mockReset();
    });

    it('starts with an empty queue', async () => {
        mockedInvoke.mockResolvedValue(undefined as never);
        const { result } = renderHook(() => useFileUpload(null, createStore()), { wrapper });

        await waitFor(() => expect(result.current.uploadQueue).toEqual([]));
    });

    it('uploads a queued file and marks it successful', async () => {
        const store = createStore();
        mockedInvoke.mockImplementation(async (cmd: string) => {
            if (cmd === 'cmd_upload_file') return 'msg-1';
            if (cmd === 'cmd_sync_folder') return [];
            return undefined;
        });

        const { result } = renderHook(() => useFileUpload(7, store), { wrapper });

        await act(async () => {
            queueFiles(result, ['C:/media/movie.mkv'], 7);
        });

        await waitFor(
            () => {
                expect(result.current.uploadQueue[0]?.status).toBe('success');
            },
            { timeout: 5000 },
        );

        expect(mockedInvoke).toHaveBeenCalledWith('cmd_upload_file', {
            path: 'C:/media/movie.mkv',
            folderId: 7,
            transferId: expect.any(String),
        });
        // The folder cache is reconciled so the new file shows up immediately.
        expect(mockedInvoke).toHaveBeenCalledWith('cmd_sync_folder', { folderId: 7 });
    });

    it('marks the item cancelled and tells the backend to stop', async () => {
        const store = createStore();
        let rejectUpload: ((reason?: unknown) => void) | undefined;
        mockedInvoke.mockImplementation(async (cmd: string) => {
            if (cmd === 'cmd_upload_file') {
                return new Promise((_resolve, reject) => {
                    rejectUpload = reject;
                });
            }
            return undefined;
        });

        const { result } = renderHook(() => useFileUpload(1, store), { wrapper });

        await act(async () => {
            queueFiles(result, ['C:/media/big.iso'], 1);
        });

        await waitFor(() => expect(result.current.uploadQueue[0]?.status).toBe('uploading'));

        const id = result.current.uploadQueue[0].id;
        await act(async () => {
            result.current.cancelItem(id);
        });

        expect(mockedInvoke).toHaveBeenCalledWith('cmd_cancel_transfer', { transferId: id });

        // Let the in-flight upload settle so the hook does not leak a pending promise.
        await act(async () => {
            rejectUpload?.(new Error('Transfer cancelled'));
        });
    });

    it('leaves a completed item untouched when cancel is called', async () => {
        const store = createStore();
        mockedInvoke.mockImplementation(async (cmd: string) => {
            if (cmd === 'cmd_upload_file') return 'msg-2';
            return undefined;
        });

        const { result } = renderHook(() => useFileUpload(2, store), { wrapper });

        await act(async () => {
            queueFiles(result, ['C:/media/done.mkv'], 2);
        });
        await waitFor(
            () => expect(result.current.uploadQueue[0]?.status).toBe('success'),
            { timeout: 5000 },
        );

        const id = result.current.uploadQueue[0].id;
        await act(async () => {
            result.current.cancelItem(id);
        });

        // Regression guard: cancelling a finished upload must not rewrite its
        // status, and must not ask the backend to cancel anything.
        expect(result.current.uploadQueue[0].status).toBe('success');
        expect(mockedInvoke).not.toHaveBeenCalledWith('cmd_cancel_transfer', {
            transferId: id,
        });
    });

    it('drops pending items immediately on cancel', async () => {
        const store = createStore();
        mockedInvoke.mockImplementation(async (cmd: string) => {
            if (cmd === 'cmd_cancel_transfer') return true;
            return undefined;
        });

        const { result } = renderHook(() => useFileUpload(3, store), { wrapper });

        // Queue two files with concurrency 1: the second stays pending.
        await act(async () => {
            queueFiles(result, ['C:/media/a.mkv', 'C:/media/b.mkv'], 3);
        });

        await waitFor(() => expect(result.current.uploadQueue.length).toBe(2));

        const pending = result.current.uploadQueue.find(i => i.status === 'pending');
        if (pending) {
            await act(async () => {
                result.current.cancelItem(pending.id);
            });
            expect(result.current.uploadQueue.find(i => i.id === pending.id)).toBeUndefined();
        }
    });
});
