import { useState, useEffect, useRef } from 'react';
import { X, File, ChevronLeft, ChevronRight } from '../../shared/icons.tsx';
import { invoke } from '@tauri-apps/api/core';
import { TelegramFile } from '../../../types';
import { isImageFile } from '../../../utils';
import { useModalDialog } from '../../../hooks/useModalDialog';

const PREVIEW_CACHE_TTL_MS = 5 * 60 * 1000;
const PREVIEW_CACHE_MAX_ITEMS = 8;

type PreviewCacheValue = {
    src: string;
    cachedAt: number;
};

const previewCache = new Map<string, PreviewCacheValue>();
const pendingPrefetch = new Set<string>();

const getPreviewCacheKey = (fileId: number, folderId: number | null) => `${folderId ?? 'home'}:${fileId}`;

const touchPreviewCache = (key: string, value: PreviewCacheValue) => {
    if (previewCache.has(key)) previewCache.delete(key);
    previewCache.set(key, value);

    while (previewCache.size > PREVIEW_CACHE_MAX_ITEMS) {
        const oldestKey = previewCache.keys().next().value;
        if (!oldestKey) break;
        previewCache.delete(oldestKey);
    }
};

const getCachedPreview = (key: string): string | null => {
    const value = previewCache.get(key);
    if (!value) return null;

    if (Date.now() - value.cachedAt > PREVIEW_CACHE_TTL_MS) {
        previewCache.delete(key);
        return null;
    }

    touchPreviewCache(key, value);
    return value.src;
};

const rememberPreview = (key: string, src: string) => {
    touchPreviewCache(key, { src, cachedAt: Date.now() });
};

const forgetPreview = (key: string) => {
    previewCache.delete(key);
};

const isSafeToPrefetch = (name: string) => isImageFile(name);

interface PreviewModalProps {
    file: TelegramFile;
    onClose: () => void;
    onNext?: () => void;
    onPrev?: () => void;
    currentIndex?: number;
    totalItems?: number;
    nextFile?: TelegramFile | null;
    prevFile?: TelegramFile | null;
    activeFolderId: number | null;
}

