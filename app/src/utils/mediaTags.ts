import { parseEpisodeInfo } from './seriesParser';
import { isVideoFile } from '../utils';

export interface MediaBadge {
    type: 'episode' | 'resolution' | 'hdr' | 'codec' | 'audio';
    label: string;
    variant: 'cyan' | 'emerald' | 'amber' | 'purple' | 'slate';
}

export function extractMediaBadges(filename: string): MediaBadge[] {
    if (!isVideoFile(filename)) return [];

    const badges: MediaBadge[] = [];
    const lower = filename.toLowerCase();

    // 1. Episode Badge
    const epInfo = parseEpisodeInfo(filename);
    if (epInfo.isEpisode && epInfo.displayBadge) {
        badges.push({
            type: 'episode',
            label: epInfo.displayBadge,
            variant: 'emerald',
        });
    }

    // 2. Resolution Badge
    if (lower.includes('2160p') || lower.includes('4k') || lower.includes('uhd')) {
        badges.push({ type: 'resolution', label: '4K UHD', variant: 'cyan' });
    } else if (lower.includes('1080p') || lower.includes('fhd')) {
        badges.push({ type: 'resolution', label: '1080p', variant: 'cyan' });
    } else if (lower.includes('720p') || lower.includes('hd')) {
        badges.push({ type: 'resolution', label: '720p', variant: 'slate' });
    } else if (lower.includes('480p') || lower.includes('sd')) {
        badges.push({ type: 'resolution', label: '480p', variant: 'slate' });
    }

    // 3. HDR / Color Bit-Depth
    if (lower.includes('dolby vision') || lower.includes('dovi') || lower.includes('dv')) {
        badges.push({ type: 'hdr', label: 'DV', variant: 'purple' });
    } else if (lower.includes('hdr10+') || lower.includes('hdr10plus')) {
        badges.push({ type: 'hdr', label: 'HDR10+', variant: 'amber' });
    } else if (lower.includes('hdr')) {
        badges.push({ type: 'hdr', label: 'HDR', variant: 'amber' });
    }

    if (lower.includes('10bit') || lower.includes('10-bit') || lower.includes('hi10p')) {
        badges.push({ type: 'hdr', label: '10-bit', variant: 'amber' });
    }

    // 4. Codec
    if (lower.includes('hevc') || lower.includes('x265') || lower.includes('h.265') || lower.includes('h265')) {
        badges.push({ type: 'codec', label: 'HEVC', variant: 'slate' });
    } else if (lower.includes('av1')) {
        badges.push({ type: 'codec', label: 'AV1', variant: 'slate' });
    } else if (lower.includes('x264') || lower.includes('h.264') || lower.includes('h264') || lower.includes('avc')) {
        badges.push({ type: 'codec', label: 'x264', variant: 'slate' });
    }

    // 5. Audio
    if (lower.includes('dual audio') || lower.includes('dual-audio')) {
        badges.push({ type: 'audio', label: 'Dual Audio', variant: 'purple' });
    } else if (lower.includes('multi audio') || lower.includes('multi-audio')) {
        badges.push({ type: 'audio', label: 'Multi Audio', variant: 'purple' });
    } else if (lower.includes('5.1') || lower.includes('5.1ch') || lower.includes('ddp5.1')) {
        badges.push({ type: 'audio', label: '5.1 CH', variant: 'slate' });
    } else if (lower.includes('7.1') || lower.includes('7.1ch') || lower.includes('atmos')) {
        badges.push({ type: 'audio', label: 'Atmos', variant: 'purple' });
    }

    return badges;
}
