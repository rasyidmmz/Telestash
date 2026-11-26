import { selectedFiles, groupFilesByFolder } from '../fileIdentity';
import { useCallback, useRef } from 'react';
import { showFileDialogFallback, pickWithFallback, sanitizeFilename } from '../utils';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useConfirm } from '../context/ConfirmContext';
import { TelegramFile } from '../types';
import { humanizeError } from '../utils/errorHumanizer';
import i18n from '../i18n';

export function useFileOperations(
    activeFolderId: number | null,
    selectedIds: string[],
    setSelectedIds: (ids: string[]) => void,
    displayedFiles: TelegramFile[],
    queueBulkDownload?: (files: TelegramFile[], folderId: number | null) => void,
) {
    const queryClient = useQueryClient();
    const { confirm } = useConfirm();

    // Refs to keep callbacks stable even when selection/file list changes
    const selectedIdsRef = useRef(selectedIds);
    selectedIdsRef.current = selectedIds;
    const displayedFilesRef = useRef(displayedFiles);
    displayedFilesRef.current = displayedFiles;

    const handleDelete = useCallback(async (id: number, folderId: number | null = activeFolderId) => {
        if (!await confirm({ title: "Delete File", message: "Are you sure you want to delete this file?", confirmText: "Delete", variant: 'danger' })) return;
        try {
            await invoke('cmd_delete_file', { messageId: id, folderId });
            await invoke('cmd_delete_preview_for_message', { messageId: id, folderId }).catch(() => {});
            // Reconcile the folder cache, then refresh the list from it.
            await invoke('cmd_sync_folder', { folderId }).catch(() => {});
            queryClient.invalidateQueries({ queryKey: ['files', folderId] });
            queryClient.invalidateQueries({ queryKey: ['all-favorite-files'] });
            toast.success("File deleted");
        } catch (e) {
            toast.error(humanizeError(e, i18n.t('errors.action_delete')));
        }
    }, [activeFolderId, confirm, queryClient]);

    const handleBulkDelete = useCallback(async () => {
        const ids = selectedIdsRef.current;
        if (ids.length === 0) return;
        if (!await confirm({ title: "Delete Files", message: `Are you sure you want to delete ${ids.length} files?`, confirmText: "Delete All", variant: 'danger' })) return;

        let success = 0;
        let fail = 0;
        const targets = selectedFiles(displayedFilesRef.current, ids);
        for (const file of targets) {
            const folderId = file.folder_id ?? null;
            try {
                await invoke('cmd_delete_file', { messageId: file.id, folderId });
                await invoke('cmd_delete_preview_for_message', { messageId: file.id, folderId }).catch(() => {});
                success++;
            } catch { fail++; }
        }
        setSelectedIds([]);
        for (const folderId of groupFilesByFolder(targets).keys()) {
            await invoke('cmd_sync_folder', { folderId }).catch(() => {});
            queryClient.invalidateQueries({ queryKey: ['files', folderId] });
        }
        queryClient.invalidateQueries({ queryKey: ['all-favorite-files'] });
        if (success > 0) toast.success(`Deleted ${success} files.`);
        if (fail > 0) toast.error(`Failed to delete ${fail} files.`);
    }, [activeFolderId, confirm, queryClient, setSelectedIds]);

    const handleBulkDownload = useCallback(async () => {
        const ids = selectedIdsRef.current;
        if (ids.length === 0) return;
        const currentFiles = displayedFilesRef.current;
        const targetFiles = selectedFiles(currentFiles, ids);
        if (targetFiles.length === 0) return;
        if (queueBulkDownload) {
            queueBulkDownload(targetFiles, activeFolderId);
            setSelectedIds([]);
            return;
        }
        // Fallback: direct download if queue not provided
        const downloadToDir = async (dirPath: string) => {
            let successCount = 0;
            const sep = dirPath.includes('\\') ? '\\' : '/';
            for (const file of targetFiles) {
                const sanitizedName = sanitizeFilename(file.name);
                const filePath = dirPath.endsWith(sep) ? `${dirPath}${sanitizedName}` : `${dirPath}${sep}${sanitizedName}`;
                try {
                    await invoke('cmd_download_file', { req: { message_id: file.id, save_path: filePath, folder_id: file.folder_id ?? null } });
                    successCount++;
                } catch (e) { }
            }
            toast.success(`Downloaded ${successCount} files.`);
            setSelectedIds([]);
        };
        try {
            const dirPath = await pickWithFallback(
                () => open({ directory: true, multiple: false, title: "Select Download Destination" }),
                () => handleBulkDownload(),
                {
                    errorTitle: 'Folder picker failed',
                    onBrowserPicker: async () => {
                        const paths = await showFileDialogFallback({ directory: true, multiple: false });
                        if (paths.length === 0) return null;
                        const sep = paths[0].includes('\\') ? '\\' : '/';
                        return paths[0].substring(0, paths[0].lastIndexOf(sep));
                    },
                },
            );
            if (!dirPath) return;
            await downloadToDir(dirPath);
        } catch (e) {
            toast.error(humanizeError(e, i18n.t('errors.action_bulk_download')));
        }
    }, [activeFolderId, setSelectedIds, queueBulkDownload]);

    const handleBulkMove = useCallback(async (targetFolderId: number | null, onSuccess?: () => void) => {
        const targets = selectedFiles(displayedFilesRef.current, selectedIdsRef.current);
        if (!targets.length) return;
        let moved = 0;
        let failed = 0;
        for (const [sourceFolderId, files] of groupFilesByFolder(targets)) {
            if (sourceFolderId === targetFolderId) continue;
            try {
                await invoke('cmd_move_files', { messageIds: files.map(file => file.id), sourceFolderId, targetFolderId });
                await Promise.all(files.map(file => invoke('cmd_delete_preview_for_message', { messageId: file.id, folderId: sourceFolderId }).catch(() => {})));
                await invoke('cmd_sync_folder', { folderId: sourceFolderId }).catch(() => {});
                queryClient.invalidateQueries({ queryKey: ['files', sourceFolderId] });
                moved += files.length;
            } catch { failed += files.length; }
        }
        await invoke('cmd_sync_folder', { folderId: targetFolderId }).catch(() => {});
        queryClient.invalidateQueries({ queryKey: ['files', targetFolderId] });
        queryClient.invalidateQueries({ queryKey: ['all-favorite-files'] });
        if (moved) toast.success(`Moved ${moved} files.`);
        if (failed) toast.error(`Failed to move ${failed} files.`);
        if (!failed) { setSelectedIds([]); onSuccess?.(); }
    }, [queryClient, setSelectedIds]);

    const handleDownloadFolder = useCallback(async () => {
        const files = displayedFilesRef.current;
        if (files.length === 0) {
            toast.info("Folder is empty.");
            return;
        }
        if (queueBulkDownload) {
            queueBulkDownload(files, activeFolderId);
            return;
        }
        // Fallback: direct download if queue not provided
        const downloadToDir = async (dirPath: string) => {
            let successCount = 0;
            toast.info(`Downloading folder contents (${files.length} files)...`);
            const sep = dirPath.includes('\\') ? '\\' : '/';
            for (const file of files) {
                const sanitizedName = sanitizeFilename(file.name);
                const filePath = dirPath.endsWith(sep) ? `${dirPath}${sanitizedName}` : `${dirPath}${sep}${sanitizedName}`;
                try {
                    await invoke('cmd_download_file', { req: { message_id: file.id, save_path: filePath, folder_id: file.folder_id ?? null } });
                    successCount++;
                } catch (e) { }
            }
            toast.success(`Folder Download Complete: ${successCount} files.`);
        };
        try {
            const dirPath = await pickWithFallback(
                () => import('@tauri-apps/plugin-dialog').then(d => d.open({
                    directory: true, multiple: false, title: "Download Folder To..."
                })),
                () => handleDownloadFolder(),
                {
                    errorTitle: 'Folder picker failed',
                    onBrowserPicker: async () => {
                        const paths = await showFileDialogFallback({ directory: true, multiple: false });
                        if (paths.length === 0) return null;
                        const sep = paths[0].includes('\\') ? '\\' : '/';
                        return paths[0].substring(0, paths[0].lastIndexOf(sep));
                    },
                },
            );
            if (!dirPath) return;
            await downloadToDir(dirPath);
        } catch (e) {
            toast.error(humanizeError(e, i18n.t('errors.action_bulk_download')));
        }
    }, [activeFolderId, queueBulkDownload]);

    const handleGlobalSearch = useCallback(async (query: string) => {
        try {
            // Local cache first: instant and no longer capped at 50 results.
            // Folders never opened are not cached yet, so fall back to the
            // Telegram-wide search when the local query comes up empty.
            const local = await invoke<TelegramFile[]>('cmd_search_cached_files', { query });
            if (local.length > 0) return local;
            return await invoke<TelegramFile[]>('cmd_search_global', { query });
        } catch {
            return [];
        }
    }, []);

    return {
        handleDelete,
        handleBulkDelete,
        handleBulkDownload,
        handleBulkMove,
        handleDownloadFolder,
        handleGlobalSearch,
    };
}
