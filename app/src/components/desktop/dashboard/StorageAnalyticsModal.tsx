import { useMemo } from 'react';
import { X, HardDrive, PieChart, Film, Music, FileText, Folder, Zap, Trash2 } from '../../shared/icons.tsx';
import { TelegramFile, TelegramFolder } from '../../../types';
import { formatBytes } from '../../../utils';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';

interface StorageAnalyticsModalProps {
    files: TelegramFile[];
    folders: TelegramFolder[];
    onClose: () => void;
}

export function StorageAnalyticsModal({ files, folders, onClose }: StorageAnalyticsModalProps) {
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
            toast.success('Preview & thumbnail cache cleared successfully!');
        } catch (err) {
            toast.error(`Failed to clear cache: ${err}`);
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-md p-4 animate-in fade-in duration-200">
            <div className="bg-[#0D131F] border border-cyan-500/20 rounded-2xl w-full max-w-2xl overflow-hidden shadow-2xl flex flex-col max-h-[90vh]">
                
                {/* Header */}
                <div className="px-6 py-4 border-b border-gray-800 flex items-center justify-between bg-black/40">
                    <div className="flex items-center gap-3">
                        <div className="w-9 h-9 rounded-xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
                            <PieChart className="w-5 h-5" />
                        </div>
                        <div>
                            <h2 className="text-base font-bold text-white tracking-wide font-mono flex items-center gap-2">
                                STORAGE ANALYTICS <span className="text-xs font-normal text-cyan-400/80 bg-cyan-950/60 px-2 py-0.5 rounded border border-cyan-800/50">[0-DISK VAULT]</span>
                            </h2>
                            <p className="text-xs text-gray-400">Analisis alokasi media dan ruang penyimpanan cloud Telegram</p>
                        </div>
                    </div>
                    <button
                        onClick={onClose}
                        className="p-2 text-gray-400 hover:text-white hover:bg-gray-800/60 rounded-xl transition-all"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>

                {/* Content */}
                <div className="p-6 space-y-6 overflow-y-auto font-sans">
                    
                    {/* Summary Cards */}
                    <div className="grid grid-cols-3 gap-3">
                        <div className="p-4 bg-gray-900/60 border border-gray-800 rounded-xl flex items-center gap-3">
                            <div className="p-3 bg-cyan-500/10 text-cyan-400 rounded-lg border border-cyan-500/20">
                                <HardDrive className="w-5 h-5" />
                            </div>
                            <div>
                                <div className="text-xs text-gray-400 font-mono">TOTAL DIGUNAKAN</div>
                                <div className="text-lg font-bold text-white font-mono">{formatBytes(analytics.totalSize)}</div>
                            </div>
                        </div>

                        <div className="p-4 bg-gray-900/60 border border-gray-800 rounded-xl flex items-center gap-3">
                            <div className="p-3 bg-emerald-500/10 text-emerald-400 rounded-lg border border-emerald-500/20">
                                <Film className="w-5 h-5" />
                            </div>
                            <div>
                                <div className="text-xs text-gray-400 font-mono">TOTAL MEDIA</div>
                                <div className="text-lg font-bold text-white font-mono">{analytics.totalFiles} Berkas</div>
                            </div>
                        </div>

                        <div className="p-4 bg-gray-900/60 border border-gray-800 rounded-xl flex items-center gap-3">
                            <div className="p-3 bg-purple-500/10 text-purple-400 rounded-lg border border-purple-500/20">
                                <Folder className="w-5 h-5" />
                            </div>
                            <div>
                                <div className="text-xs text-gray-400 font-mono">FOLDER VAULT</div>
                                <div className="text-lg font-bold text-white font-mono">{analytics.totalFolders} Folder</div>
                            </div>
                        </div>
                    </div>

                    {/* Progress Bar Distribution */}
                    <div className="space-y-2">
                        <div className="flex justify-between text-xs text-gray-400 font-mono">
                            <span>DISTRIBUSI JENIS MEDIA</span>
                            <span>{analytics.video.percent}% Video</span>
                        </div>
                        <div className="h-3 w-full bg-gray-900 rounded-full overflow-hidden flex border border-gray-800 p-0.5">
                            <div style={{ width: `${analytics.video.percent}%` }} className="bg-cyan-500 h-full rounded-l-full transition-all duration-500" title="Video" />
                            <div style={{ width: `${analytics.audio.percent}%` }} className="bg-emerald-500 h-full transition-all duration-500" title="Audio" />
                            <div style={{ width: `${analytics.subtitle.percent}%` }} className="bg-amber-500 h-full transition-all duration-500" title="Subtitles" />
                            <div style={{ width: `${analytics.doc.percent}%` }} className="bg-gray-600 h-full rounded-r-full transition-all duration-500" title="Dokumen" />
                        </div>
                    </div>

                    {/* Detail Breakdown */}
                    <div className="grid grid-cols-2 gap-3">
                        <div className="p-3 bg-gray-900/40 border border-gray-800/80 rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <Film className="w-4 h-4 text-cyan-400" />
                                <div>
                                    <div className="text-sm font-semibold text-white">Film & Video</div>
                                    <div className="text-xs text-gray-400">{analytics.video.count} Berkas</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-cyan-400">
                                {formatBytes(analytics.video.size)}
                            </div>
                        </div>

                        <div className="p-3 bg-gray-900/40 border border-gray-800/80 rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <Music className="w-4 h-4 text-emerald-400" />
                                <div>
                                    <div className="text-sm font-semibold text-white">Audio & Musik</div>
                                    <div className="text-xs text-gray-400">{analytics.audio.count} Berkas</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-emerald-400">
                                {formatBytes(analytics.audio.size)}
                            </div>
                        </div>

                        <div className="p-3 bg-gray-900/40 border border-gray-800/80 rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <Zap className="w-4 h-4 text-amber-400" />
                                <div>
                                    <div className="text-sm font-semibold text-white">Whisper AI Subtitles</div>
                                    <div className="text-xs text-gray-400">{analytics.subtitle.count} Teks SRT</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-amber-400">
                                {formatBytes(analytics.subtitle.size)}
                            </div>
                        </div>

                        <div className="p-3 bg-gray-900/40 border border-gray-800/80 rounded-xl flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <FileText className="w-4 h-4 text-gray-400" />
                                <div>
                                    <div className="text-sm font-semibold text-white">Dokumen & Lainnya</div>
                                    <div className="text-xs text-gray-400">{analytics.doc.count} Berkas</div>
                                </div>
                            </div>
                            <div className="text-right font-mono text-sm font-bold text-gray-300">
                                {formatBytes(analytics.doc.size)}
                            </div>
                        </div>
                    </div>

                </div>

                {/* Footer */}
                <div className="px-6 py-4 border-t border-gray-800 bg-black/40 flex items-center justify-between">
                    <button
                        onClick={handlePurgeCache}
                        className="px-4 py-2 bg-red-500/10 border border-red-500/30 text-red-400 rounded-xl hover:bg-red-500/20 text-xs font-semibold font-mono flex items-center gap-2 transition-all"
                    >
                        <Trash2 className="w-3.5 h-3.5" />
                        PURGE LOCAL CACHE
                    </button>

                    <button
                        onClick={onClose}
                        className="px-5 py-2 bg-cyan-500 text-black hover:bg-cyan-400 font-semibold rounded-xl text-xs transition-all font-mono"
                    >
                        CLOSE
                    </button>
                </div>

            </div>
        </div>
    );
}
