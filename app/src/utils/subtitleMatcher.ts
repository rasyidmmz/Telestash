import { TelegramFile } from '../types';
import { parseEpisodeInfo } from './seriesParser';

export type SubtitleFormat = 'vobsub_idx' | 'vobsub_sub' | 'srt' | 'ass' | 'ssa' | 'vtt';

export interface MatchedSubtitle {
    path: string;
    name: string;
    format: SubtitleFormat;
    language: string;
    label: string;
    pairedVobSubPath?: string;
}

export interface SubtitleMatchResult {
    videoFile: TelegramFile;
    matchedSubtitles: MatchedSubtitle[];
}

const SUBTITLE_EXTENSIONS: Record<string, SubtitleFormat> = {
    idx: 'vobsub_idx',
    sub: 'vobsub_sub',
    srt: 'srt',
    ass: 'ass',
    ssa: 'ssa',
    vtt: 'vtt',
};

const LANGUAGE_PATTERNS: Record<string, string> = {
    en: 'en',
    eng: 'en',
    english: 'en',
    id: 'id',
    ind: 'id',
    indonesia: 'id',
    indonesian: 'id',
    ja: 'ja',
    jp: 'ja',
    jpn: 'ja',
    japanese: 'ja',
    ko: 'ko',
    kor: 'ko',
    korean: 'ko',
    zh: 'zh',
    chi: 'zh',
    zho: 'zh',
    chinese: 'zh',
    es: 'es',
    spa: 'es',
    spanish: 'es',
    fr: 'fr',
    fre: 'fr',
    fra: 'fr',
    french: 'fr',
    de: 'de',
    ger: 'de',
    deu: 'de',
    german: 'de',
    ru: 'ru',
    rus: 'ru',
    russian: 'ru',
    ar: 'ar',
    ara: 'ar',
    arabic: 'ar',
};

/**
 * Detect subtitle format from filename or path.
 */
export function detectSubtitleFormat(filePath: string): SubtitleFormat | null {
    const ext = filePath.split('.').pop()?.toLowerCase() || '';
    return SUBTITLE_EXTENSIONS[ext] || null;
}

/**
 * Detect language code from subtitle filename.
 */
export function detectSubtitleLanguage(filename: string): string {
    const nameWithoutExt = filename.replace(/\.[^/.]+$/, '');
    
    // Check tags like .en.srt, .id.idx, [English], (Indonesian), _eng
    const tokens = nameWithoutExt.toLowerCase().split(/[._\s\-[\]()]+/);
    for (const token of tokens) {
        if (LANGUAGE_PATTERNS[token]) {
            return LANGUAGE_PATTERNS[token];
        }
    }
    return 'und';
}

/**
 * Get display label for a language code.
 */
export function getLanguageLabel(lang: string): string {
    switch (lang) {
        case 'en': return 'English';
        case 'id': return 'Indonesian';
        case 'ja': return 'Japanese';
        case 'ko': return 'Korean';
        case 'zh': return 'Chinese';
        case 'es': return 'Spanish';
        case 'fr': return 'French';
        case 'de': return 'German';
        case 'ru': return 'Russian';
        case 'ar': return 'Arabic';
        default: return 'Undetermined';
    }
}

/**
 * Normalize and extract episode identification string (e.g. "S01E01", "9x01")
 */
function extractEpisodeKey(filename: string): string | null {
    const ep = parseEpisodeInfo(filename);
    if (ep.isEpisode && ep.season !== null && ep.episode !== null) {
        return `S${ep.season.toString().padStart(2, '0')}E${ep.episode.toString().padStart(2, '0')}`;
    }

    // Fallback: search regex directly
    const match = filename.match(/(?:s|season)?(\d{1,2})[x.e_ -]+(\d{1,3})/i);
    if (match && match[1] && match[2]) {
        const s = parseInt(match[1], 10);
        const e = parseInt(match[2], 10);
        return `S${s.toString().padStart(2, '0')}E${e.toString().padStart(2, '0')}`;
    }
    return null;
}

/**
 * Clean filename stem for fuzzy matching.
 */
