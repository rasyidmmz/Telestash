import test from 'node:test';
import assert from 'node:assert/strict';

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
    const nextEpisodeNum = currentInfo.episode + 1;
    const nextSeasonNum = currentSeason + 1;

    const videoFiles = naturalSortFiles(folderFiles.filter(f => f.type !== 'folder' && isVideoFile(f.name)));

    // 1. Look for next episode in current season
    const exactNextInSeason = videoFiles.find(f => {
        const info = parseEpisodeInfo(f.name);
        return info.isEpisode && (info.season ?? 1) === currentSeason && info.episode === nextEpisodeNum;
    });
    if (exactNextInSeason) return exactNextInSeason;

    // 2. Look for first episode of next season
    const nextSeasonFirstEp = videoFiles.find(f => {
        const info = parseEpisodeInfo(f.name);
        return info.isEpisode && (info.season ?? 1) === nextSeasonNum && (info.episode === 1 || info.episode === 0);
    });
    if (nextSeasonFirstEp) return nextSeasonFirstEp;

    // 3. Fallback: find next index in naturally sorted video files
    const currentIndex = videoFiles.findIndex(f => f.id === currentFile.id || f.name === currentFile.name);
    if (currentIndex >= 0 && currentIndex + 1 < videoFiles.length) {
        const candidate = videoFiles[currentIndex + 1];
        const candInfo = parseEpisodeInfo(candidate.name);
        if (candInfo.isEpisode) {
            return candidate;
        }
    }

    return null;
}

function groupRecentWatchEntries(entries) {
    if (!entries || entries.length === 0) return [];
    const seenGroups = new Set();
    const consolidated = [];

    for (const entry of entries) {
        const info = parseEpisodeInfo(entry.file_name);
        let groupKey;

        if (info.isEpisode) {
            const titleKey = info.seriesTitle ? info.seriesTitle.toLowerCase().trim() : '';
            if (titleKey) {
                groupKey = `series-title:${titleKey}`;
            } else if (entry.folder_id !== null && entry.folder_id !== undefined) {
                groupKey = `series-folder:${entry.folder_id}`;
            } else {
                groupKey = `series-file:${entry.file_id}`;
            }
        } else {
            groupKey = `movie:${entry.file_id}`;
        }

        if (!seenGroups.has(groupKey)) {
            seenGroups.add(groupKey);
            consolidated.push(entry);
        }
    }

    return consolidated;
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

test('getNextEpisode resolves the next sequential episode in same season', () => {
    const files = [
        { name: 'From.S02E05.mkv', id: 205, type: 'file' },
        { name: 'From.S02E06.mkv', id: 206, type: 'file' },
        { name: 'From.S02E07.mkv', id: 207, type: 'file' },
    ];
    const next = getNextEpisode(files[0], files);
    assert.notEqual(next, null);
    assert.equal(next.id, 206);
});

test('getNextEpisode resolves first episode of next season at season finale', () => {
    const files = [
        { name: 'From.S01E10.mkv', id: 110, type: 'file' },
        { name: 'From.S02E01.mkv', id: 201, type: 'file' },
    ];
    const next = getNextEpisode(files[0], files);
    assert.notEqual(next, null);
    assert.equal(next.id, 201);
});

test('groupRecentWatchEntries deduplicates multiple episodes of same series to only latest', () => {
    const history = [
        { file_id: 205, file_name: 'From.S02E05.1080p.mkv', timestamp: '2026-08-21T14:00:00Z', folder_id: 10 },
        { file_id: 204, file_name: 'From.S02E04.1080p.mkv', timestamp: '2026-08-21T13:00:00Z', folder_id: 10 },
        { file_id: 110, file_name: 'From.S01E10.1080p.mkv', timestamp: '2026-08-21T12:00:00Z', folder_id: 10 },
        { file_id: 999, file_name: 'Dune.Part.Two.2024.2160p.mkv', timestamp: '2026-08-21T11:00:00Z', folder_id: 20 },
        { file_id: 301, file_name: 'Severance.S01E01.1080p.mkv', timestamp: '2026-08-21T10:00:00Z', folder_id: 30 },
        { file_id: 201, file_name: 'From.S02E01.1080p.mkv', timestamp: '2026-08-21T09:00:00Z', folder_id: 10 },
    ];

    const grouped = groupRecentWatchEntries(history);
    assert.equal(grouped.length, 3);
    assert.equal(grouped[0].file_id, 205); // Latest From episode S02E05
    assert.equal(grouped[1].file_id, 999); // Dune Movie
    assert.equal(grouped[2].file_id, 301); // Severance S01E01
});
