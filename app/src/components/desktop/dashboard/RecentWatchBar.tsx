import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Play, Trash2, ChartPie } from '../../shared/icons';
import { clearWatchHistory, removeWatchEntry, type WatchHistoryEntry } from '../../../utils/watchHistory';
import { getNextEpisode, groupRecentWatchEntries, parseEpisodeInfo } from '../../../utils/seriesParser';
import { type FolderPresentationMap, folderPresentationKey } from '../../../folderPresentation';
import type { TelegramFile } from '../../../types';

interface RecentWatchBarProps {
    entries: WatchHistoryEntry[];
    currentFiles?: TelegramFile[];
    activeFolderId?: number | null;
    folderPresentation?: FolderPresentationMap;
    folderNames?: Record<string, string>;
    onPlay: (entry: WatchHistoryEntry) => void;
    onPlayFile?: (file: TelegramFile) => void;
    onRefresh: () => void;
    onAnalyticsClick?: () => void;
}

function positionLabel(seconds: number): string {
    const minutes = Math.floor(seconds / 60);
    return `${minutes}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
}

export function RecentWatchBar({ entries, currentFiles = [], activeFolderId = null,
    folderPresentation = {}, folderNames = {}, onPlay, onPlayFile, onRefresh, onAnalyticsClick }: RecentWatchBarProps) {
    const [resumePositions, setResumePositions] = useState<Record<string, number>>({});

    useEffect(() => {
        let active = true;
        invoke<{ folder_id: number | null; message_id: number; seconds: number }[]>('cmd_list_resume_positions')
            .then(positions => {
                if (!active) return;
                setResumePositions(Object.fromEntries(positions.map(p => [`${folderPresentationKey(p.folder_id)}:${p.message_id}`, p.seconds])));
            }).catch(() => {});
        return () => { active = false; };
    }, []);

    if (!entries.length) return null;
    const grouped = groupRecentWatchEntries(entries, folderPresentation);
    const currentKind = folderPresentation[folderPresentationKey(activeFolderId)]?.kind ?? 'general';
    const lastInFolder = entries.find(entry => (entry.folder_id ?? null) === activeFolderId);
    const lastFile = lastInFolder && currentFiles.find(file => file.id === lastInFolder.file_id
        && (file.folder_id ?? null) === (lastInFolder.folder_id ?? null));
    const nextFile = currentKind !== 'movies' && lastFile
        ? getNextEpisode(lastFile, currentFiles.filter(file => (file.folder_id ?? null) === activeFolderId))
        : null;

    return (
        <section className="watch-strip" aria-label="Continue watching">
            <div className="watch-strip-heading">
                <h2>Continue watching</h2>
                <div className="watch-strip-tools">
                    {onAnalyticsClick && <button type="button" onClick={onAnalyticsClick}><ChartPie className="w-4 h-4" /> Analytics</button>}
                    <button type="button" onClick={() => { clearWatchHistory(); onRefresh(); }}><Trash2 className="w-4 h-4" /> Clear</button>
                </div>
            </div>
            <div className="watch-strip-items">
                {nextFile && onPlayFile && (
                    <button type="button" className="watch-card watch-next" onClick={() => onPlayFile(nextFile)}>
                        <span className="watch-card-caption">Next episode</span>
                        <strong title={nextFile.name}>{nextFile.name}</strong>
                        <span><Play className="w-4 h-4" /> Play next</span>
                    </button>
                )}
                {grouped.slice(0, 10).map(entry => {
                    const key = folderPresentationKey(entry.folder_id);
                    const kind = folderPresentation[key]?.kind ?? 'general';
                    const episode = parseEpisodeInfo(entry.file_name);
                    const title = kind === 'series' ? (folderNames[key] || episode.seriesTitle || entry.file_name) : entry.file_name;
                    const seconds = Math.max(resumePositions[`${key}:${entry.file_id}`] ?? 0, entry.last_position_secs ?? 0);
                    const percent = entry.total_duration_secs && entry.total_duration_secs > 0
                        ? Math.min(100, Math.max(0, seconds / entry.total_duration_secs * 100)) : null;
                    return (
                        <div key={`${key}:${entry.file_id}`} className="watch-card">
                            <button type="button" className="watch-card-main" aria-label={`Resume ${entry.file_name}`} onClick={() => onPlay(entry)}>
                                <span className="watch-card-caption">{kind === 'series' ? 'Series' : kind === 'movies' ? 'Movie' : 'Recent'}{episode.displayBadge ? ` · ${episode.displayBadge}` : ''}</span>
                                <strong title={title}>{title}</strong>
                                {title !== entry.file_name && <span className="watch-card-file" title={entry.file_name}>{entry.file_name}</span>}
                                {seconds > 0 && <span className="watch-card-position">Resume at {positionLabel(seconds)}</span>}
                                {percent !== null && <span className="watch-card-progress" aria-label={`${Math.round(percent)}% watched`}><span style={{ width: `${percent}%` }} /></span>}
                            </button>
                            <button type="button" className="watch-card-remove" aria-label={`Remove ${entry.file_name} from history`}
                                onClick={() => { removeWatchEntry(entry.file_id, entry.folder_id); onRefresh(); }}><Trash2 className="w-4 h-4" /></button>
                        </div>
                    );
                })}
            </div>
        </section>
    );
}
