import test from 'node:test';
import assert from 'node:assert/strict';

// Test series episode parsing patterns
function isVideoFile(name) {
    return /\.(mp4|mkv|avi|mov|webm|flv|ts|m4v)$/i.test(name);
}

function parseEpisodeInfo(filename) {
    if (!isVideoFile(filename)) {
        return { isEpisode: false, season: null, episode: null };
    }
    const baseName = filename.replace(/\.[^/.]+$/, '');

    const sxE = baseName.match(/(?:s|season)[.\s_-]*(\d+)[.\s_-]*(?:e|ep|episode)[.\s_-]*(\d+)/i);
    if (sxE && sxE[1] && sxE[2]) {
        const season = parseInt(sxE[1], 10);
        const episode = parseInt(sxE[2], 10);
        const sPad = season.toString().padStart(2, '0');
        const ePad = episode.toString().padStart(2, '0');
        const titleMatch = baseName.split(sxE[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? titleMatch.replace(/[._]/g, ' ') : undefined;
        return { isEpisode: true, season, episode, displayBadge: `S${sPad}E${ePad}`, seriesTitle };
    }

    const epOnly = baseName.match(/(?:^|[.\s_\-[])(?:e|ep|episode)[.\s_-]*(\d+)(?:\b|v\d+|[.\s_\-\]])/i);
    if (epOnly && epOnly[1]) {
        const episode = parseInt(epOnly[1], 10);
        const ePad = episode.toString().padStart(2, '0');
        const seasonMatch = baseName.match(/(?:s|season)[.\s_-]*(\d+)/i);
        const season = seasonMatch && seasonMatch[1] ? parseInt(seasonMatch[1], 10) : 1;
        const titleMatch = baseName.split(epOnly[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? titleMatch.replace(/[._]/g, ' ') : undefined;
        return { isEpisode: true, season, episode, displayBadge: `EP ${ePad}`, seriesTitle };
    }

    const dashNum = baseName.match(/[-–—]\s*(\d{1,3})(?:\s*\[|\s*\(|\s*\.|\s*$)/);
    if (dashNum && dashNum[1]) {
        const episode = parseInt(dashNum[1], 10);
        const ePad = episode.toString().padStart(2, '0');
        const titleMatch = baseName.split(dashNum[0])[0]?.replace(/^\[.*?\]\s*/, '').trim();
        const seriesTitle = titleMatch ? titleMatch.replace(/[._]/g, ' ') : undefined;
        return { isEpisode: true, season: 1, episode, displayBadge: `EP ${ePad}`, seriesTitle };
    }

    return { isEpisode: false, season: null, episode: null };
}

function naturalSortFiles(files) {
    return [...files].sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' })
    );
}

function getNextEpisode(currentFile, folderFiles) {
    const currentInfo = parseEpisodeInfo(currentFile.name);
    if (!currentInfo.isEpisode || currentInfo.episode === null) return null;

    const currentSeason = currentInfo.season ?? 1;
    const targetEpisode = currentInfo.episode + 1;

    const videoFiles = naturalSortFiles(folderFiles.filter(f => f.type !== 'folder' && isVideoFile(f.name)));

    const exactNext = videoFiles.find(f => {
        const info = parseEpisodeInfo(f.name);
        return info.isEpisode && (info.season ?? 1) === currentSeason && info.episode === targetEpisode;
    });

    if (exactNext) return exactNext;

    const currentIndex = videoFiles.findIndex(f => f.id === currentFile.id);
    if (currentIndex >= 0 && currentIndex + 1 < videoFiles.length) {
        const candidate = videoFiles[currentIndex + 1];
        const candInfo = parseEpisodeInfo(candidate.name);
        if (candInfo.isEpisode) {
            return candidate;
        }
    }

    return null;
}

test('parseEpisodeInfo parses standard S01E05 patterns', () => {
    const res = parseEpisodeInfo('Breaking.Bad.S01E05.1080p.mkv');
    assert.equal(res.isEpisode, true);
    assert.equal(res.season, 1);
    assert.equal(res.episode, 5);
    assert.equal(res.displayBadge, 'S01E05');
    assert.equal(res.seriesTitle, 'Breaking Bad');
});

test('parseEpisodeInfo parses Season 2 Episode 10 patterns', () => {
    const res = parseEpisodeInfo('Game of Thrones Season 2 Episode 10.mp4');
    assert.equal(res.isEpisode, true);
    assert.equal(res.season, 2);
    assert.equal(res.episode, 10);
    assert.equal(res.displayBadge, 'S02E10');
});

test('parseEpisodeInfo parses anime dash format', () => {
    const res = parseEpisodeInfo('[SubsPlease] Frieren - 08 [1080p].mkv');
    assert.equal(res.isEpisode, true);
    assert.equal(res.season, 1);
    assert.equal(res.episode, 8);
    assert.equal(res.displayBadge, 'EP 08');
});

test('naturalSortFiles sorts numerically rather than lexicographically', () => {
    const files = [
        { name: 'Show.S01E10.mkv', id: 10 },
        { name: 'Show.S01E01.mkv', id: 1 },
        { name: 'Show.S01E02.mkv', id: 2 },
        { name: 'Show.S01E11.mkv', id: 11 },
        { name: 'Show.S01E09.mkv', id: 9 },
    ];
    const sorted = naturalSortFiles(files);
    assert.deepEqual(sorted.map(f => f.id), [1, 2, 9, 10, 11]);
});

test('getNextEpisode resolves the next sequential episode', () => {
    const files = [
        { name: 'Severance.S01E01.mkv', id: 101, type: 'file' },
        { name: 'Severance.S01E02.mkv', id: 102, type: 'file' },
        { name: 'Severance.S01E03.mkv', id: 103, type: 'file' },
    ];
    const next = getNextEpisode(files[0], files);
    assert.notEqual(next, null);
    assert.equal(next.id, 102);

    const next2 = getNextEpisode(files[1], files);
    assert.equal(next2.id, 103);

    const next3 = getNextEpisode(files[2], files);
    assert.equal(next3, null);
});
