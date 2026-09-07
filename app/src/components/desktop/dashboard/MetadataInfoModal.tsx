import { X, Star, Film } from '../../shared/icons.tsx';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TelegramFile } from '../../../types';
import { FileMetadata } from '../../../hooks/useFileMetadata';
import { useTranslation } from 'react-i18next';

interface MetadataInfoModalProps {
    file: { name: string; sizeStr: string };
    metadata: FileMetadata | null;
    poster: string | null;
    onClose: () => void;
}

/**
 * Read-only info drawer for a video's TMDB metadata (Poster Wall → View Info).
 * Uses the same overlay/panel skeleton as the other dashboard modals and the
 * existing stash-* tokens only.
 */
export function MetadataInfoModal({ file, metadata, poster, onClose }: MetadataInfoModalProps) {
    const { t } = useTranslation();

    const genres: string[] = (() => {
        if (!metadata?.genres_json) return [];
        try {
            const parsed = JSON.parse(metadata.genres_json);
            return Array.isArray(parsed) ? parsed.map(String) : [];
        } catch {
            return [];
        }
    })();

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4" onClick={onClose}>
            <div
                className="bg-stash-surface border border-stash-border rounded-2xl w-full max-w-2xl overflow-hidden shadow-2xl flex flex-col max-h-[85vh]"
                role="dialog"
                aria-modal="true"
                aria-label={metadata?.title || file.name}
                onClick={(e) => e.stopPropagation()}
            >
                {/* Header */}
                <div className="flex items-center justify-between px-5 py-3.5 border-b border-stash-border">
                    <div className="flex items-center gap-2 min-w-0">
                        <Film className="w-4 h-4 text-stash-primary shrink-0" />
                        <h2 className="text-sm font-semibold text-stash-text truncate">{t('files.metadata_info_title')}</h2>
                    </div>
                    <button onClick={onClose} className="p-1 rounded hover:bg-stash-hover text-stash-subtext hover:text-stash-text transition" aria-label={t('common.close')}>
                        <X className="w-4 h-4" />
                    </button>
                </div>

                {/* Body */}
                {metadata ? (
                    <div className="flex gap-5 p-5 overflow-y-auto">
                        {/* Poster column */}
                        <div className="w-40 shrink-0">
                            {poster ? (
                                <img src={poster} alt={metadata.title} className="w-full rounded-lg border border-stash-border object-cover" style={{ aspectRatio: '2/3' }} />
                            ) : (
                                <div className="w-full rounded-lg border border-stash-border bg-stash-bg flex items-center justify-center" style={{ aspectRatio: '2/3' }}>
                                    <Film className="w-10 h-10 text-stash-subtext/40" />
                                </div>
                            )}
                        </div>

                        {/* Details column */}
                        <div className="min-w-0 flex-1 space-y-3">
                            <h3 className="text-lg font-semibold text-stash-text leading-snug">{metadata.title}</h3>
                            <div className="flex items-center gap-2.5 flex-wrap">
                                {metadata.year && (
                                    <span className="text-xs font-mono text-stash-subtext">{metadata.year}</span>
                                )}
                                {typeof metadata.rating === 'number' && metadata.rating > 0 && (
                                    <span className="inline-flex items-center gap-1 text-xs font-mono text-stash-text">
                                        <Star className="w-3.5 h-3.5 text-stash-primary" weight="fill" />
                                        {metadata.rating.toFixed(1)}
                                    </span>
                                )}
                                {metadata.original_title && metadata.original_title !== metadata.title && (
                                    <span className="text-xs text-stash-subtext truncate max-w-[16rem]" title={metadata.original_title}>
                                        {metadata.original_title}
                                    </span>
                                )}
                            </div>
                            {genres.length > 0 && (
                                <div className="flex gap-1.5 flex-wrap">
                                    {genres.map((g) => (
                                        <span key={g} className="text-[10px] px-2 py-0.5 rounded-full border border-stash-border text-stash-subtext">
                                            {g}
                                        </span>
                                    ))}
                                </div>
                            )}
                            {metadata.overview && (
                                <p className="text-sm text-stash-subtext leading-relaxed">{metadata.overview}</p>
                            )}
                            <p className="text-xs text-stash-subtext/70 truncate pt-1" title={file.name}>
                                {file.name} · {file.sizeStr}
                            </p>
                        </div>
                    </div>
                ) : (
                    <div className="p-10 text-center text-sm text-stash-subtext">
                        {t('files.metadata_info_empty')}
                    </div>
                )}
            </div>
        </div>
    );
}

/**
 * Loads the stored metadata + cached poster for one file, then renders the
 * info modal. Used from the file context menu (View Info).
 */
export function MetadataInfoLoader({ file, onClose }: { file: TelegramFile; onClose: () => void }) {
    const [metadata, setMetadata] = useState<FileMetadata | null>(null);
    const [poster, setPoster] = useState<string | null>(null);

    useEffect(() => {
        let cancelled = false;
        invoke<FileMetadata | null>('cmd_get_file_metadata', {
            messageId: file.id,
            folderId: file.folder_id ?? null,
        })
            .then((m) => { if (!cancelled) setMetadata(m ?? null); })
            .catch(() => { /* no metadata stored yet */ });
        invoke<string>('cmd_get_tmdb_poster', {
            messageId: file.id,
            folderId: file.folder_id ?? null,
        })
            .then((p) => { if (!cancelled && p) setPoster(p); })
            .catch(() => { /* no poster cached */ });
        return () => { cancelled = true; };
    }, [file.id, file.folder_id]);

    return <MetadataInfoModal file={file} metadata={metadata} poster={poster} onClose={onClose} />;
}
