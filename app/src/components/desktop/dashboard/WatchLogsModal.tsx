import { useState, useEffect, useRef } from 'react';
import { X, Search, Trash2, Download, Film } from '../../shared/icons.tsx';
import { WatchLogEvent, getWatchLogs, clearWatchLogs, exportWatchLogsText } from '../../../utils/watchHistory';
import { toast } from 'sonner';
import { useModalDialog } from '../../../hooks/useModalDialog';

interface WatchLogsModalProps {
    onClose: () => void;
}

export function WatchLogsModal({ onClose }: WatchLogsModalProps) {
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const dialogRef = useModalDialog(true, onClose, closeButtonRef);
    const [logs, setLogs] = useState<WatchLogEvent[]>([]);
    const [searchTerm, setSearchTerm] = useState('');
    const [filterType, setFilterType] = useState<string>('ALL');

    const reloadLogs = () => {
        setLogs(getWatchLogs());
    };

    useEffect(() => {
        reloadLogs();
    }, []);

    const handleClear = () => {
        if (window.confirm('Are you sure you want to clear all Watch History Logs?')) {
            clearWatchLogs();
            reloadLogs();
            toast.success('Watch History Logs cleared successfully');
        }
    };

    const handleExport = () => {
        const text = exportWatchLogsText();
        const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `telestash_watch_logs_${new Date().toISOString().slice(0, 10)}.log`;
        a.click();
        URL.revokeObjectURL(url);
        toast.success('Watch activity log downloaded successfully');
    };

    const filteredLogs = logs.filter(log => {
        const matchesSearch = log.file_name.toLowerCase().includes(searchTerm.toLowerCase()) ||
            log.details.toLowerCase().includes(searchTerm.toLowerCase());
        const matchesFilter = filterType === 'ALL' || log.event_type === filterType;
        return matchesSearch && matchesFilter;
    });

    const getEventBadgeClass = (type: WatchLogEvent['event_type']) => {
        switch (type) {
            case 'PLAY_START':
                return 'bg-emerald-950/80 text-emerald-400 border-emerald-500/30';
            case 'COMPLETED':
                return 'bg-cyan-950/80 text-cyan-400 border-cyan-500/30';
            case 'SUBTITLE_GEN':
                return 'bg-purple-950/80 text-purple-400 border-purple-500/30';
            case 'ERROR':
                return 'bg-red-950/80 text-red-400 border-red-500/30';
            default:
                return 'bg-gray-900 text-stash-text border-stash-border';
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm animate-in fade-in">
            <div ref={dialogRef} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby="watch-logs-title" className="w-full max-w-4xl h-[650px] max-h-[90vh] bg-stash-surface border border-stash-border rounded-2xl shadow-2xl flex flex-col overflow-hidden">
                {/* Header */}
                <div className="flex items-center justify-between px-5 py-4 border-b border-stash-border bg-stash-hover">
                    <div className="flex items-center gap-3">
                        <div className="p-2 rounded-lg bg-stash-primary/10 border border-stash-primary/30 text-stash-primary">
                            <Film className="w-5 h-5" />
                        </div>
                        <div>
                            <h2 id="watch-logs-title" className="text-base font-semibold text-stash-text flex items-center gap-2">
                                Watch History Logs
                                <span className="text-[10px] px-2 py-0.5 rounded bg-stash-primary/10 border border-stash-primary/30 text-stash-primary">
                                    {filteredLogs.length} Events
                                </span>
                            </h2>
                            <p className="text-xs text-stash-subtext font-mono">
                                Cinema Media Playback & AI Subtitle Activity Log
                            </p>
                        </div>
                    </div>
                    <button
                        ref={closeButtonRef}
                        onClick={onClose}
                        className="p-1.5 text-stash-subtext hover:text-stash-text rounded-lg hover:bg-stash-hover transition-colors"
                        aria-label="Close watch history logs"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>

                {/* Toolbar */}
                <div className="flex items-center justify-between gap-3 px-5 py-3 border-b border-stash-border bg-stash-hover">
                    <div className="flex-1 relative">
                        <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-stash-subtext" />
                        <input
                            type="text"
                            placeholder="Search movie title or event..."
                            value={searchTerm}
                            onChange={(e) => setSearchTerm(e.target.value)}
                            className="w-full bg-stash-bg border border-stash-border rounded-lg pl-9 pr-3 py-1.5 text-xs text-stash-text font-mono placeholder:text-stash-subtext focus:outline-none focus:border-stash-primary/50"
                        />
                    </div>

                    <div className="flex items-center gap-2">
                        <select
                            value={filterType}
                            onChange={(e) => setFilterType(e.target.value)}
                            className="bg-stash-bg border border-stash-border rounded-lg px-3 py-1.5 text-xs font-mono text-stash-text focus:outline-none focus:border-stash-primary/50"
                        >
                            <option value="ALL">All Events</option>
                            <option value="PLAY_START">PLAY_START</option>
                            <option value="COMPLETED">COMPLETED</option>
                            <option value="SUBTITLE_GEN">SUBTITLE_GEN</option>
                            <option value="ERROR">ERROR</option>
                        </select>

                        <button
                            onClick={handleExport}
                            className="px-3 py-1.5 bg-stash-bg hover:bg-stash-hover border border-stash-border text-xs font-mono text-stash-text rounded-lg transition-colors flex items-center gap-1.5"
                            title="Export Watch Logs"
                        >
                            <Download className="w-3.5 h-3.5 text-cyan-400" />
                            <span>Export</span>
                        </button>

                        <button
                            onClick={handleClear}
                            className="px-3 py-1.5 bg-red-950/40 hover:bg-red-900/60 border border-red-900/50 text-xs font-mono text-red-400 rounded-lg transition-colors flex items-center gap-1.5"
                            title="Clear Watch Logs"
                        >
                            <Trash2 className="w-3.5 h-3.5" />
                            <span>Clear</span>
                        </button>
                    </div>
                </div>

                {/* Log List */}
                <div className="flex-1 overflow-y-auto p-4 space-y-2 font-mono text-xs scrollbar-thin scrollbar-thumb-gray-800">
                    {filteredLogs.length === 0 ? (
                        <div className="flex flex-col items-center justify-center h-full text-stash-subtext">
                            <Film className="w-10 h-10 mb-2 stroke-[1.5] opacity-40" />
                            <p className="text-xs">No video playback activity history found</p>
                        </div>
                    ) : (
                        filteredLogs.map((log) => {
                            const formattedTime = new Date(log.timestamp).toLocaleString([], {
                                year: 'numeric',
                                month: '2-digit',
                                day: '2-digit',
                                hour: '2-digit',
                                minute: '2-digit',
                                second: '2-digit'
                            });
                            return (
                                <div
                                    key={log.id}
                                    className="p-3 rounded-lg bg-stash-hover border border-stash-border hover:border-stash-primary/40 transition-colors flex items-start gap-3"
                                >
                                    <span className="text-[10px] text-stash-subtext whitespace-nowrap mt-0.5">
                                        [{formattedTime}]
                                    </span>
                                    <span className={`text-[10px] font-bold px-2 py-0.5 rounded border uppercase whitespace-nowrap ${getEventBadgeClass(log.event_type)}`}>
                                        {log.event_type}
                                    </span>
                                    <div className="flex-1 min-w-0">
                                        <div className="text-stash-text font-semibold truncate">
                                            {log.file_name}
                                        </div>
                                        <div className="text-stash-subtext text-[11px] mt-0.5">
                                            {log.details}
                                        </div>
                                    </div>
                                </div>
                            );
                        })
                    )}
                </div>

                {/* Footer */}
                <div className="px-5 py-3 border-t border-stash-border bg-stash-hover flex items-center justify-between text-[11px] font-mono text-stash-subtext">
                    <span>TeleStash Independent Cinema Watch Log System</span>
                    <button
                        onClick={onClose}
                        className="px-4 py-1.5 bg-stash-hover hover:bg-stash-border text-stash-text rounded-lg transition-colors"
                    >
                        Close
                    </button>
                </div>
            </div>
        </div>
    );
}
