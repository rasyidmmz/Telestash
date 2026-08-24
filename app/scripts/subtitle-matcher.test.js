import test from 'node:test';
import assert from 'node:assert/strict';

const SUBTITLE_EXTENSIONS = {
    idx: 'vobsub_idx',
    sub: 'vobsub_sub',
    srt: 'srt',
    ass: 'ass',
    ssa: 'ssa',
    vtt: 'vtt',
};

const LANGUAGE_PATTERNS = {
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
};

function detectSubtitleFormat(filePath) {
    const ext = filePath.split('.').pop()?.toLowerCase() || '';
    return SUBTITLE_EXTENSIONS[ext] || null;
}

function detectSubtitleLanguage(filename) {
    const nameWithoutExt = filename.replace(/\.[^/.]+$/, '');
    const tokens = nameWithoutExt.toLowerCase().split(/[._\s\-[\]()]+/);
    for (const token of tokens) {
        if (LANGUAGE_PATTERNS[token]) {
            return LANGUAGE_PATTERNS[token];
        }
    }
    return 'und';
}

function extractEpisodeKey(filename) {
    const sxE = filename.match(/(?:^|[.\s_\-[])(?:s|season)[.\s_-]*(\d+)[.\s_-]*(?:e|ep|episode)[.\s_-]*(\d+)/i);
    if (sxE && sxE[1] && sxE[2]) {
        return `S${parseInt(sxE[1], 10).toString().padStart(2, '0')}E${parseInt(sxE[2], 10).toString().padStart(2, '0')}`;
    }
    const nxE = filename.match(/(?:^|[.\s_\-[])(\d{1,2})x(\d{1,3})(?:[.\s_\-–—\]]|$)/i);
    if (nxE && nxE[1] && nxE[2]) {
        return `S${parseInt(nxE[1], 10).toString().padStart(2, '0')}E${parseInt(nxE[2], 10).toString().padStart(2, '0')}`;
    }
    return null;
}

function cleanStem(name) {
    return name
        .replace(/\.[^/.]+$/, '')
        .replace(/\.(en|eng|id|ind|ja|jpn|es|fr|de|ru|und)$/i, '')
        .replace(/[._\s\-]+/g, ' ')
        .toLowerCase()
        .trim();
}

function matchSubtitlesToVideos(videos, subtitlePaths) {
    const results = new Map();
    const videoEpMap = new Map();
    const videoStemMap = new Map();

    for (const v of videos) {
        results.set(v.id, { videoFile: v, matchedSubtitles: [] });
        const epKey = extractEpisodeKey(v.name);
        if (epKey) {
            if (!videoEpMap.has(epKey)) {
                videoEpMap.set(epKey, []);
            }
            videoEpMap.get(epKey).push(v);
        }
        videoStemMap.set(cleanStem(v.name), v);
    }

    const vobSubPairs = new Map();
    const regularSubs = [];

    for (const subPath of subtitlePaths) {
        const format = detectSubtitleFormat(subPath);
        if (!format) continue;

        if (format === 'vobsub_idx' || format === 'vobsub_sub') {
            const base = subPath.replace(/\.(idx|sub)$/i, '');
            if (!vobSubPairs.has(base)) {
                vobSubPairs.set(base, {});
            }
            if (format === 'vobsub_idx') {
                vobSubPairs.get(base).idx = subPath;
            } else {
                vobSubPairs.get(base).sub = subPath;
            }
        } else {
            regularSubs.push(subPath);
        }
    }

    for (const [, pair] of vobSubPairs) {
        if (!pair.idx) continue;
        const subPath = pair.idx;
        const filename = subPath.split(/[/\\]/).pop() || subPath;
        const lang = detectSubtitleLanguage(filename);
        const epKey = extractEpisodeKey(filename);
        const stem = cleanStem(filename);

        let matchedVideo;
        if (epKey && videoEpMap.has(epKey)) {
            matchedVideo = videoEpMap.get(epKey)[0];
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
                    label: `${lang === 'en' ? 'English' : 'Undetermined'} (VobSub)`,
                    pairedVobSubPath: pair.sub,
                });
            }
        }
    }

    for (const subPath of regularSubs) {
        const filename = subPath.split(/[/\\]/).pop() || subPath;
        const format = detectSubtitleFormat(subPath);
        const lang = detectSubtitleLanguage(filename);
        const epKey = extractEpisodeKey(filename);
        const stem = cleanStem(filename);

        let matchedVideo;
        if (epKey && videoEpMap.has(epKey)) {
            matchedVideo = videoEpMap.get(epKey)[0];
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
                    label: `${lang === 'en' ? 'English' : (lang === 'id' ? 'Indonesian' : 'Undetermined')} (${format.toUpperCase()})`,
                });
            }
        }
    }

    return Array.from(results.values()).filter(r => r.matchedSubtitles.length > 0);
}

