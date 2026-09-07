import test from 'node:test';
import assert from 'node:assert/strict';

// Mirror of app/src/utils/mediaTitle.ts (node --test runs without a TS loader).
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

function parseMediaTitle(filename) {
    let base = filename.replace(/\.[a-z0-9]{2,5}$/i, '');

    const kind = EPISODE_PATTERNS.some((re) => re.test(base)) ? 'tv' : 'movie';

    let year;
    const yearMatch = base.match(YEAR_PATTERN);
    if (yearMatch && yearMatch.index !== undefined) {
        year = parseInt(yearMatch[1], 10);
        base = base.slice(0, yearMatch.index);
    }

    if (!year) {
        base = base.replace(RELEASE_TAG_PATTERN, ' ');
    }
    for (const re of EPISODE_PATTERNS) {
        base = base.replace(re, ' ');
    }

    const title = base
        .replace(NOISE_PATTERN, ' ')
        .replace(/\s+/g, ' ')
        .trim()
        .replace(/^[-–]+\s*|\s*[-–]+$/g, '');

    return { title: title || filename, year, kind };
}

test('parses a movie filename with year and release tags', () => {
    const info = parseMediaTitle('The.Dark.Knight.2008.1080p.BluRay.x264.mkv');
    assert.equal(info.title, 'The Dark Knight');
    assert.equal(info.year, 2008);
    assert.equal(info.kind, 'movie');
});

test('parses a series filename via S01E01 marker', () => {
    const info = parseMediaTitle('Breaking.Bad.S01E03.720p.WEB-DL.x265.mkv');
    assert.equal(info.title, 'Breaking Bad');
    assert.equal(info.kind, 'tv');
});

test('detects series without a year', () => {
    const info = parseMediaTitle('Superfan.S08E24.mkv');
    assert.equal(info.kind, 'tv');
    assert.equal(info.title, 'Superfan');
    assert.equal(info.year, undefined);
});

test('handles plain movie name without any metadata', () => {
    const info = parseMediaTitle('Home.Video.2020.mp4');
    // Year found at 2020; text before it is the title ("Home Video").
    assert.equal(info.year, 2020);
    assert.equal(info.title, 'Home Video');
    assert.equal(info.kind, 'movie');
});

test('strips release tags when no year is present', () => {
    const info = parseMediaTitle('Arrival.2016.1080p.WEB-DL.DDP5.1.Atmos.HEVC.mkv');
    assert.equal(info.title, 'Arrival');
    assert.equal(info.year, 2016);
});

test('falls back to the raw name for garbage titles', () => {
    const info = parseMediaTitle('video_file.mp4');
    assert.equal(info.title, 'video file');
    assert.equal(info.kind, 'movie');
});
