import { Play, Clock, Trash2, History, Sparkles } from 'lucide-react';
import { WatchHistoryEntry, removeWatchEntry, clearWatchHistory } from '../../../utils/watchHistory';
import { formatBytes } from '../../../utils';
import { TelegramFile } from '../../../types';
import { getNextEpisode } from '../../../utils/seriesParser';
import { MediaBadgesList } from '../../shared/MediaBadgesList';

interface RecentWatchBarProps {
    entries: WatchHistoryEntry[];
    currentFiles?: TelegramFile[];
    onPlay: (entry: WatchHistoryEntry) => void;
    onPlayFile?: (file: TelegramFile) => void;
    onRefresh: () => void;
}

export function RecentWatchBar({ entries, currentFiles, onPlay, onPlayFile, onRefresh }: RecentWatchBarProps) {
    if (!entries || entries.length === 0) return null;

    const handleRemove = (e: React.MouseEvent, fileId: number) => {
        e.stopPropagation();
        removeWatchEntry(fileId);
        onRefresh();
    };

    const handleClearAll = () => {
        clearWatchHistory();
        onRefresh();
    };

    // Calculate "Next Up" episode based on the latest watched entry
    const latestWatched = entries[0];
    const latestFileObj: TelegramFile | null = latestWatched ? {
        id: latestWatched.file_id,
        name: latestWatched.file_name,
        size: latestWatched.file_size,
        sizeStr: formatBytes(latestWatched.file_size),
        folder_id: latestWatched.folder_id ?? undefined,
        type: 'file'
    } : null;

    const nextEpisodeFile = (latestFileObj && currentFiles && currentFiles.length > 0)
        ? getNextEpisode(latestFileObj, currentFiles)
        : null;

    return (
        <div className="mb-6 px-1 animate-in fade-in slide-in-from-top-2">
            <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                    <History className="w-4 h-4 text-cyan-400" />
                    <span className="text-xs font-mono font-bold text-telegram-text uppercase tracking-wider">
                        Continue Watching / Recent Watch
                    </span>
                    <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-slate-900 border border-gray-800 text-cyan-400">
                        {entries.length}
                    </span>
                </div>
                <button
                    onClick={handleClearAll}
                    className="text-[11px] font-mono text-gray-500 hover:text-red-400 transition-colors flex items-center gap-1"
                    title="Clear Recent Watch History"
                >
                    <Trash2 className="w-3 h-3" />
                    <span>Clear</span>
                </button>
            </div>

            <div className="flex items-center gap-3 overflow-x-auto pb-2 scrollbar-thin scrollbar-thumb-gray-800">
                {/* Highlighted "Next Up" Episode Card */}
                {nextEpisodeFile && (
                    <div
                        key={`next-up-${nextEpisodeFile.id}`}
                        onClick={() => {
                            if (onPlayFile) {
                                onPlayFile(nextEpisodeFile);
                            } else {
                                onPlay({
                                    id: `next-${nextEpisodeFile.id}`,
                                    file_id: nextEpisodeFile.id,
                                    file_name: nextEpisodeFile.name,
                                    file_size: nextEpisodeFile.size,
                                    folder_id: nextEpisodeFile.folder_id ?? null,
                                    timestamp: new Date().toISOString(),
                                    status: 'started'
                                });
                            }
                        }}
                        role="button"
                        tabIndex={0}
                        aria-label={`Play Next: ${nextEpisodeFile.name}`}
                        onKeyDown={(event) => {
                            if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) return;
                            event.preventDefault();
                            event.currentTarget.click();
                        }}
                        className="group relative flex-shrink-0 w-64 p-3 rounded-lg bg-gradient-to-br from-cyan-950/80 via-slate-900/95 to-slate-900/95 hover:from-cyan-900/90 hover:via-slate-800/95 hover:to-slate-800/95 border-2 border-cyan-500/60 hover:border-cyan-400 cursor-pointer transition-all duration-200 select-none shadow-[0_0_15px_rgba(6,182,212,0.15)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-400"
                    >
                        <div className="flex items-center justify-between gap-2 mb-2">
                            <span className="flex items-center gap-1 text-[10px] font-mono font-bold px-2 py-0.5 rounded bg-cyan-500 text-slate-950 uppercase tracking-wide">
                                <Sparkles className="w-2.5 h-2.5 fill-current" />
                                NEXT UP
                            </span>
                            <span className="text-[10px] font-mono text-cyan-400/80">
                                {formatBytes(nextEpisodeFile.size)}
                            </span>
                        </div>

                        <div className="text-xs font-semibold text-white truncate group-hover:text-cyan-300 transition-colors mb-1.5" title={nextEpisodeFile.name}>
                            {nextEpisodeFile.name}
                        </div>

                        <div className="flex items-center justify-between pt-2 border-t border-cyan-500/20">
                            <MediaBadgesList filename={nextEpisodeFile.name} maxBadges={2} />
                            <span className="inline-flex items-center gap-1 text-[11px] font-mono font-bold text-cyan-300 group-hover:translate-x-0.5 transition-transform">
                                <Play className="w-3 h-3 fill-cyan-400" />
                                Play Next
                            </span>
                        </div>
                    </div>
                )}

                {/* Standard Watch History Cards */}
                {entries.slice(0, 10).map((entry) => {
                    const formattedDate = new Date(entry.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
                    return (
                        <div
                            key={entry.id}
                            onClick={() => onPlay(entry)}
                            role="button"
                            tabIndex={0}
                            aria-label={`Resume ${entry.file_name}`}
                            onKeyDown={(event) => {
                                if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) return;
                                event.preventDefault();
                                event.currentTarget.click();
                            }}
                            className="group relative flex-shrink-0 w-60 p-3 rounded-lg bg-slate-900/90 hover:bg-slate-800/90 border border-gray-800/80 hover:border-cyan-500/50 cursor-pointer transition-all duration-200 select-none shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-400"
                        >
                            <div className="flex items-start justify-between gap-2 mb-2">
                                <div className="flex-1 min-w-0">
                                    <div className="text-xs font-medium text-telegram-text truncate group-hover:text-cyan-400 transition-colors" title={entry.file_name}>
                                        {entry.file_name}
                                    </div>
                                    <div className="flex items-center gap-2 mt-1 text-[10px] font-mono text-gray-400">
                                        <span>{formatBytes(entry.file_size)}</span>
                                        <span>·</span>
                                        <span className="flex items-center gap-0.5">
                                            <Clock className="w-2.5 h-2.5 text-cyan-400" />
                                            {formattedDate}
                                        </span>
                                    </div>
                                </div>

                                <button
                                    onClick={(e) => handleRemove(e, entry.file_id)}
                                    className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-gray-800 text-gray-500 hover:text-red-400 transition-all"
                                    title="Remove from history"
                                    aria-label={`Remove ${entry.file_name} from history`}
                                >
                                    <Trash2 className="w-3 h-3" />
                                </button>
                            </div>

                            <div className="flex items-center justify-between pt-2 border-t border-gray-800/50">
                                {entry.quality_tag ? (
                                    <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded bg-cyan-950/80 border border-cyan-500/30 text-cyan-400 uppercase">
                                        {entry.quality_tag}
                                    </span>
                                ) : (
                                    <span className="text-[9px] font-mono text-emerald-400 uppercase">
                                        [0-DISK STREAM]
                                    </span>
                                )}

                                <span className="inline-flex items-center gap-1 text-[10px] font-mono font-bold text-cyan-400 group-hover:translate-x-0.5 transition-transform">
                                    <Play className="w-3 h-3 fill-cyan-400" />
                                    Resume
                                </span>
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
