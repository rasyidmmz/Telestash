import { DownloadItem } from "../../../types";
import { Download, Check, X, AlertCircle, RotateCcw, Pause, Play } from "lucide-react";

function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

interface DownloadQueueProps {
    items: DownloadItem[];
    onClearFinished: () => void;
    onCancelAll: () => void;
    onCancelItem: (id: string) => void;
    onPauseItem?: (id: string) => void;
    onResumeItem?: (id: string) => void;
    onPauseAll?: () => void;
    onResumeAll?: () => void;
    onRetryItem: (id: string) => void;
}

export function DownloadQueue({
    items,
    onClearFinished,
    onCancelAll,
    onCancelItem,
    onPauseItem,
    onResumeItem,
    onPauseAll,
    onResumeAll,
    onRetryItem,
}: DownloadQueueProps) {
    if (items.length === 0) return null;

    const activeCount = items.filter(i => i.status === 'pending' || i.status === 'downloading').length;
    const pausedCount = items.filter(i => i.status === 'paused').length;
    const completedCount = items.filter(i => i.status === 'success').length;

    return (
        <div className="fixed bottom-4 right-4 w-80 bg-telegram-surface border border-telegram-border rounded-xl shadow-2xl overflow-hidden z-[100]">
            <div className="p-3 border-b border-telegram-border bg-telegram-hover flex justify-between items-center">
                <div className="flex items-center gap-2">
                    <Download className="w-4 h-4 text-telegram-secondary" />
                    <h4 className="text-sm font-medium text-telegram-text">Downloads</h4>
                    {activeCount > 0 && (
                        <span className="text-xs px-1.5 py-0.5 bg-telegram-secondary/20 text-telegram-secondary rounded-full">
                            {activeCount} active
                        </span>
                    )}
                </div>
                <div className="flex gap-2 items-center">
                    {activeCount > 0 && onPauseAll && (
                        <button onClick={onPauseAll} className="text-xs text-amber-400 hover:text-amber-300 transition-colors">Pause All</button>
                    )}
                    {pausedCount > 0 && onResumeAll && (
                        <button onClick={onResumeAll} className="text-xs text-green-400 hover:text-green-300 transition-colors">Resume All</button>
                    )}
                    {activeCount > 0 && (
                        <button onClick={onCancelAll} className="text-xs text-red-400 hover:text-red-300 transition-colors">Cancel All</button>
                    )}
                    {completedCount > 0 && (
                        <button onClick={onClearFinished} className="text-xs text-telegram-primary hover:text-telegram-text transition-colors">
                            Clear Finished
                        </button>
                    )}
                </div>
            </div>
            <div className="max-h-60 overflow-y-auto p-2 space-y-2">
                {items.map(item => (
                    <div key={item.id} className="flex flex-col gap-1 p-2 bg-telegram-hover rounded">
                        <div className="flex items-center gap-2 text-sm">
                            <div className="flex-shrink-0">
                                {item.status === 'pending' && <div className="w-4 h-4 rounded-full bg-yellow-500/20 flex items-center justify-center"><div className="w-2 h-2 bg-yellow-500 rounded-full" /></div>}
                                {item.status === 'paused' && <div className="w-4 h-4 rounded-full bg-amber-500/20 flex items-center justify-center"><div className="w-2 h-2 bg-amber-500 rounded-full" /></div>}
                                {item.status === 'downloading' && <div className="w-4 h-4 rounded-full border-2 border-telegram-secondary border-t-transparent animate-spin" />}
                                {item.status === 'success' && <div className="w-4 h-4 rounded-full bg-green-500/20 flex items-center justify-center"><Check className="w-3 h-3 text-green-500" /></div>}
                                {item.status === 'error' && <div className="w-4 h-4 rounded-full bg-red-500/20 flex items-center justify-center"><X className="w-3 h-3 text-red-500" /></div>}
                                {item.status === 'cancelled' && <div className="w-4 h-4 rounded-full bg-gray-500/20 flex items-center justify-center"><X className="w-3 h-3 text-gray-400" /></div>}
                            </div>
                            <div className="flex-1 truncate text-telegram-subtext text-xs" title={item.filename}>
                                {item.filename}
                            </div>

                            {/* Pause / Resume buttons per file */}
                            {item.status === 'downloading' && onPauseItem && (
                                <button
                                    onClick={() => onPauseItem(item.id)}
                                    className="p-1 text-gray-400 hover:text-amber-400 hover:bg-telegram-surface/80 rounded transition-colors flex-shrink-0"
                                    title="Pause download"
                                    aria-label="Pause download"
                                >
                                    <Pause className="w-3.5 h-3.5" />
                                </button>
                            )}
                            {item.status === 'pending' && onPauseItem && (
                                <button
                                    onClick={() => onPauseItem(item.id)}
                                    className="p-1 text-gray-400 hover:text-amber-400 hover:bg-telegram-surface/80 rounded transition-colors flex-shrink-0"
                                    title="Pause / Hold in queue"
                                    aria-label="Pause queued download"
                                >
                                    <Pause className="w-3.5 h-3.5" />
                                </button>
                            )}
                            {item.status === 'paused' && onResumeItem && (
                                <button
                                    onClick={() => onResumeItem(item.id)}
                                    className="p-1 text-amber-400 hover:text-green-400 hover:bg-telegram-surface/80 rounded transition-colors flex-shrink-0"
                                    title="Resume download"
                                    aria-label="Resume download"
                                >
                                    <Play className="w-3.5 h-3.5" />
                                </button>
                            )}

                            {/* Cancel / Remove / Retry buttons */}
                            {(item.status === 'downloading' || item.status === 'paused') && (
                                <button
                                    onClick={() => onCancelItem(item.id)}
                                    className="p-1 text-gray-400 hover:text-red-400 hover:bg-telegram-surface/80 rounded transition-colors flex-shrink-0"
                                    title="Cancel"
                                    aria-label="Cancel download"
                                >
                                    <X className="w-3.5 h-3.5" />
                                </button>
                            )}
                            {item.status === 'pending' && (
                                <button
                                    onClick={() => onCancelItem(item.id)}
                                    className="p-1 text-gray-400 hover:text-red-400 hover:bg-telegram-surface/80 rounded transition-colors flex-shrink-0"
                                    title="Remove from queue"
                                    aria-label="Remove from queue"
                                >
                                    <X className="w-3.5 h-3.5" />
                                </button>
                            )}
                            {(item.status === 'error' || item.status === 'cancelled') && (
                                <button
                                    onClick={() => onRetryItem(item.id)}
                                    className="p-1 text-gray-400 hover:text-blue-400 hover:bg-telegram-surface/80 rounded transition-colors flex-shrink-0"
                                    title="Retry"
                                    aria-label="Retry download"
                                >
                                    <RotateCcw className="w-3.5 h-3.5" />
                                </button>
                            )}
                        </div>

                        {/* Progress Bar for Active Items */}
                        {item.status === 'downloading' && (
                            <>
                                <div className="w-full bg-telegram-border h-1 mt-1 rounded-full overflow-hidden">
                                    {item.progress !== undefined ? (
                                        <div
                                            className="bg-telegram-secondary h-full rounded-full transition-all duration-300"
                                            style={{ width: `${item.progress}%` }}
                                        />
                                    ) : (
                                        <div className="bg-telegram-secondary h-full w-full animate-progress-indeterminate" />
                                    )}
                                </div>
                                <div className="flex justify-between text-[10px] text-telegram-subtext mt-0.5">
                                    <span>
                                        {item.downloadedBytes !== undefined && item.totalBytes !== undefined
                                            ? `${formatBytes(item.downloadedBytes)} / ${formatBytes(item.totalBytes)}`
                                            : item.progress !== undefined ? `${item.progress}%` : ''}
                                    </span>
                                    <span>
                                        {item.speedBytesPerSec !== undefined && item.speedBytesPerSec > 0
                                            ? `${formatBytes(item.speedBytesPerSec)}/s`
                                            : ''}
                                    </span>
                                </div>
                            </>
                        )}

                        {/* Progress Bar for Paused Items */}
                        {item.status === 'paused' && (
                            <>
                                <div className="w-full bg-telegram-border h-1 mt-1 rounded-full overflow-hidden">
                                    <div
                                        className="bg-amber-500 h-full rounded-full transition-all duration-300"
                                        style={{ width: `${item.progress || 0}%` }}
                                    />
                                </div>
                                <div className="flex justify-between text-[10px] text-amber-400/90 mt-0.5 font-medium">
                                    <span>Paused {item.progress !== undefined ? `• ${item.progress}%` : ''}</span>
                                    <span>
                                        {item.downloadedBytes !== undefined && item.totalBytes !== undefined
                                            ? `${formatBytes(item.downloadedBytes)} / ${formatBytes(item.totalBytes)}`
                                            : ''}
                                    </span>
                                </div>
                            </>
                        )}

                        {item.status === 'error' && item.error && (
                            <div className="flex items-center gap-1 text-xs text-red-400 mt-1">
                                <AlertCircle className="w-3 h-3 flex-shrink-0" />
                                <span className="truncate">{item.error}</span>
                            </div>
                        )}
                        {item.status === 'cancelled' && <div className="text-xs text-gray-400 mt-0.5">Cancelled</div>}
                    </div>
                ))}
            </div>
        </div>
    );
}