function cleanStem(name: string): string {
    return name
        .replace(/\.[^/.]+$/, '') // remove extension
        .replace(/\.(en|eng|id|ind|ja|jpn|es|fr|de|ru|und)$/i, '') // remove language tag
        .replace(/[._\s\-]+/g, ' ')
        .toLowerCase()
        .trim();
}

/**
 * Match a list of subtitle file paths to a list of TelegramFile video objects.
 */
export function matchSubtitlesToVideos(
    videos: TelegramFile[],
    subtitlePaths: string[]
): SubtitleMatchResult[] {
    const results: Map<number, SubtitleMatchResult> = new Map();

    // Map videos by ID and episode key / cleaned stem
    const videoEpMap = new Map<string, TelegramFile[]>();
    const videoStemMap = new Map<string, TelegramFile>();

    for (const v of videos) {
        results.set(v.id, { videoFile: v, matchedSubtitles: [] });
        const epKey = extractEpisodeKey(v.name);
        if (epKey) {
            if (!videoEpMap.has(epKey)) {
                videoEpMap.set(epKey, []);
            }
            videoEpMap.get(epKey)!.push(v);
        }
        videoStemMap.set(cleanStem(v.name), v);
    }

    // Identify VobSub pairs (.idx and .sub)
    const vobSubPairs = new Map<string, { idx?: string; sub?: string }>();
    const regularSubs: string[] = [];

    for (const subPath of subtitlePaths) {
        const format = detectSubtitleFormat(subPath);
        if (!format) continue;

        if (format === 'vobsub_idx' || format === 'vobsub_sub') {
            const base = subPath.replace(/\.(idx|sub)$/i, '');
            if (!vobSubPairs.has(base)) {
                vobSubPairs.set(base, {});
            }
            if (format === 'vobsub_idx') {
                vobSubPairs.get(base)!.idx = subPath;
            } else {
                vobSubPairs.get(base)!.sub = subPath;
            }
        } else {
            regularSubs.push(subPath);
        }
    }

    // Process VobSub pairs (only process the .idx as the primary entry with linked .sub)
    for (const [, pair] of vobSubPairs) {
        if (!pair.idx) continue;
        const subPath = pair.idx;
        const filename = subPath.split(/[/\\]/).pop() || subPath;
        const lang = detectSubtitleLanguage(filename);
        const epKey = extractEpisodeKey(filename);
        const stem = cleanStem(filename);

        let matchedVideo: TelegramFile | undefined;

        if (epKey && videoEpMap.has(epKey)) {
            matchedVideo = videoEpMap.get(epKey)![0];
        } else if (videoStemMap.has(stem)) {
            matchedVideo = videoStemMap.get(stem);
        }

        if (matchedVideo) {
            const entry = results.get(matchedVideo.id);
            if (entry) {
                entry.matchedSubtitles.push({
                    path: subPath,
                    name: filename,
                    format: 'vobsub_idx',
                    language: lang,
                    label: `${getLanguageLabel(lang)} (VobSub)`,
                    pairedVobSubPath: pair.sub,
                });
            }
        }
    }

    // Process regular subtitle files (.srt, .ass, .ssa, .vtt)
    for (const subPath of regularSubs) {
        const filename = subPath.split(/[/\\]/).pop() || subPath;
        const format = detectSubtitleFormat(subPath)!;
        const lang = detectSubtitleLanguage(filename);
        const epKey = extractEpisodeKey(filename);
        const stem = cleanStem(filename);

        let matchedVideo: TelegramFile | undefined;

        if (epKey && videoEpMap.has(epKey)) {
            matchedVideo = videoEpMap.get(epKey)![0];
        } else if (videoStemMap.has(stem)) {
            matchedVideo = videoStemMap.get(stem);
        }

        if (matchedVideo) {
            const entry = results.get(matchedVideo.id);
            if (entry) {
                entry.matchedSubtitles.push({
                    path: subPath,
                    name: filename,
                    format,
                    language: lang,
                    label: `${getLanguageLabel(lang)} (${format.toUpperCase()})`,
                });
            }
        }
    }

    // Return only videos that have at least one matched subtitle
    return Array.from(results.values()).filter(r => r.matchedSubtitles.length > 0);
}
