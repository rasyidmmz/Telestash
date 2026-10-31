import { useRef } from 'react';
import { Play, X, HardDrive } from '../../shared/icons';
import { FileTypeIcon } from '../../shared/FileTypeIcon';
import { useModalDialog } from '../../../hooks/useModalDialog';
import { useVideoMetadata } from '../../../hooks/useVideoMetadata';
import { isMediaFile } from '../../../utils';
import type { TelegramFile } from '../../../types';

interface MediaDetailProps {
    file: TelegramFile;
    sourceName: string;
    onClose: () => void;
    onPlay: () => void;
    onDownload: () => void;
}

export function MediaDetail({ file, sourceName, onClose, onPlay, onDownload }: MediaDetailProps) {
    const closeRef = useRef<HTMLButtonElement>(null);
    const dialogRef = useModalDialog(true, onClose, closeRef);
    const { data: metadata } = useVideoMetadata(file.id, file.folder_id ?? null, file.name);
    const duration = metadata?.duration_secs && metadata.duration_secs > 0
        ? `${Math.floor(metadata.duration_secs / 60)} min` : null;
    const resolution = metadata?.width && metadata.height ? `${metadata.width} × ${metadata.height}` : null;

    return (
        <div className="media-detail-backdrop" onClick={event => { if (event.target === event.currentTarget) onClose(); }}>
            <div className="media-detail" ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="media-detail-title" tabIndex={-1}>
                <button ref={closeRef} type="button" className="media-detail-close" aria-label="Close details" onClick={onClose}><X className="w-5 h-5" /></button>
                <div className="media-detail-art" aria-hidden="true"><FileTypeIcon filename={file.name} className="w-16 h-16" /></div>
                <div className="media-detail-content">
                    <span className="media-detail-source">{sourceName}</span>
                    <h2 id="media-detail-title">{file.name}</h2>
                    <p className="media-detail-metadata">{[file.sizeStr, duration, resolution, metadata?.video_codec].filter(Boolean).join(' · ')}</p>
                    <div className="media-detail-actions">
                        <button type="button" className="media-detail-primary" onClick={onPlay}><Play className="w-4 h-4" /> {isMediaFile(file.name) ? 'Play' : 'Open'}</button>
                        <button type="button" className="media-detail-secondary" onClick={onDownload}><HardDrive className="w-4 h-4" /> Download</button>
                    </div>
                </div>
            </div>
        </div>
    );
}