test('detectSubtitleFormat identifies correct subtitle formats', () => {
  assert.equal(detectSubtitleFormat('sub/The.Office.1x01.idx'), 'vobsub_idx');
  assert.equal(detectSubtitleFormat('sub/The.Office.1x01.sub'), 'vobsub_sub');
  assert.equal(detectSubtitleFormat('sub/The.Office.S01E01.en.srt'), 'srt');
  assert.equal(detectSubtitleFormat('sub/Anime.Episode.01.ass'), 'ass');
  assert.equal(detectSubtitleFormat('sub/Movie.vtt'), 'vtt');
  assert.equal(detectSubtitleFormat('sub/Movie.mp4'), null);
});

test('detectSubtitleLanguage extracts language codes correctly', () => {
  assert.equal(detectSubtitleLanguage('The.Office.1x01.en.idx'), 'en');
  assert.equal(detectSubtitleLanguage('The.Office.1x01.eng.idx'), 'en');
  assert.equal(detectSubtitleLanguage('The.Office.1x01.id.srt'), 'id');
  assert.equal(detectSubtitleLanguage('The.Office.1x01.ind.srt'), 'id');
  assert.equal(detectSubtitleLanguage('The.Office.1x01.indonesia.srt'), 'id');
  assert.equal(detectSubtitleLanguage('The.Office.1x01.ja.ass'), 'ja');
  assert.equal(detectSubtitleLanguage('The.Office.1x01.idx'), 'und');
});

test('matchSubtitlesToVideos pairs VobSub .idx and .sub with matching video files', () => {
  const videos = [
    { id: 101, name: 'The Office S01E01 1080p.mkv', size: 1000, sizeStr: '1 GB' },
    { id: 102, name: 'The Office S01E02 1080p.mkv', size: 1000, sizeStr: '1 GB' },
  ];

  const subFiles = [
    'C:/Series/sub/The Office S01E01.idx',
    'C:/Series/sub/The Office S01E01.sub',
    'C:/Series/sub/The Office S01E02.idx',
    'C:/Series/sub/The Office S01E02.sub',
    'C:/Series/sub/Unrelated Movie.srt',
  ];

  const matches = matchSubtitlesToVideos(videos, subFiles);
  assert.equal(matches.length, 2);

  const ep1 = matches.find(m => m.videoFile.id === 101);
  assert.ok(ep1);
  assert.equal(ep1.matchedSubtitles.length, 1);
  assert.equal(ep1.matchedSubtitles[0].format, 'vobsub_idx');
  assert.ok(ep1.matchedSubtitles[0].path.endsWith('The Office S01E01.idx'));
  assert.ok(ep1.matchedSubtitles[0].pairedVobSubPath.endsWith('The Office S01E01.sub'));

  const ep2 = matches.find(m => m.videoFile.id === 102);
  assert.ok(ep2);
  assert.equal(ep2.matchedSubtitles.length, 1);
  assert.equal(ep2.matchedSubtitles[0].format, 'vobsub_idx');
  assert.ok(ep2.matchedSubtitles[0].path.endsWith('The Office S01E02.idx'));
  assert.ok(ep2.matchedSubtitles[0].pairedVobSubPath.endsWith('The Office S01E02.sub'));
});

test('matchSubtitlesToVideos pairs multi-language .srt files and 1x01 notation', () => {
  const videos = [
    { id: 201, name: 'The Office (US) - 9x01 - New Guys.mkv', size: 500, sizeStr: '500 MB' },
  ];

  const subFiles = [
    'C:/Subs/9x01.en.srt',
    'C:/Subs/9x01.id.srt',
  ];

  const matches = matchSubtitlesToVideos(videos, subFiles);
  assert.equal(matches.length, 1);
  assert.equal(matches[0].matchedSubtitles.length, 2);
  const langs = matches[0].matchedSubtitles.map(s => s.language).sort();
  assert.deepEqual(langs, ['en', 'id']);
});
