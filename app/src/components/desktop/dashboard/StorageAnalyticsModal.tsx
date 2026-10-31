import { useMemo, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { X, HardDrive, PieChart, Film, Music, FileText, Folder, Zap, Trash2, Copy } from '../../shared/icons.tsx';
import { TelegramFile, TelegramFolder } from '../../../types';
import { formatBytes } from '../../../utils';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import { humanizeError } from '../../../utils/errorHumanizer';
import i18n from '../../../i18n';
import { useModalDialog } from '../../../hooks/useModalDialog';

interface StorageAnalyticsModalProps {
    files: TelegramFile[];
    folders: TelegramFolder[];
    onClose: () => void;
}

interface DuplicateGroup {
    size: number;
    hash: string;
    confirmed?: boolean;
    files: { message_id: number; folder_id: number | null; name: string; size: number; created_at: string; hash: string }[];
}

export function StorageAnalyticsModal({ files, folders, onClose }: StorageAnalyticsModalProps) {
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const dialogRef = useModalDialog(true, onClose, closeButtonRef);
    const queryClient = useQueryClient();
    const [dupScanning, setDupScanning] = useState(false);
    const [dupGroups, setDupGroups] = useState<DuplicateGroup[] | null>(null);
    const [dupSelected, setDupSelected] = useState<Set<string>>(new Set());

    const handleFindDuplicates = async (confirm: boolean) => {
        setDupScanning(true);
        setDupGroups(null);
        try {
            const folderIds: (number | null)[] = [null, ...folders.map((f) => f.id)];
            const groups = await invoke<DuplicateGroup[]>('cmd_find_duplicates', { folderIds, confirm });
            setDupGroups(groups);
            if (groups.length === 0) toast.success('No duplicates found.');
            else if (!confirm) toast.success(`Found ${groups.length} possible duplicate group(s) by name+size.`);
        } catch (err) {
            toast.error(humanizeError(err, i18n.t('errors.action_duplicate_scan')));
        } finally {
            setDupScanning(false);
        }
    };

    const toggleDup = (key: string) => {
        setDupSelected((prev) => {
            const next = new Set(prev);
            if (next.has(key)) next.delete(key);
            else next.add(key);
            return next;
        });
    };

    const handleDeleteSelected = async () => {
        const keys = Array.from(dupSelected);
        let deleted = 0;
        for (const key of keys) {
            const [fid, mid] = key.split(':');
            const folderId = fid === 'null' ? null : Number(fid);
            try {
                await invoke('cmd_delete_file', { messageId: Number(mid), folderId });
                deleted += 1;
            } catch (err) {
                toast.error(humanizeError(err, i18n.t('errors.action_delete')));
            }
        }
        setDupSelected(new Set());
        setDupGroups(null);
        queryClient.invalidateQueries({ queryKey: ['files'] });
        toast.success(`Deleted ${deleted} duplicate file(s).`);
    };

    const analytics = useMemo(() => {
        let videoSize = 0;
        let videoCount = 0;
        let audioSize = 0;
        let audioCount = 0;
        let subtitleSize = 0;
        let subtitleCount = 0;
        let docSize = 0;
        let docCount = 0;
        let totalSize = 0;

        files.forEach((file) => {
            const size = file.size || 0;
            totalSize += size;
            const ext = file.name.split('.').pop()?.toLowerCase() || '';

            if (['mp4', 'mkv', 'avi', 'mov', 'webm', 'flv', 'wmv'].includes(ext)) {
                videoSize += size;
                videoCount++;
            } else if (['mp3', 'flac', 'wav', 'aac', 'ogg', 'm4a'].includes(ext)) {
                audioSize += size;
                audioCount++;
            } else if (['srt', 'vtt', 'ass', 'ssa'].includes(ext)) {
                subtitleSize += size;
                subtitleCount++;
            } else {
                docSize += size;
                docCount++;
            }
        });

        const safeTotal = totalSize || 1;

        return {
            totalSize,
            totalFiles: files.length,
            totalFolders: folders.length,
            video: { size: videoSize, count: videoCount, percent: Math.round((videoSize / safeTotal) * 100) },
            audio: { size: audioSize, count: audioCount, percent: Math.round((audioSize / safeTotal) * 100) },
            subtitle: { size: subtitleSize, count: subtitleCount, percent: Math.round((subtitleSize / safeTotal) * 100) },
            doc: { size: docSize, count: docCount, percent: Math.round((docSize / safeTotal) * 100) },
        };
    }, [files, folders]);

    const handlePurgeCache = async () => {
        try {
            await invoke('cmd_clean_cache');
            toast.success('Preview cache cleared successfully!');
        } catch (err) {
            toast.error(humanizeError(err, i18n.t('errors.action_clear_cache')));
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-md p-4 animate-in fade-in duration-200">
            <div ref={dialogRef} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby="storage-analytics-title" className="bg-stash-surface border border-stash-border rounded-2xl w-full max-w-2xl overflow-hidden shadow-2xl flex flex-col max-h-[90vh]">
                
                {/* Header */}
                <div className="px-6 py-4 border-b border-stash-border flex items-center justify-between bg-stash-hover">
                    <div className="flex items-center gap-3">
                        <div className="w-9 h-9 rounded-xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
                            <PieChart className="w-5 h-5" />
                        </div>
                        <div>
                            <h2 id="storage-analytics-title" className="text-base font-semibold text-stash-text tracking-tight">Storage analytics</h2>
                            <p className="text-xs text-stash-subtext">Media and storage in your Telegram library</p>
                        </div>
                    </div>
                    <button
                        ref={closeButtonRef}
                        onClick={onClose}
                        className="p-2 text-stash-subtext hover:text-stash-text hover:bg-stash-hover rounded-xl transition-all"
                        aria-label="Close storage analytics"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>

                {/* Content */}
                <div className="p-6 space-y-6 overflow-y-auto font-sans">
                    
                    {/* Summary Cards */}
                    <div className="grid grid-cols-3 gap-3">
                        <div className="p-4 bg-stash-hover border border-stash-border rounded-xl flex items-center gap-3">
                            <div className="p-3 bg-cyan-500/10 text-cyan-400 rounded-lg border border-stash-border">
                                <HardDrive className="w-5 h-5" />
                            </div>
                            <div>
                                <div className="text-xs text-stash-subtext font-mono">TOTAL USED</div>
                                <div className="text-lg font-bold text-stash-text font-mono">{formatBytes(analytics.totalSize)}</div>
                            </div>
                        </div>

                        <div className="p-4 bg-stash-hover border border-stash-border rounded-xl flex items-center gap-3">
                            <div className="p-3 bg-emerald-500/10 text-emerald-400 rounded-lg border border-emerald-500/20">
                                <Film className="w-5 h-5" />
                            </div>
                            <div>
                                <div className="text-xs text-stash-subtext font-mono">TOTAL FILES</div>
                                <div className="text-lg font-bold text-stash-text font-mono">{analytics.totalFiles} files</div>
                            </div>
                        </div>

                        <div className="p-4 bg-stash-hover border border-stash-border rounded-xl flex items-center gap-3">
                            <div className="p-3 bg-purple-500/10 text-purple-400 rounded-lg border border-purple-500/20">
                                <Folder className="w-5 h-5" />
                            </div>
                            <div>
                                <div className="text-xs text-stash-subtext font-mono">FOLDERS</div>
                                <div className="text-lg font-bold text-stash-text font-mono">{analytics.totalFolders} folders</div>
                            </div>
                        </div>
                    </div>

                    {/* Progress Bar Distribution */}
                    <div className="space-y-2">
                        <div className="flex justify-between text-xs text-stash-subtext font-mono">
                            <span>MEDIA TYPES</span>
                            <span>{analytics.video.percent}% Video</span>
                        </div>
                        <div className="h-3 w-full bg-stash-hover rounded-full overflow-hidden flex border border-stash-border p-0.5">
                            <div style={{ width: `${analytics.video.percent}%` }} className="bg-cyan-500 h-full rounded-l-full transition-all duration-500" title="Video" />
                            <div style={{ width: `${analytics.audio.percent}%` }} className="bg-emerald-500 h-full transition-all duration-500" title="Audio" />
                            <div style={{ width: `${analytics.subtitle.percent}%` }} className="bg-amber-500 h-full transition-all duration-500" title="Subtitles" />
                            <div style={{ width: `${analytics.doc.percent}%` }} className="bg-gray-600 h-full rounded-r-full transition-all duration-500" title="Documents" />
                        </div>
                    </div>

                    {/* Detail Breakdown */}
                    <div className="grid grid-cols-2 gap-3">
                        <div className="p-3 bg-stash-hover border border-stash-border rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <Film className="w-4 h-4 text-cyan-400" />
                                <div>
                                    <div className="text-sm font-semibold text-stash-text">Film & Video</div>
                                    <div className="text-xs text-stash-subtext">{analytics.video.count} files</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-cyan-400">
                                {formatBytes(analytics.video.size)}
                            </div>
                        </div>

                        <div className="p-3 bg-stash-hover border border-stash-border rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <Music className="w-4 h-4 text-emerald-400" />
                                <div>
                                    <div className="text-sm font-semibold text-stash-text">Audio & Music</div>
                                    <div className="text-xs text-stash-subtext">{analytics.audio.count} files</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-emerald-400">
                                {formatBytes(analytics.audio.size)}
                            </div>
                        </div>

                        <div className="p-3 bg-stash-hover border border-stash-border rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <Zap className="w-4 h-4 text-amber-400" />
                                <div>
                                    <div className="text-sm font-semibold text-stash-text">Subtitle Sidecar</div>
                                    <div className="text-xs text-stash-subtext">{analytics.subtitle.count} files</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-amber-400">
                                {formatBytes(analytics.subtitle.size)}
                            </div>
                        </div>

                        <div className="p-3 bg-stash-hover border border-stash-border rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <FileText className="w-4 h-4 text-stash-subtext" />
                                <div>
                                    <div className="text-sm font-semibold text-stash-text">Documents & Other</div>
                                    <div className="text-xs text-stash-subtext">{analytics.doc.count} files</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-stash-text">
                                {formatBytes(analytics.doc.size)}
                            </div>
                        </div>
                    </div>

                </div>

                    {/* Duplicates */}
                    <div className="p-4 bg-stash-hover border border-stash-border rounded-xl">
                        <div className="flex items-center justify-between mb-3">
                            <div className="flex items-center gap-2 text-xs font-mono text-stash-text uppercase tracking-wider">
                                <Copy className="w-3.5 h-3.5 text-amber-400" />
                                DUPLICATE FILES
                            </div>
                            <div className="flex items-center gap-2">
                                <button
                                    onClick={() => handleFindDuplicates(false)}
                                    disabled={dupScanning}
                                    className="px-3 py-1.5 bg-amber-500/10 border border-amber-500/30 text-amber-400 rounded-lg hover:bg-amber-500/20 text-xs font-mono disabled:opacity-50"
                                    title="Fast: match by name+size only (no download)"
                                >
                                    {dupScanning ? 'SCANNING...' : 'QUICK SCAN'}
                                </button>
                                <button
                                    onClick={() => handleFindDuplicates(true)}
                                    disabled={dupScanning}
                                    className="px-3 py-1.5 bg-amber-500/10 border border-amber-500/30 text-amber-400 rounded-lg hover:bg-amber-500/20 text-xs font-mono disabled:opacity-50"
                                    title="Confirm with content hash of first 256 KiB"
                                >
                                    {dupScanning ? 'SCANNING...' : 'FIND DUPLICATES'}
                                </button>
                            </div>
                        </div>

                        {dupScanning && (
                            <p className="text-xs text-stash-subtext font-mono">Quick scan is instant; full scan streams file prefixes from Telegram and can take a while.</p>
                        )}

                        {dupGroups && dupGroups.length === 0 && (
                            <p className="text-xs text-stash-subtext font-mono">No duplicates found.</p>
                        )}

                        {dupGroups && dupGroups.length > 0 && (
                            <div className="space-y-3 max-h-64 overflow-y-auto">
                                {dupGroups.map((group, gi) => (
                                    <div key={gi} className="border border-stash-border rounded-lg p-3">
                                        <div className="flex items-center justify-between mb-2">
                                            <span className="text-xs font-mono text-amber-400">
                                                {group.files.length} copies
                                                {group.confirmed === false && (
                                                    <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded border border-amber-500/40 text-amber-300/90">
                                                        possible
                                                    </span>
                                                )}
                                            </span>
                                            <span className="text-xs font-mono text-stash-subtext">{formatBytes(group.size)} each</span>
                                        </div>
                                        <div className="space-y-1.5">
                                            {group.files.map((f) => {
                                                const key = `${f.folder_id ?? 'null'}:${f.message_id}`;
                                                return (
                                                    <label key={key} className="flex items-center gap-2 text-xs text-stash-text cursor-pointer group">
                                                        <input
                                                            type="checkbox"
                                                            checked={dupSelected.has(key)}
                                                            onChange={() => toggleDup(key)}
                                                            className="accent-red-500"
                                                        />
                                                        <span className="truncate flex-1" title={f.name}>{f.name}</span>
                                                        <span className="text-stash-subtext font-mono">{new Date(f.created_at).toLocaleDateString()}</span>
                                                    </label>
                                                );
                                            })}
                                        </div>
                                    </div>
                                ))}
                                {dupSelected.size > 0 && (
                                    <button
                                        onClick={handleDeleteSelected}
                                        className="w-full px-3 py-2 bg-red-500/10 border border-red-500/30 text-red-400 rounded-lg hover:bg-red-500/20 text-xs font-mono"
                                    >
                                        DELETE {dupSelected.size} SELECTED
                                    </button>
                                )}
                            </div>
                        )}
                    </div>

                {/* Footer */}
                <div className="px-6 py-4 border-t border-stash-border bg-stash-hover flex items-center justify-between">
                    <button
                        onClick={handlePurgeCache}
                        className="px-4 py-2 bg-red-500/10 border border-red-500/30 text-red-400 rounded-xl hover:bg-red-500/20 text-xs font-semibold font-mono flex items-center gap-2 transition-all"
                    >
                        <Trash2 className="w-3.5 h-3.5" />
                        PURGE LOCAL CACHE
                    </button>

                    <button
                        onClick={onClose}
                        className="px-5 py-2 auth-primary font-semibold rounded-xl text-xs transition-all font-mono"
                    >
                        CLOSE
                    </button>
                </div>

            </div>
        </div>
    );
}
