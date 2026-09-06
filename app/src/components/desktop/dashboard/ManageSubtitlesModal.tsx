import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Trash2, Loader2, X } from '../../shared/icons.tsx';
import { TelegramFile, VideoSubtitleInfo } from '../../../types';
import { getLanguageLabel } from '../../../utils/subtitleMatcher';
import { useTranslation } from 'react-i18next';

interface ManageSubtitlesModalProps {
    target: { file: TelegramFile; subtitles: VideoSubtitleInfo[] };
    onClose: () => void;
}

export const ManageSubtitlesModal: React.FC<ManageSubtitlesModalProps> = ({ target, onClose }) => {
    const queryClient = useQueryClient();
    const { t } = useTranslation();
    const [deletingId, setDeletingId] = useState<string | null>(null);
    const [confirmingId, setConfirmingId] = useState<string | null>(null);

    const handleDelete = async (subtitle: VideoSubtitleInfo) => {
        if (confirmingId !== subtitle.id) {
            setConfirmingId(subtitle.id);
            return;
        }
        setConfirmingId(null);
        setDeletingId(subtitle.id);
        try {
            await invoke('cmd_delete_video_subtitle', {
                subtitleId: subtitle.id,
                folderId: target.file.folder_id ?? null,
                videoFileName: target.file.name,
            });
            await queryClient.invalidateQueries({
                queryKey: ['video-subtitles', target.file.folder_id ?? null, target.file.id],
            });
            toast.success(t('files.subtitles_deleted', { language: getLanguageLabel(subtitle.language) }));
        } catch (err) {
            toast.error(`${t('files.subtitles_delete_failed')}: ${err}`);
        } finally {
            setDeletingId(null);
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" onClick={onClose}>
            <div
                className="bg-stash-surface border border-stash-border rounded-xl shadow-2xl w-full max-w-md p-5 animate-in fade-in zoom-in-95 duration-100"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex items-start justify-between gap-3 mb-4">
                    <div>
                        <h3 className="text-base font-semibold text-stash-text">{t('files.subtitles_manage')}</h3>
                        <p className="text-xs text-stash-subtext truncate max-w-[320px]" title={target.file.name}>
                            {target.file.name}
                        </p>
                    </div>
                    <button onClick={onClose} className="text-stash-subtext hover:text-stash-text transition-colors">
                        <X className="w-4 h-4" />
                    </button>
                </div>

                <div className="flex flex-col gap-2 max-h-72 overflow-y-auto">
                    {target.subtitles.map((subtitle) => (
                        <div
                            key={subtitle.id}
                            className="flex items-center justify-between gap-3 px-3 py-2 bg-stash-bg/60 border border-stash-border rounded-lg"
                        >
                            <div className="min-w-0">
                                <div className="text-sm text-stash-text font-medium">
                                    {getLanguageLabel(subtitle.language)}
                                    <span className="text-xs text-stash-subtext font-normal ml-2 uppercase">{subtitle.format}</span>
                                </div>
                                <div className="text-xs text-stash-subtext truncate" title={subtitle.original_filename}>
                                    {subtitle.original_filename}
                                </div>
                            </div>
                            <button
                                onClick={() => handleDelete(subtitle)}
                                disabled={deletingId !== null}
                                title={confirmingId === subtitle.id ? t('files.subtitles_delete_confirm') : t('files.subtitles_remove')}
                                className={`shrink-0 p-1.5 rounded transition-colors ${
                                    confirmingId === subtitle.id
                                        ? 'bg-red-500/20 text-red-400'
                                        : 'text-stash-subtext hover:text-red-400 hover:bg-red-500/10'
                                } disabled:opacity-50`}
                            >
                                {deletingId === subtitle.id ? (
                                    <Loader2 className="w-4 h-4 animate-spin" />
                                ) : (
                                    <Trash2 className="w-4 h-4" />
                                )}
                            </button>
                        </div>
                    ))}
                </div>
            </div>
        </div>
    );
};