export function PreviewModal({ file, onClose, onNext, onPrev, currentIndex, totalItems, nextFile, prevFile, activeFolderId }: PreviewModalProps) {
    const [src, setSrc] = useState<string | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const latestRequestRef = useRef(0);
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const dialogRef = useModalDialog(true, onClose, closeButtonRef);

    useEffect(() => {
        const load = async () => {
            const key = getPreviewCacheKey(file.id, activeFolderId);
            const requestId = ++latestRequestRef.current;
            const cachedSrc = getCachedPreview(key);

            if (cachedSrc) {
                if (requestId !== latestRequestRef.current) return;
                setSrc(cachedSrc);
                setLoading(false);
                setError(null);
                return;
            }

            setLoading(true);
            setError(null);
            try {
                const path = await invoke<string>('cmd_get_preview', {
                    messageId: file.id,
                    folderId: activeFolderId
                });
                if (requestId !== latestRequestRef.current) return;

                if (path) {
                    // Image preview returns an HTTP URL (served by the local
                    // streaming server with token auth); non-image paths
                    // (e.g. archives for cmd_open_file_externally) are returned
                    // as filesystem paths by the backend.
                    if (path.startsWith('http://') || path.startsWith('https://')) {
                        setSrc(path);
                        rememberPreview(key, path);
                    } else {
                        // Non-image preview: MediaPlayer / open-externally
                        // path is handled elsewhere — nothing to render here.
                        setSrc('');
                        setError('Preview not available for this file type');
                    }
                } else {
                    setError('Preview not available');
                }
            } catch (e) {
                if (requestId !== latestRequestRef.current) return;
                setError(String(e));
            } finally {
                if (requestId !== latestRequestRef.current) return;
                setLoading(false);
            }
        };
        load();
    }, [file, activeFolderId]);

    useEffect(() => {
        const candidates = [nextFile, prevFile].filter((f): f is TelegramFile => !!f && isSafeToPrefetch(f.name));

        candidates.forEach((candidate) => {
            const key = getPreviewCacheKey(candidate.id, activeFolderId);
            if (getCachedPreview(key) || pendingPrefetch.has(key)) return;

            pendingPrefetch.add(key);
            invoke<string>('cmd_get_preview', {
                messageId: candidate.id,
                folderId: activeFolderId
            }).then((path) => {
                if (!path) return;
                if (path.startsWith('http://') || path.startsWith('https://')) {
                    rememberPreview(key, path);
                }
            }).catch(() => {
                // Ignore prefetch errors, main preview flow will handle user-visible failures.
            }).finally(() => {
                pendingPrefetch.delete(key);
            });
        });
    }, [nextFile, prevFile, activeFolderId]);

    useEffect(() => {
        const handleKeyDown = (e: KeyboardEvent) => {
            const target = e.target as HTMLElement;
            if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
                return;
            }

            const key = e.key.toLowerCase();

            if (e.key === 'ArrowRight' || key === 'l') {
                e.preventDefault();
                onNext?.();
                return;
            }

            if (e.key === 'ArrowLeft' || key === 'j') {
                e.preventDefault();
                onPrev?.();
                return;
            }

        };

        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [onClose, onNext, onPrev]);

    return (
        <div className="fixed inset-0 z-[150] bg-black/90 flex items-center justify-center p-4 backdrop-blur-sm" onClick={onClose}>
            <div ref={dialogRef} tabIndex={-1} role="dialog" aria-modal="true" aria-label={`Preview: ${file.name}`} className="w-full max-w-6xl h-full flex flex-col" onClick={e => e.stopPropagation()}>
                <div className="flex items-center justify-between gap-4 pb-4 text-white">
                    <div className="min-w-0">
                        <h2 className="truncate text-sm font-semibold">{file.name}</h2>
                        {typeof currentIndex === 'number' && typeof totalItems === 'number' && totalItems > 0 && (
                            <p className="text-xs text-white/60 mt-1">{currentIndex + 1} of {totalItems}</p>
                        )}
                    </div>
                    <button ref={closeButtonRef} onClick={onClose} className="p-2.5 bg-white/10 hover:bg-white/20 rounded-xl transition-colors" aria-label="Close preview">
                        <X className="w-5 h-5" />
                    </button>
                </div>
                <div className="relative flex-1 min-h-0 flex items-center justify-center">
                {onPrev && (
                    <button onClick={onPrev} className="absolute left-2 top-1/2 -translate-y-1/2 p-2 text-white bg-black/60 hover:bg-black/80 rounded-xl transition-colors z-10" title="Previous (ArrowLeft / J)" aria-label="Previous file">
                        <ChevronLeft className="w-6 h-6" />
                    </button>
                )}
                {onNext && (
                    <button onClick={onNext} className="absolute right-2 top-1/2 -translate-y-1/2 p-2 text-white bg-black/60 hover:bg-black/80 rounded-xl transition-colors z-10" title="Next (ArrowRight / L)" aria-label="Next file">
                        <ChevronRight className="w-6 h-6" />
                    </button>
                )}
                {loading && (
                    <div className="flex flex-col items-center gap-4 text-white">
                        <div className="w-10 h-10 border-4 border-stash-primary border-t-transparent rounded-full animate-spin"></div>
                        <p>Loading preview...</p>
                        <p className="text-xs text-white/50">Downloading from Telegram...</p>
                    </div>
                )}

                {error && (
                    <div className="text-red-400 bg-white/10 p-4 rounded-lg border border-red-500/20">
                        <p className="font-bold">Preview Error</p>
                        <p className="text-sm">{error}</p>
                    </div>
                )}

                {!loading && !error && src && (
                    <div className="flex flex-col items-center">
                        {isImageFile(file.name) ? (
                            <img
                                src={src}
                                className="max-w-full max-h-[calc(100vh-9rem)] object-contain rounded-lg shadow-2xl bg-black"
                                alt={`Preview of ${file.name}`}
                                onError={() => {
                                    const key = getPreviewCacheKey(file.id, activeFolderId);
                                    forgetPreview(key);
                                    setError('Failed to render image preview');
                                }}
                            />
                        ) : (
                            <div className="bg-stash-surface p-8 rounded-xl text-center border border-stash-border shadow-2xl">
                                <File className="w-16 h-16 text-stash-primary mx-auto mb-4" />
                                <h3 className="text-xl text-stash-text font-medium mb-2">{file.name}</h3>
                                <p className="text-stash-subtext mb-6">Preview not supported in app.</p>
                                <p className="text-xs text-stash-subtext">File type: {file.name.split('.').pop()}</p>
                            </div>
                        )}
                    </div>
                )}

                </div>
            </div>
        </div>
    );
}
