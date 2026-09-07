/**
 * Filename → media title parsing for TMDB matching (Tier 2).
 *
 * Extracts a clean title, release year, and movie/series kind from vault
 * filenames like "The.Dark.Knight.2008.1080p.BluRay.x264.mkv" or
 * "Breaking.Bad.S01E03.720p.mkv". Pure functions, mirrored in
 * scripts/media-title.test.js (node --test, no TS loader).
 */

export interface MediaTitleInfo {
    title: string;
    year?: number;
    kind: 'movie' | 'tv';
}

const EPISODE_PATTERNS = [
    /\bS\d{1,2}\s?[-.]?E\s?\d{1,3}\b/i,
    /\b\d{1,2}x\d{1,3}\b/i,
    /\b(?:EP?|E)\s?\d{1,3}\b/i,
    /\bEpisode\s?\d{1,3}\b/i,
];

const YEAR_PATTERN = /\b(19\d{2}|20\d{2})\b/;

const RELEASE_TAG_PATTERN = new RegExp(
    [
        '\\b(?:2160p|1080p|1080i|720p|480p|4k|uhd)\\b',
        '\\b(?:x264|x265|h264|h265|hevc|av1|xvid|divx)\\b',
        '\\b(?:hdr10?\\+?|dolby[ .]?vision|dv)\\b',
        '\\b(?:web[ .-]?(?:dl|rip)|blu[ .-]?ray|bdrip|brrip|dvdrip|remux|hdtv|cam|ts|dvdscr)\\b',
        '\\b(?:aac\\d?|eac3|ac3|dts(?:[ .-]?(?:hd|hdma|x))?|truehd|atmos|flac|mp3|opus)\\b',
        '\\b(?:10bit|8bit|dual[ .-]?audio|multi|subbed|dubbed)\\b',
        '\\b(?:proper|repack|extended|uncut|remastered|imax|internal|widevine|hmax|nf|amzn|dsnp|atvp)\\b',
        '\\b(?:ddp?\\d?\\d?|5[ .]1|7[ .]1|2[ .]0)\\b',
    ].join('|'),
    'gi',
);

const NOISE_PATTERN = /[[{()}\]._-]/g;

export function parseMediaTitle(filename: string): MediaTitleInfo {
    let base = filename.replace(/\.[a-z0-9]{2,5}$/i, '');

    const kind: 'movie' | 'tv' = EPISODE_PATTERNS.some((re) => re.test(base)) ? 'tv' : 'movie';

    let year: number | undefined;
    const yearMatch = base.match(YEAR_PATTERN);
    if (yearMatch && yearMatch.index !== undefined) {
        year = parseInt(yearMatch[1], 10);
        // Text before the year is the strongest title signal; drop the rest
        // (release tags, group names) wholesale.
        base = base.slice(0, yearMatch.index);
    }

    if (!year) {
        // Still strip tags so titles like "Show.S01E03.1080p" without a year
        // position advantage stay clean.
        base = base.replace(RELEASE_TAG_PATTERN, ' ');
    }
    // Series markers never belong in the search title.
    for (const re of EPISODE_PATTERNS) {
        base = base.replace(re, ' ');
    }

    const title = base
        .replace(NOISE_PATTERN, ' ')
        .replace(/\s+/g, ' ')
        .trim()
        .replace(/^[-–]+\s*|\s*[-–]+$/g, '');

    return {
        title: title || filename,
        year,
        kind,
    };
}
