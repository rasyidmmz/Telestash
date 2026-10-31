import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileOperations } from './useFileOperations';

const { invoke, invalidateQueries, confirm } = vi.hoisted(() => ({
    invoke: vi.fn().mockResolvedValue(undefined), invalidateQueries: vi.fn(), confirm: vi.fn().mockResolvedValue(true),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tanstack/react-query', () => ({ useQueryClient: () => ({ invalidateQueries }) }));
vi.mock('../context/ConfirmContext', () => ({ useConfirm: () => ({ confirm }) }));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

const files = [
    { id: 7, folder_id: 10, name: 'One.mkv', size: 1, sizeStr: '1 B' },
    { id: 7, folder_id: 20, name: 'Two.mkv', size: 1, sizeStr: '1 B' },
];

describe('source-aware file operations', () => {
    beforeEach(() => { vi.clearAllMocks(); });
    it('deletes only the selected source when message numbers collide', async () => {
        const { result } = renderHook(() => useFileOperations(null, ['20:7'], vi.fn(), files));
        await act(() => result.current.handleBulkDelete());
        expect(invoke.mock.calls.filter(([command]) => command === 'cmd_delete_file'))
            .toEqual([['cmd_delete_file', { messageId: 7, folderId: 20 }]]);
    });
    it('moves selected files using separate source-folder requests', async () => {
        const { result } = renderHook(() => useFileOperations(null, ['10:7', '20:7'], vi.fn(), files));
        await act(() => result.current.handleBulkMove(30));
        expect(invoke.mock.calls.filter(([command]) => command === 'cmd_move_files'))
            .toEqual([
                ['cmd_move_files', { messageIds: [7], sourceFolderId: 10, targetFolderId: 30 }],
                ['cmd_move_files', { messageIds: [7], sourceFolderId: 20, targetFolderId: 30 }],
            ]);
    });
    it('single-file deletion uses its explicit source rather than the active folder', async () => {
        const { result } = renderHook(() => useFileOperations(10, [], vi.fn(), files));
        await act(() => result.current.handleDelete(7, 20));
        expect(invoke).toHaveBeenCalledWith('cmd_delete_file', { messageId: 7, folderId: 20 });
    });
});
