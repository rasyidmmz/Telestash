import { motion } from 'framer-motion';
import { useState, useEffect } from 'react';
import { Folder, Eye, Trash2, Link, Download } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { TelegramFile } from '../../../types';
import { createDragGhost } from '../../../utils';
import { FileTypeIcon } from '../../shared/FileTypeIcon';
import { useVideoMetadata } from '../../../hooks/useVideoMetadata';
import { useVideoSubtitles } from '../../../hooks/useVideoSubtitles';
import { VideoMetaBadge } from '../../shared/VideoMetaBadge';
import { MediaBadgesList } from '../../shared/MediaBadgesList';

interface FileCardProps {
    file: TelegramFile;
    onDelete: () => void;
    onDownload: () => void;
    onPreview?: () => void;
    onShare?: () => void;
    isSelected: boolean;
    onClick?: (e: React.MouseEvent) => void;
    onContextMenu?: (e: React.MouseEvent) => void;
    onDrop?: (e: React.DragEvent, folderId: number) => void;
    onDragStart?: (fileIds: number[]) => void;
    onDragEnd?: () => void;
    activeFolderId?: number | null;
    height?: number;
    onToggleSelection?: () => void;
    selectedIds?: number[];
}

// Check if file is an image type that can have a thumbnail
function isImageFile(filename: string): boolean {
    const ext = filename.split('.').pop()?.toLowerCase() || '';
    return ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp'].includes(ext);
}


