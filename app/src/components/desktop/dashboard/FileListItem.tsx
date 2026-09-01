import { useState } from 'react';
import { Folder, MoreVertical } from '../../shared/icons.tsx';
import { TelegramFile } from '../../../types';
import { createDragGhost } from '../../../utils';
import { FileTypeIcon } from '../../shared/FileTypeIcon';
import { useVideoMetadata } from '../../../hooks/useVideoMetadata';
import { useVideoSubtitles } from '../../../hooks/useVideoSubtitles';
import { VideoMetaBadge } from '../../shared/VideoMetaBadge';
import { MediaBadgesList } from '../../shared/MediaBadgesList';


interface FileListItemProps {
    file: TelegramFile;
    selectedIds: number[];
    onFileClick: (e: React.MouseEvent, id: number) => void;
    handleContextMenu: (e: React.MouseEvent, file: TelegramFile) => void;
    onDragStart?: (fileIds: number[]) => void;
    onDragEnd?: () => void;
    onDrop?: (e: React.DragEvent, folderId: number) => void;
}

export function FileListItem({
    file, selectedIds, onFileClick, handleContextMenu,
    onDragStart, onDragEnd, onDrop
}: FileListItemProps) {
    const [isDragOver, setIsDragOver] = useState(false);
    const isFolder = file.type === 'folder';

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

    return (
        <div
            onClick={(e) => onFileClick(e, file.id)}
            role="button"
            tabIndex={0}
            aria-label={`${isFolder ? 'Open folder' : 'Select file'} ${file.name}`}
            onKeyDown={(event) => {
                if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) return;
                event.preventDefault();
                event.currentTarget.click();
            }}
            onContextMenu={(e) => handleContextMenu(e, file)}
            draggable
            onDragStart={(e) => {
                const idsToDrag = selectedIds.includes(file.id) ? selectedIds : [file.id];
                if (onDragStart) onDragStart(idsToDrag);
                e.dataTransfer.setData("application/x-telegram-file-ids", JSON.stringify(idsToDrag));
                e.dataTransfer.effectAllowed = 'move';
                const dragCount = idsToDrag.length;
                const ghost = createDragGhost(file.name, isFolder, dragCount);
                e.dataTransfer.setDragImage(ghost, 0, 0);
                requestAnimationFrame(() => ghost.remove());
            }}
            onDragEnd={() => {
                if (onDragEnd) onDragEnd();
            }}
            onDragOver={(e) => {
                // Internal drags only (TeleStash file IDs); OS file drops are ignored.
                const isInternal = e.dataTransfer.types.includes("application/x-telegram-file-id") ||
                    e.dataTransfer.types.includes("application/x-telegram-file-ids");
                if (isFolder && isInternal) {
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
                const isInternal = e.dataTransfer.types.includes("application/x-telegram-file-id") ||
                    e.dataTransfer.types.includes("application/x-telegram-file-ids");
                if (isFolder && isInternal && onDrop) {
                    e.preventDefault();
                    e.stopPropagation();
                    setIsDragOver(false);
                    onDrop(e, file.id);
                }
            }}
            className={`vault-file-row group ${selectedIds.includes(file.id) ? 'is-selected' : ''} ${isDragOver ? 'is-drag-over' : ''}`}
        >
            <div className="flex justify-center">
                {isFolder ? <Folder className="w-5 h-5 text-stash-primary" /> : <FileTypeIcon filename={file.name} className="w-5 h-5" />}
            </div>
            <div className="min-w-0 flex items-center gap-2 text-sm text-stash-text font-medium flex-wrap">
                <span className="truncate">{file.name}</span>
                <MediaBadgesList filename={file.name} maxBadges={3} />
                <VideoMetaBadge metadata={videoMeta} isLoading={videoMetaLoading} filename={file.name} />
                {subtitles && subtitles.length > 0 && (
                    <span className="inline-flex items-center text-[9px] font-mono font-bold tracking-tight px-1.5 py-0.5 rounded border bg-indigo-950/80 text-indigo-400 border-indigo-500/30" title={subtitles.map(s => `${s.label || s.language} (${s.format})`).join(', ')}>
                        SUB: {Array.from(new Set(subtitles.map(s => s.language.toUpperCase()))).join(', ')}
                    </span>
                )}
            </div>
            <div className="hidden sm:block text-right text-xs text-stash-subtext truncate">{file.sizeStr}</div>
            <div className="hidden sm:block text-right text-xs text-stash-subtext font-mono opacity-50 truncate">{file.created_at || '-'}</div>

            {/* 3-dot Menu Button — in grid flow, not absolutely positioned */}
            <div className="flex justify-end">
                <button
                    onClick={(e) => {
                        e.stopPropagation();
                        handleContextMenu(e, file);
                    }}
                    className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 p-1 bg-stash-surface hover:bg-stash-hover border border-stash-border shadow-md rounded text-stash-subtext hover:text-stash-text transition-all"
                    aria-label="File actions"
                >
                    <MoreVertical className="w-4 h-4" />
                </button>
            </div>
        </div>
    );
}
