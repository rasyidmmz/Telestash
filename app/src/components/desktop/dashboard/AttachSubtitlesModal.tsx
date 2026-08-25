import React, { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Subtitles, FolderOpen, CheckCircle2, AlertCircle, Loader2, X, FileText, Globe } from 'lucide-react';
import { TelegramFile } from '../../../types';
import { isVideoFile } from '../../../utils';
import { matchSubtitlesToVideos, SubtitleMatchResult, getLanguageLabel } from '../../../utils/subtitleMatcher';

interface AttachSubtitlesModalProps {
    isOpen: boolean;
    onClose: () => void;
    folderId: number | null;
    files: TelegramFile[];
    onSuccess?: () => void;
}

export const AttachSubtitlesModal: React.FC<AttachSubtitlesModalProps> = ({
    isOpen,
    onClose,
    folderId,
    files,
    onSuccess,
}) => {
    const queryClient = useQueryClient();
    const [selectedPath, setSelectedPath] = useState<string>('');
    const [matches, setMatches] = useState<SubtitleMatchResult[]>([]);
    const [isScanning, setIsScanning] = useState(false);
    const [isUploading, setIsUploading] = useState(false);
    const [uploadProgress, setUploadProgress] = useState<{ current: number; total: number; currentName: string }>({
        current: 0,
        total: 0,
        currentName: '',
    });
    const [error, setError] = useState<string | null>(null);

    const handleClose = () => {
        if (!isUploading) {
            setSelectedPath('');
            setMatches([]);
            setUploadProgress({ current: 0, total: 0, currentName: '' });
            setError(null);
            onClose();
        }
    };

    if (!isOpen) return null;

    const videoFiles = files.filter(f => isVideoFile(f.name));

    const handlePickDirectory = async () => {
        try {
            setError(null);
            const selected = await open({
                directory: true,
                multiple: false,
                title: 'Select Subtitles Folder (e.g. sub/)',
            });

            if (selected && typeof selected === 'string') {
                setSelectedPath(selected);
                setIsScanning(true);
                
                // Read directory contents via Tauri fs or scan command
                const subPaths = await invoke<string[]>('cmd_list_directory_files', { path: selected })
                    .catch(async () => {
                        // Fallback: if cmd_list_directory_files is not registered, we can also support file selection
                        return [] as string[];
                    });

                const matchedResults = matchSubtitlesToVideos(videoFiles, subPaths);
                setMatches(matchedResults);
                setIsScanning(false);
            }
        } catch (e: any) {
            setIsScanning(false);
            setError(e?.toString() || 'Failed to scan selected folder');
        }
    };

    const handlePickFiles = async () => {
        try {
            setError(null);
            const selected = await open({
                directory: false,
                multiple: true,
                title: 'Select Subtitle Files (.idx, .sub, .srt, .ass, .vtt)',
                filters: [{
                    name: 'Subtitles',
                    extensions: ['idx', 'sub', 'srt', 'ass', 'ssa', 'vtt'],
                }],
            });

            if (selected) {
                const paths = Array.isArray(selected) ? selected : [selected];
                setSelectedPath(`${paths.length} subtitle files selected`);
                setIsScanning(true);
                const matchedResults = matchSubtitlesToVideos(videoFiles, paths);
                setMatches(matchedResults);
                setIsScanning(false);
            }
        } catch (e: any) {
            setIsScanning(false);
            setError(e?.toString() || 'Failed to select subtitle files');
        }
    };

    const handleStartUpload = async () => {
        if (matches.length === 0) return;
        setIsUploading(true);
        setError(null);

        let totalSubtitles = 0;
        matches.forEach(m => totalSubtitles += m.matchedSubtitles.length);

        let processed = 0;
        setUploadProgress({ current: 0, total: totalSubtitles, currentName: '' });

        try {
            for (const match of matches) {
                for (const sub of match.matchedSubtitles) {
                    setUploadProgress({
                        current: processed + 1,
                        total: totalSubtitles,
                        currentName: `${match.videoFile.name} ➔ ${sub.name}`,
                    });

                    await invoke('cmd_attach_video_subtitles', {
                        folderId,
                        videoMessageId: match.videoFile.id,
                        videoFileName: match.videoFile.name,
                        primaryPath: sub.path,
                        pairedPath: sub.pairedVobSubPath || null,
                        format: sub.format,
                        language: sub.language,
                        label: sub.label,
                    });

                    processed++;
                    setUploadProgress(prev => ({ ...prev, current: processed }));
                }
            }

            setIsUploading(false);
            toast.success(`Successfully attached ${totalSubtitles} subtitle track${totalSubtitles > 1 ? 's' : ''}!`);
            queryClient.invalidateQueries({ queryKey: ['video-subtitles'] });
            setSelectedPath('');
            setMatches([]);
            setUploadProgress({ current: 0, total: 0, currentName: '' });
            setError(null);
            if (onSuccess) onSuccess();
            onClose();
        } catch (e: any) {
            setIsUploading(false);
            setError(e?.toString() || 'Failed to attach subtitle');
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4 animate-in fade-in duration-200">
            <div className="bg-slate-900 border border-slate-700/80 rounded-2xl w-full max-w-3xl max-h-[85vh] flex flex-col shadow-2xl overflow-hidden text-slate-200">
                {/* Header */}
                <div className="px-6 py-4 border-b border-slate-800 flex items-center justify-between bg-slate-950/50">
                    <div className="flex items-center gap-3">
                        <div className="p-2.5 bg-blue-500/10 text-blue-400 rounded-xl border border-blue-500/20">
                            <Subtitles className="w-6 h-6" />
                        </div>
                        <div>
                            <h2 className="text-lg font-semibold text-white flex items-center gap-2">
                                Attach External Subtitles
                            </h2>
                            <p className="text-xs text-slate-400">
                                Match and link .idx/.sub (VobSub), .srt, .ass, or .vtt subtitles without cluttering your series folder.
                            </p>
                        </div>
                    </div>
                    {!isUploading && (
                        <button
                            onClick={handleClose}
                            className="p-2 text-slate-400 hover:text-white hover:bg-slate-800 rounded-lg transition-colors"
                        >
                            <X className="w-5 h-5" />
                        </button>
                    )}
                </div>

                {/* Body */}
                <div className="p-6 overflow-y-auto flex-1 space-y-6">
                    {/* Picker Buttons */}
                    {!isUploading && (
                        <div className="grid grid-cols-2 gap-4">
                            <button
                                onClick={handlePickDirectory}
                                className="flex flex-col items-center justify-center gap-2 p-5 border border-dashed border-slate-700 hover:border-blue-500 bg-slate-800/40 hover:bg-blue-500/5 rounded-xl transition-all group"
                            >
                                <FolderOpen className="w-8 h-8 text-blue-400 group-hover:scale-110 transition-transform" />
                                <span className="text-sm font-medium text-white">Select 'sub/' Folder</span>
                                <span className="text-xs text-slate-400 text-center">Automatically scan all subtitle pairs inside folder</span>
                            </button>

                            <button
                                onClick={handlePickFiles}
                                className="flex flex-col items-center justify-center gap-2 p-5 border border-dashed border-slate-700 hover:border-blue-500 bg-slate-800/40 hover:bg-blue-500/5 rounded-xl transition-all group"
                            >
                                <FileText className="w-8 h-8 text-indigo-400 group-hover:scale-110 transition-transform" />
                                <span className="text-sm font-medium text-white">Select Subtitle Files</span>
                                <span className="text-xs text-slate-400 text-center">Pick multiple .idx, .sub, .srt, or .ass files directly</span>
                            </button>
                        </div>
                    )}

                    {/* Path & Status */}
                    {selectedPath && (
                        <div className="text-xs bg-slate-800/60 border border-slate-700/50 rounded-lg p-3 flex items-center justify-between">
                            <span className="text-slate-300 truncate max-w-md">📂 {selectedPath}</span>
                            <span className="text-blue-400 font-medium">{matches.length} matched episodes</span>
                        </div>
                    )}

                    {/* Error Banner */}
                    {error && (
                        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-3.5 flex items-center gap-3 text-red-400 text-sm">
                            <AlertCircle className="w-5 h-5 flex-shrink-0" />
                            <span>{error}</span>
                        </div>
                    )}

                    {/* Scanning Indicator */}
                    {isScanning && (
                        <div className="py-12 flex flex-col items-center justify-center gap-3 text-slate-400">
                            <Loader2 className="w-8 h-8 animate-spin text-blue-400" />
                            <span className="text-sm font-medium">Matching subtitles to episodes...</span>
                        </div>
                    )}

                    {/* Upload Progress */}
                    {isUploading && (
                        <div className="py-8 px-4 bg-slate-800/50 border border-slate-700 rounded-xl space-y-4">
                            <div className="flex items-center justify-between text-sm">
                                <span className="font-medium text-white flex items-center gap-2">
                                    <Loader2 className="w-4 h-4 animate-spin text-blue-400" />
                                    Attaching subtitles ({uploadProgress.current}/{uploadProgress.total})...
                                </span>
                                <span className="text-blue-400 font-semibold">
                                    {Math.round((uploadProgress.current / Math.max(1, uploadProgress.total)) * 100)}%
                                </span>
                            </div>
                            <div className="w-full bg-slate-700 h-2.5 rounded-full overflow-hidden">
                                <div
                                    className="bg-blue-500 h-full transition-all duration-300"
                                    style={{ width: `${(uploadProgress.current / Math.max(1, uploadProgress.total)) * 100}%` }}
                                />
                            </div>
                            <p className="text-xs text-slate-400 truncate">{uploadProgress.currentName}</p>
                        </div>
                    )}

                    {/* Matched Preview List */}
                    {!isScanning && matches.length > 0 && (
                        <div className="space-y-3">
                            <h3 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
                                Matched Episodes & Subtitle Tracks ({matches.length})
                            </h3>
                            <div className="border border-slate-800 rounded-xl divide-y divide-slate-800/60 overflow-hidden bg-slate-950/30">
                                {matches.map(m => (
                                    <div key={m.videoFile.id} className="p-3.5 flex items-start justify-between gap-4 hover:bg-slate-800/30 transition-colors">
                                        <div className="min-w-0 flex-1">
                                            <div className="text-sm font-medium text-white truncate">
                                                {m.videoFile.name}
                                            </div>
                                            <div className="mt-1 flex flex-wrap gap-2">
                                                {m.matchedSubtitles.map((s, idx) => (
                                                    <div key={idx} className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-blue-500/10 border border-blue-500/20 text-blue-300 text-xs">
                                                        <Globe className="w-3 h-3 text-blue-400" />
                                                        <span>{getLanguageLabel(s.language)}</span>
                                                        <span className="text-[10px] text-blue-400/80 uppercase font-mono">({s.format.replace('vobsub_', '')})</span>
                                                        {s.pairedVobSubPath && <span className="text-[10px] text-emerald-400 font-mono">+sub</span>}
                                                    </div>
                                                ))}
                                            </div>
                                        </div>
                                        <div className="flex-shrink-0 flex items-center text-emerald-400 text-xs gap-1 pt-1">
                                            <CheckCircle2 className="w-4 h-4" />
                                            <span>Ready</span>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        </div>
                    )}

                    {!isScanning && selectedPath && matches.length === 0 && (
                        <div className="py-8 text-center text-slate-400 text-sm">
                            No matching video files found for the subtitles in this location.
                        </div>
                    )}
                </div>

                {/* Footer */}
                <div className="px-6 py-4 border-t border-slate-800 bg-slate-950/50 flex items-center justify-end gap-3">
                    <button
                        onClick={handleClose}
                        disabled={isUploading}
                        className="px-4 py-2 text-sm font-medium text-slate-400 hover:text-white hover:bg-slate-800 rounded-xl transition-colors disabled:opacity-50"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={handleStartUpload}
                        disabled={matches.length === 0 || isUploading}
                        className="px-5 py-2 text-sm font-medium bg-blue-600 hover:bg-blue-500 text-white rounded-xl shadow-lg shadow-blue-500/20 transition-all disabled:opacity-50 disabled:pointer-events-none flex items-center gap-2"
                    >
                        {isUploading ? (
                            <>
                                <Loader2 className="w-4 h-4 animate-spin" />
                                Attaching Subtitles...
                            </>
                        ) : (
                            <>
                                <Subtitles className="w-4 h-4" />
                                Attach & Upload Subtitles ({matches.length})
                            </>
                        )}
                    </button>
                </div>
            </div>
        </div>
    );
};