export function FileCard({ file, onDelete, onDownload, onPreview, onShare, isSelected, onClick, onContextMenu, onDrop, onDragStart, onDragEnd, activeFolderId, height, onToggleSelection, selectedIds }: FileCardProps) {
    const isFolder = file.type === 'folder';
    const [isDragOver, setIsDragOver] = useState(false);
    const [thumbnail, setThumbnail] = useState<string | null>(null);
    const [thumbnailLoading, setThumbnailLoading] = useState(false);

    // Lazy video metadata badge (.mp4 only)
    const { data: videoMeta, isLoading: videoMetaLoading } = useVideoMetadata(
        file.id,
        file.folder_id ?? null,
        file.name,
    );

    // Attached Subtitles
    const { data: subtitles } = useVideoSubtitles(
        file.id,
        file.folder_id ?? null,
        file.name,
    );

    // Lazy load thumbnail for image files
    useEffect(() => {
        if (isFolder || !isImageFile(file.name)) return;

        let cancelled = false;
        setThumbnailLoading(true);

        invoke<string>('cmd_get_thumbnail', {
            messageId: file.id,
            folderId: activeFolderId
        }).then((result) => {
            if (!cancelled && result) {
                setThumbnail(result);
            }
        }).catch(() => {
            // Silently fail - will show icon instead
        }).finally(() => {
            if (!cancelled) setThumbnailLoading(false);
        });

        return () => { cancelled = true; };
    }, [file.id, file.name, activeFolderId, isFolder]);

    return (
        <div
            className="relative rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-stash-primary"
            role="button"
            tabIndex={0}
            aria-label={`${isFolder ? 'Open folder' : 'Select file'} ${file.name}`}
            draggable={!isFolder}
            onContextMenu={onContextMenu}
            onClick={onClick}
            onKeyDown={(event) => {
                if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) return;
                event.preventDefault();
                event.currentTarget.click();
            }}
            onDragStart={!isFolder ? (e: any) => {
                const idsToDrag = selectedIds && selectedIds.includes(file.id) ? selectedIds : [file.id];
                if (onDragStart) onDragStart(idsToDrag);
                e.dataTransfer.setData("application/x-telegram-file-ids", JSON.stringify(idsToDrag));
                e.dataTransfer.effectAllowed = 'move';
                const dragCount = idsToDrag.length;
                const ghost = createDragGhost(file.name, isFolder, dragCount);
                e.dataTransfer.setDragImage(ghost, 0, 0);
                requestAnimationFrame(() => ghost.remove());
            } : undefined}
            onDragEnd={!isFolder ? () => {
                if (onDragEnd) onDragEnd();
            } : undefined}
            onDragOver={(e) => {
                if (isFolder) {
                    e.preventDefault();
                    e.stopPropagation();
                    if (!isDragOver) setIsDragOver(true);
                }
            }}
            onDragLeave={(e) => {
                if (isFolder) {
                    e.preventDefault();
                    e.stopPropagation();
                    setIsDragOver(false);
                }
            }}
            onDrop={(e) => {
                if (isFolder && onDrop) {
                    e.preventDefault();
                    e.stopPropagation();
                    setIsDragOver(false);
                    onDrop(e, file.id);
                }
            }}
        >
            <motion.div
                whileHover={{ y: -4 }}
                className={`group cursor-pointer bg-stash-surface rounded-xl overflow-hidden border hover:shadow-[0_4px_20px_rgba(0,0,0,0.2)] transition-all relative
                ${isSelected ? 'border-stash-primary bg-stash-primary/5 ring-1 ring-stash-primary' : 'border-stash-border hover:border-stash-primary/50'}
                ${isDragOver ? 'ring-2 ring-stash-primary bg-stash-primary/20 scale-105' : ''}`}
                style={height ? { height: `${height}px` } : { aspectRatio: '4/3' }}
            >
                {/* Thumbnail or Icon */}
                {thumbnail ? (
                    <div className="absolute inset-0">
                        <img
                            src={thumbnail}
                            alt={file.name}
                            className="w-full h-full object-contain"
                        />
                        {/* Gradient overlay for text readability */}
                        <div className="absolute inset-0 bg-gradient-to-t from-black/70 via-transparent to-transparent" />
                    </div>
                ) : (
                    <div className="absolute inset-0 flex items-center justify-center p-4">
                        {isFolder ? (
                            <Folder className="w-12 h-12 text-stash-primary" />
                        ) : thumbnailLoading && isImageFile(file.name) ? (
                            <div className="w-8 h-8 border-2 border-stash-primary/30 border-t-stash-primary rounded-full animate-spin" />
                        ) : (
                            <FileTypeIcon filename={file.name} size="lg" />
                        )}
                    </div>
                )}

                {/* Selection Checkmark */}
                <button
                    type="button"
                    onClick={(e) => {
                        e.stopPropagation();
                        if (onToggleSelection) onToggleSelection();
                    }}
                    className={`absolute top-2 left-2 w-5 h-5 rounded-full border flex items-center justify-center transition-all z-10 cursor-pointer ${isSelected ? 'bg-stash-primary border-stash-primary' : 'border-white/50 bg-black/30 opacity-0 group-hover:opacity-100'}`}
                    aria-label={`${isSelected ? 'Unselect' : 'Select'} ${file.name}`}
                    aria-pressed={isSelected}
                >
                    {isSelected && <div className="w-1.5 h-1.5 bg-black rounded-full" />}
                </button>

                {/* File info overlay at bottom */}
                <div className={`absolute bottom-0 left-0 right-0 p-3 ${thumbnail ? 'text-white' : 'text-stash-text'}`}>
                    <h3 className="text-sm font-medium truncate w-full" title={file.name}>{file.name}</h3>
                    <div className="flex items-center gap-1.5 mt-1 flex-wrap">
                        <p className={`text-xs ${thumbnail ? 'text-white/70' : 'text-stash-subtext'}`}>{file.sizeStr}</p>
                        <MediaBadgesList filename={file.name} maxBadges={3} />
                        <VideoMetaBadge metadata={videoMeta} isLoading={videoMetaLoading} filename={file.name} />
                        {subtitles && subtitles.length > 0 && (
                            <span className="inline-flex items-center text-[9px] font-mono font-bold tracking-tight px-1.5 py-0.5 rounded border bg-indigo-950/80 text-indigo-400 border-indigo-500/30" title={subtitles.map(s => `${s.label || s.language} (${s.format})`).join(', ')}>
                                SUB: {Array.from(new Set(subtitles.map(s => s.language.toUpperCase()))).join(', ')}
                            </span>
                        )}
                    </div>
                </div>

                {/* Quick actions on hover */}
                <div className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity flex gap-1 z-10">
                    <button onClick={(e) => { e.stopPropagation(); if (onPreview) onPreview() }} className="file-action-btn p-1 bg-black/50 rounded-full hover:bg-stash-primary hover:text-white text-white/70" title="Preview" aria-label={`Preview ${file.name}`}>
                        <Eye className="w-3 h-3" />
                    </button>
                    <button onClick={(e) => { e.stopPropagation(); onDownload() }} className="file-action-btn p-1 bg-black/50 rounded-full hover:bg-green-500 hover:text-white text-white/70" title="Download" aria-label={`Download ${file.name}`}>
                        <Download className="w-3 h-3" />
                    </button>
                    {!isFolder && onShare && (
                        <button onClick={(e) => { e.stopPropagation(); onShare() }} className="file-action-btn p-1 bg-black/50 rounded-full hover:bg-stash-primary hover:text-white text-white/70" title="Share" aria-label={`Share ${file.name}`}>
                            <Link className="w-3 h-3" />
                        </button>
                    )}
                    <button onClick={(e) => { e.stopPropagation(); onDelete() }} className="file-action-btn p-1 bg-black/50 rounded-full hover:bg-red-500 hover:text-white text-white/70" title="Delete" aria-label={`Delete ${file.name}`}>
                        <Trash2 className="w-3 h-3" />
                    </button>
                </div>
            </motion.div>
        </div>
    )
}
