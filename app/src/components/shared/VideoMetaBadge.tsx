import { Clock } from '@phosphor-icons/react';
import { VideoMetadata } from '../../types';

interface VideoMetaBadgeProps {
    metadata: VideoMetadata | null | undefined;
    isLoading: boolean;
    filename?: string;
    hideResolution?: boolean;
}

function formatDuration(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    if (m >= 60) {
        const h = Math.floor(m / 60);
        const rm = m % 60;
        return `${h}h ${String(rm).padStart(2, '0')}m`;
    }
    return `${m}m ${String(s).padStart(2, '0')}s`;
}

function getResolutionTag(width: number): string {
    if (width >= 3840) return '4K';
    if (width >= 1920) return '1080p';
    if (width >= 1280) return '720p';
    return `${width}p`;
}

export function VideoMetaBadge({ metadata, isLoading, filename, hideResolution }: VideoMetaBadgeProps) {
    if (isLoading || !metadata) return null;

    const hasDuration = typeof metadata.duration_secs === 'number' && metadata.duration_secs > 0;
    const hasResolution = typeof metadata.width === 'number' && metadata.width > 0;

    if (!hasDuration && !hasResolution) return null;

    const filenameHasRes = filename ? /(?:2160p|4k|uhd|1080p|fhd|720p|hd|480p|sd)/i.test(filename) : false;
    const shouldShowRes = hasResolution && !hideResolution && !filenameHasRes;
    const resTag = shouldShowRes ? getResolutionTag(metadata.width!) : null;

    return (
        <span className="inline-flex items-center gap-1.5 text-[10px] font-mono tracking-wider">
            {resTag && (
                <span className="px-1.5 py-0.5 rounded bg-cyan-950/60 border border-cyan-500/30 text-cyan-400 font-bold uppercase">
                    {resTag}
                </span>
            )}
            {hasDuration && (
                <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-slate-900 border border-gray-800 text-gray-300">
                    <Clock className="w-2.5 h-2.5 text-cyan-400" />
                    {formatDuration(metadata.duration_secs!)}
                </span>
            )}
        </span>
    );
}
