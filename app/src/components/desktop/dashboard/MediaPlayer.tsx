import { useEffect, useRef, useState } from 'react';
import { X, ChevronLeft, ChevronRight } from '../../shared/icons.tsx';
import { invoke } from '@tauri-apps/api/core';
import { TelegramFile } from '../../../types';
import { isVideoFile, isAudioFile } from '../../../utils';
import { toast } from 'sonner';
import { recordWatchEvent } from '../../../utils/watchHistory';
import { humanizeError } from '../../../utils/errorHumanizer';
import i18n from '../../../i18n';
import { useModalDialog } from '../../../hooks/useModalDialog';

interface StreamInfo {
    token: string;
    base_url: string;
}

interface MediaPlayerProps {
    file: TelegramFile;
    onClose: () => void;
    onNext?: () => void;
    onPrev?: () => void;
    currentIndex?: number;
    totalItems?: number;
    activeFolderId: number | null;
    playlistFiles?: TelegramFile[];
}

export function MediaPlayer({ file, onClose, onNext, onPrev, currentIndex, totalItems, activeFolderId: _activeFolderId, playlistFiles }: MediaPlayerProps) {
    const [streamInfo, setStreamInfo] = useState<StreamInfo | null>(null);
    const [hasOpenedInMpv, setHasOpenedInMpv] = useState(false);
    const [mpvError, setMpvError] = useState<string | null>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const dialogRef = useModalDialog(true, onClose, closeButtonRef);

    // Get stream info on mount
    useEffect(() => {
        invoke<StreamInfo>('cmd_get_stream_info').then(setStreamInfo).catch(() => {});
    }, []);

    // Derive folderIdParam from the file itself, not the sidebar's activeFolderId.
    // This ensures the stream URL is correct even when resuming from history while
    // a different folder is active (fixes wrong-title / wrong-stream bug).
    const fileFolderParam = file.folder_id != null ? file.folder_id.toString() : 'home';
    const streamUrl = streamInfo
        ? `${streamInfo.base_url}/stream/${fileFolderParam}/${file.id}/${encodeURIComponent(file.name)}?token=${streamInfo.token}`
        : null;

    const isVideo = isVideoFile(file.name);
    const isAudio = isAudioFile(file.name);
    const isMedia = isVideo || isAudio;

    // Reset MPV state when file changes (playlist navigation)
    useEffect(() => {
        setHasOpenedInMpv(false);
        setMpvError(null);
    }, [file.id, file.folder_id]);

    // Naturally sort playlist files by filename (ascending: Ep 01 -> Ep 02 -> Ep 10)
    const sortedPlaylistFiles = playlistFiles && playlistFiles.length > 0
        ? [...playlistFiles].sort((a, b) =>
            a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' })
          )
        : [];

    const playlistItems = streamInfo && sortedPlaylistFiles.length > 0
        ? sortedPlaylistFiles.map(f => ({
            url: `${streamInfo.base_url}/stream/${f.folder_id != null ? f.folder_id.toString() : 'home'}/${f.id}/${encodeURIComponent(f.name)}?token=${streamInfo.token}`,
            message_id: f.id,
            folder_id: f.folder_id ?? undefined,
            title: f.name
        }))
        : undefined;

    const startIndex = sortedPlaylistFiles.length > 0
        ? sortedPlaylistFiles.findIndex(f => f.id === file.id)
        : undefined;

    // Automatically trigger MPV launch when streamUrl is ready
    useEffect(() => {
        if (isMedia && streamUrl && !hasOpenedInMpv && !mpvError) {
            invoke('cmd_play_in_mpv', {
                url: streamUrl,
                messageId: file.id,
                folderId: file.folder_id,
                title: file.name,
                playlist: playlistItems,
                startIndex: startIndex !== undefined && startIndex >= 0 ? startIndex : undefined
            })
                .then(() => {
                    setHasOpenedInMpv(true);
                    recordWatchEvent(file, 'started');
                })
                .catch((err) => {
                    console.error('Failed to play in MPV:', err);
                    const errMsg = err?.toString() || 'Failed to open MPV';
                    setMpvError(errMsg);
                    toast.error(`${humanizeError(err, i18n.t('errors.action_play'))} (${errMsg})`);
                });
        }
    }, [isMedia, streamUrl, hasOpenedInMpv, mpvError, file.id, file.folder_id, file.name, file, sortedPlaylistFiles, streamInfo, fileFolderParam]);

    // Handle keyboard shortcuts (Left/Right arrow keys for navigation, Esc for close)
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
            } else if (e.key === 'ArrowLeft' || key === 'j') {
                e.preventDefault();
                onPrev?.();
            }
        };

        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [onClose, onNext, onPrev]);

    return (
        <div className="fixed inset-0 z-[200] bg-black/90 flex items-center justify-center p-4 backdrop-blur-md" onClick={onClose}>
            <div ref={dialogRef} tabIndex={-1} className="relative w-full max-w-lg text-center p-8 bg-stash-surface border border-stash-border rounded-2xl shadow-2xl flex flex-col items-center gap-6" onClick={e => e.stopPropagation()} role="dialog" aria-modal="true" aria-label={`Player: ${file.name}`}>
                {/* Close Button */}
                <div className="absolute top-4 right-4">
                    <button
                        ref={closeButtonRef}
                        onClick={onClose}
                        className="w-10 h-10 flex items-center justify-center text-stash-subtext hover:text-stash-text bg-stash-hover rounded-xl transition-colors"
                        title="Close (Esc)"
                        aria-label="Close player dialog"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>

                {/* Playlist Navigation Buttons */}
                {onPrev && (
                    <button
                        onClick={onPrev}
                        className="absolute left-4 top-1/2 -translate-y-1/2 p-2 text-stash-subtext hover:text-stash-text bg-stash-hover rounded-xl transition-colors z-10"
                        title="Previous (ArrowLeft / J)"
                        aria-label="Previous file"
                    >
                        <ChevronLeft className="w-6 h-6" />
                    </button>
                )}

                {onNext && (
                    <button
                        onClick={onNext}
                        className="absolute right-4 top-1/2 -translate-y-1/2 p-2 text-stash-subtext hover:text-stash-text bg-stash-hover rounded-xl transition-colors z-10"
                        title="Next (ArrowRight / L)"
                        aria-label="Next file"
                    >
                        <ChevronRight className="w-6 h-6" />
                    </button>
                )}

                {/* Media Icon & Status */}
                {!streamUrl ? (
                    <div className="flex flex-col items-center gap-4 py-6">
                        <div className="w-10 h-10 border-4 border-stash-primary border-t-transparent rounded-full animate-spin"></div>
                        <p className="text-stash-subtext">Connecting stream...</p>
                    </div>
                ) : mpvError ? (
                    <div className="flex flex-col items-center gap-4 py-4 text-center">
                        <div className="w-16 h-16 rounded-full bg-red-500/10 flex items-center justify-center text-red-500 border border-red-500/20">
                            <X className="w-8 h-8" />
                        </div>
                        <h3 className="text-lg font-semibold text-stash-text">Failed to Play Media</h3>
                        <p className="text-sm text-stash-subtext max-w-sm">{mpvError}</p>
                    </div>
                ) : (
                    <>
                        <div className="w-20 h-20 rounded-2xl bg-stash-primary/10 flex items-center justify-center text-stash-primary border border-stash-primary/25">
                            {isVideo ? (
                                <svg xmlns="http://www.w3.org/2000/svg" className="w-10 h-10 fill-current" viewBox="0 0 24 24"><polygon points="6 3 20 12 6 21 6 3"/></svg>
                            ) : (
                                <svg xmlns="http://www.w3.org/2000/svg" className="w-10 h-10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" /></svg>
                            )}
                        </div>

                        <div>
                            <h3 className="text-xl font-semibold text-stash-text mb-2 line-clamp-2 px-6">{file.name}</h3>
                            <p className="text-sm text-stash-subtext" role="status">
                                {hasOpenedInMpv ? 'Opened in player' : 'Opening player...'}
                            </p>
                        </div>

                        <div className="flex flex-col gap-2 w-full mt-2">
                            <button
                                onClick={() => {
                                    if (streamUrl) {
                                        invoke('cmd_play_in_mpv', {
                                            url: streamUrl,
                                            messageId: file.id,
                                            folderId: file.folder_id,
                                            title: file.name,
                                            playlist: playlistItems,
                                            startIndex: startIndex !== undefined && startIndex >= 0 ? startIndex : undefined
                                        }).catch(err => {
                                            toast.error(humanizeError(err, i18n.t('errors.action_play')));
                                        });
                                    }
                                }}
                                className="auth-primary w-full py-3 rounded-xl transition-colors text-sm font-semibold"
                            >
                                Reopen in MPV
                            </button>
                            <button
                                onClick={onClose}
                                className="w-full py-3 bg-stash-hover hover:bg-stash-border text-stash-text border border-stash-border rounded-xl transition-colors text-sm font-semibold"
                            >
                                Close
                            </button>
                        </div>
                    </>
                )}

                {/* Playlist Counter */}
                {typeof currentIndex === 'number' && typeof totalItems === 'number' && totalItems > 0 && (
                    <div className="text-[11px] text-stash-subtext select-none">
                        File {currentIndex + 1} of {totalItems}
                    </div>
                )}
            </div>
        </div>
    );
}
