import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { QueueItem } from '../types';
import { showFileDialogFallback, pickWithFallback } from '../utils';
import { useSettings } from '../context/SettingsContext';
import type { Store } from '@tauri-apps/plugin-store';

interface ProgressPayload {
    id: string;
    percent: number;
    uploaded_bytes: number;
    total_bytes: number;
    speed_bytes_per_sec: number;
}

interface RemoteProgressPayload {
    id: string;
    phase: 'downloading' | 'uploading';
    percent: number;
    speed: number;
    uploaded_bytes: number;
    total_bytes: number;
}

export function useFileUpload(activeFolderId: number | null, store: Store | null) {
    const queryClient = useQueryClient();
    const { settings } = useSettings();
    const [uploadQueue, setUploadQueue] = useState<QueueItem[]>([]);
    const [initialized, setInitialized] = useState(false);
    const cancelledRef = useRef<Set<string>>(new Set());
    const activeCountRef = useRef(0);

    // Listen for progress events from Rust
    useEffect(() => {
        let unlistenProgress: UnlistenFn | undefined;
        let unlistenRemote: UnlistenFn | undefined;

        listen<ProgressPayload>('upload-progress', (event) => {
            setUploadQueue(q => q.map(i =>
                i.id === event.payload.id ? {
                    ...i,
                    progress: event.payload.percent,
                    uploadedBytes: event.payload.uploaded_bytes,
                    totalBytes: event.payload.total_bytes,
                    speedBytesPerSec: event.payload.speed_bytes_per_sec,
                } : i
            ));
        }).then(fn => { unlistenProgress = fn; });

        listen<RemoteProgressPayload>('remote-upload-progress', (event) => {
            setUploadQueue(q => q.map(i =>
                i.id === event.payload.id ? {
                    ...i,
                    status: event.payload.phase,
                    progress: event.payload.percent,
                    speedBytesPerSec: event.payload.speed,
                    uploadedBytes: event.payload.uploaded_bytes,
                    totalBytes: event.payload.total_bytes,
                } : i
            ));
        }).then(fn => { unlistenRemote = fn; });

        return () => {
            unlistenProgress?.();
            unlistenRemote?.();
        };
    }, []);

    useEffect(() => {
        if (!store || initialized) return;
        store.get<QueueItem[]>('uploadQueue').then((saved) => {
            if (saved && saved.length > 0) {
                const resumable = saved.filter(i => i.status === 'pending' || i.status === 'paused');
                if (resumable.length > 0) {
                    setUploadQueue(resumable);
                    toast.info(`Restored ${resumable.length} upload queue items`);
                }
            }
            setInitialized(true);
        });
    }, [store, initialized]);

    useEffect(() => {
        if (!store || !initialized) return;
        const toSave = uploadQueue.filter(i => i.status === 'pending' || i.status === 'paused');
        store.set('uploadQueue', toSave).then(() => store.save());
    }, [store, uploadQueue, initialized]);

    // Process up to maxConcurrentUploads in parallel
    useEffect(() => {
        const maxConcurrent = settings.maxConcurrentUploads || 1;
        const available = maxConcurrent - activeCountRef.current;
        if (available <= 0) return;
        const pendingItems = uploadQueue.filter(i => i.status === 'pending').slice(0, available);
        for (const item of pendingItems) {
            processItem(item);
        }
    }, [uploadQueue, settings.maxConcurrentUploads]);

    /** Clean up temp zip file if the item was created from a folder */
    const cleanupTempZip = async (item: QueueItem) => {
        if (item.tempZipPath) {
            try {
                await invoke('cmd_delete_temp_zip', { path: item.tempZipPath });
            } catch {
                // Best-effort cleanup
            }
        }
    };

    const processItem = async (item: QueueItem) => {
        activeCountRef.current++;
        const initialStatus = item.url ? 'downloading' : 'uploading';
        setUploadQueue(q => q.map(i => i.id === item.id ? { ...i, status: initialStatus, progress: 0 } : i));
        try {
            if (item.url) {
                await invoke('cmd_upload_from_url', { url: item.url, folderId: item.folderId, transferId: item.id });
            } else {
                await invoke('cmd_upload_file', { path: item.path, folderId: item.folderId, transferId: item.id });
            }
            // Check if cancelled during upload
            if (cancelledRef.current.has(item.id)) {
                cancelledRef.current.delete(item.id);
            } else {
                setUploadQueue(q => q.map(i => i.id === item.id ? { ...i, status: 'success', progress: 100 } : i));
                queryClient.invalidateQueries({ queryKey: ['files', item.folderId] });
            }
            // Clean up temp zip on success
            await cleanupTempZip(item);
        } catch (e) {
            if (!cancelledRef.current.has(item.id)) {
                const errMsg = String(e);
                if (errMsg.includes('Transfer cancelled')) {
                    setUploadQueue(q => q.map(i => i.id === item.id ? { ...i, status: 'cancelled' } : i));
                } else if (errMsg.includes('FILE_TOO_BIG') || errMsg.includes('too large') || errMsg.includes('2 GB') || errMsg.includes('2GB')) {
                    setUploadQueue(q => q.map(i => i.id === item.id ? { ...i, status: 'error', error: errMsg } : i));
                    toast.error(`Upload failed: Telegram rejected this file or split part. ${errMsg}`);
                } else {
                    const displayPath = item.url || item.path;
                    setUploadQueue(q => q.map(i => i.id === item.id ? { ...i, status: 'error', error: errMsg } : i));
                    toast.error(`Upload failed for ${displayPath.split('/').pop()}: ${e}`);
                }
            } else {
                cancelledRef.current.delete(item.id);
            }
            // Clean up temp zip even on failure
            await cleanupTempZip(item);
        } finally {
            // ponytail: 2s cooldown to prevent Telegram flooding
            await new Promise(resolve => setTimeout(resolve, 2000));
            activeCountRef.current--;
            setUploadQueue(q => [...q]); // trigger queue effect to check for next item
        }
    };

    /** Queues a set of file paths for upload */
    const queueFiles = (paths: string[]) => {
        if (!paths || paths.length === 0) return;
        const newItems: QueueItem[] = paths.map((path: string) => ({
            id: Math.random().toString(36).substr(2, 9),
            path,
            folderId: activeFolderId,
            status: 'pending' as const,
        }));
        setUploadQueue(prev => [...prev, ...newItems]);
        toast.info(`Queued ${paths.length} file${paths.length !== 1 ? 's' : ''} for upload`);
    };

    const handleManualUpload = async () => {
        const paths = await pickWithFallback(
            async () => {
                const selected = await open({ multiple: true, directory: false });
                if (!selected) return null;
                return Array.isArray(selected) ? selected : [selected];
            },
            () => handleManualUpload(),
            {
                errorTitle: 'File picker failed',
                onBrowserPicker: async () => {
                    const fallbackPaths = await showFileDialogFallback({ directory: false, multiple: true });
                    return fallbackPaths.length > 0 ? fallbackPaths : null;
                },
            },
        );
        if (paths && paths.length > 0) {
            queueFiles(paths);
        }
    };

    const handleFolderUpload = async () => {
        const folderPath = await pickWithFallback(
            async () => {
                const selected = await open({ multiple: false, directory: true, title: 'Select Folder to Upload' });
                if (!selected) return null;
                const fp = Array.isArray(selected) ? selected[0] : selected;
                return fp || null;
            },
            () => handleFolderUpload(),
            {
                errorTitle: 'Folder picker failed',
                onBrowserPicker: async () => {
                    const fallbackPaths = await showFileDialogFallback({ directory: true, multiple: true });
                    if (fallbackPaths.length > 0) {
                        // HTML folder picker returns individual file paths, not a folder path.
                        // We can't zip without a folder path, so files upload individually.
                        toast.info('Folder picker fallback cannot read the folder path, uploading files individually.');
                        queueFiles(fallbackPaths);
                    }
                    return null; // Already handled via queueFiles — signal that the main flow should stop
                },
            },
        );
        if (!folderPath) return;

        const folderName = folderPath.split('/').pop() || folderPath.split('\\').pop() || 'folder';

        toast.info(`Zipping "${folderName}"...`);
        try {
            const zipPath = await invoke<string>('cmd_zip_folder', { folderPath });
            const item: QueueItem = {
                id: Math.random().toString(36).substr(2, 9),
                path: zipPath,
                folderId: activeFolderId,
                status: 'pending',
                tempZipPath: zipPath,
            };
            setUploadQueue(prev => [...prev, item]);
            toast.success(`Queued "${folderName}.zip" for upload`);
        } catch (e) {
            console.error('[Upload] Zip error:', e);
            toast.error(`Failed to zip folder: ${e}`);
        }
    };

    const cancelAll = () => {
        setUploadQueue(q => {
            const activeItems = q.filter(i => i.status === 'uploading' || i.status === 'downloading' || i.status === 'paused');
            for (const item of activeItems) {
                cancelledRef.current.add(item.id);
                invoke('cmd_cancel_transfer', { transferId: item.id }).catch(() => {});
            }
            return q
                .filter(i => i.status !== 'pending')
                .map(i => (i.status === 'uploading' || i.status === 'downloading' || i.status === 'paused') ? { ...i, status: 'cancelled' as const } : i);
        });
        toast.info('All uploads cancelled');
    };

    const cancelItem = (id: string) => {
        setUploadQueue(q => {
            const item = q.find(i => i.id === id);
            if (item?.status === 'uploading' || item?.status === 'downloading' || item?.status === 'paused') {
                cancelledRef.current.add(id);
                invoke('cmd_cancel_transfer', { transferId: id }).catch(() => {});
                return q.map(i => i.id === id ? { ...i, status: 'cancelled' as const } : i);
            }
            // Remove pending items directly
            if (item?.status === 'pending') {
                return q.filter(i => i.id !== id);
            }
            return q;
        });
    };

    const pauseUpload = async (id: string) => {
        const target = uploadQueue.find(i => i.id === id);
        if (!target) return;
        if (target.status === 'pending') {
            setUploadQueue(q => q.map(i => i.id === id ? { ...i, status: 'paused' as const } : i));
            return;
        }
        if (target.status === 'uploading' || target.status === 'downloading') {
            try {
                await invoke('cmd_pause_transfer', { transferId: id });
                setUploadQueue(q => q.map(i => i.id === id ? { ...i, status: 'paused' as const, speedBytesPerSec: 0 } : i));
            } catch (e) {
                console.error('[Upload] Pause error:', e);
            }
        }
    };

    const resumeUpload = async (id: string) => {
        const target = uploadQueue.find(i => i.id === id);
        if (!target) return;
        if (target.status === 'paused') {
            try {
                await invoke('cmd_resume_transfer', { transferId: id });
            } catch {
                // Ignore if backend wasn't in active wait
            }
            setUploadQueue(q => q.map(i => i.id === id ? { ...i, status: 'pending' as const } : i));
        }
    };

    const pauseAll = async () => {
        const itemsToPause = uploadQueue.filter(i => i.status === 'uploading' || i.status === 'downloading' || i.status === 'pending');
        for (const item of itemsToPause) {
            await pauseUpload(item.id);
        }
        toast.info('All uploads paused');
    };

    const resumeAll = async () => {
        const itemsToResume = uploadQueue.filter(i => i.status === 'paused');
        for (const item of itemsToResume) {
            await resumeUpload(item.id);
        }
        toast.info('All uploads resumed');
    };

    const retryItem = (id: string) => {
        setUploadQueue(q => q.map(i =>
            i.id === id && (i.status === 'error' || i.status === 'cancelled')
                ? { ...i, status: 'pending' as const, error: undefined, progress: undefined, uploadedBytes: undefined, totalBytes: undefined, speedBytesPerSec: undefined }
                : i
        ));
    };

    const handleUrlUpload = (url: string, folderId: number | null) => {
        if (!url || !url.trim()) return;
        let filename: string;
        try {
            filename = new URL(url).pathname.split('/').pop() || 'remote_file';
        } catch {
            filename = url.split('/').pop() || 'remote_file';
        }
        const item: QueueItem = {
            id: Math.random().toString(36).substr(2, 9),
            path: filename,
            url: url.trim(),
            folderId: folderId,
            status: 'pending' as const,
        };
        setUploadQueue(prev => [...prev, item]);
        toast.info(`Queued remote upload from URL`);
    };

    return {
        uploadQueue,
        setUploadQueue,
        handleManualUpload,
        handleFolderUpload,
        handleUrlUpload,
        cancelAll,
        cancelItem,
        pauseUpload,
        resumeUpload,
        pauseAll,
        resumeAll,
        retryItem,
    };
}
