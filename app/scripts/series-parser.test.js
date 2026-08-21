import test from 'node:test';
import assert from 'node:assert/strict';

function isVideoFile(name) {
    return /\.(mp4|mkv|avi|mov|webm|flv|ts|m4v)$/i.test(name);
}

function cleanSeriesTitle(raw) {
    return raw
        .replace(/[._]/g, ' ')
        .replace(/^\[.*?\]\s*/, '')
        .replace(/Superfan Episodes/i, '')
        .replace(/\s+/g, ' ')
        .trim();
}

function parseEpisodeInfo(filename) {
    if (!isVideoFile(filename)) {
        return { isEpisode: false, season: null, episode: null };
    }
    const baseName = filename.replace(/\.[^/.]+$/, '');

    // Pattern 1: Standard SxxExx, Season X Episode Y, S08e24, S8.E24, S08-E24, S08E24-E25
    const sxE = baseName.match(/(?:^|[.\s_\-[])(?:s|season)[.\s_-]*(\d+)[.\s_-]*(?:e|ep|episode)[.\s_-]*(\d+)(?:[.\s_-]*(?:e|ep|episode|-)[.\s_-]*(\d+))?/i);
    if (sxE && sxE[1] && sxE[2]) {
        const season = parseInt(sxE[1], 10);
        const episode = parseInt(sxE[2], 10);
        const sPad = season.toString().padStart(2, '0');
        const ePad = episode.toString().padStart(2, '0');
        const multiEp = sxE[3] ? `-E${parseInt(sxE[3], 10).toString().padStart(2, '0')}` : '';
        const titleMatch = baseName.split(sxE[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;
        return { isEpisode: true, season, episode, displayBadge: `S${sPad}E${ePad}${multiEp}`, seriesTitle };
    }

    // Pattern 2: Classic NxEE notation (e.g. "9x01", "09x01", "8x24", "The Office (US) - 9x01 - New Guys")
    const nxE = baseName.match(/(?:^|[.\s_\-[])(\d{1,2})x(\d{1,3})(?:[.\s_\-–—\]]|$)/i);
    if (nxE && nxE[1] && nxE[2]) {
        const season = parseInt(nxE[1], 10);
        const episode = parseInt(nxE[2], 10);
        const sPad = season.toString().padStart(2, '0');
        const ePad = episode.toString().padStart(2, '0');
        const titleMatch = baseName.split(nxE[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;
        return { isEpisode: true, season, episode, displayBadge: `S${sPad}E${ePad}`, seriesTitle };
    }

    // Pattern 3: Season X - Y
    const sDashE = baseName.match(/(?:^|[.\s_\-[])(?:s|season)[.\s_-]*(\d+)[.\s_–—-]+(?:ep|episode)?\s*(\d{1,3})(?:[.\s_\-–—\]]|$)/i);
    if (sDashE && sDashE[1] && sDashE[2]) {
        const season = parseInt(sDashE[1], 10);
        const episode = parseInt(sDashE[2], 10);
        const sPad = season.toString().padStart(2, '0');
        const ePad = episode.toString().padStart(2, '0');
        const titleMatch = baseName.split(sDashE[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;
        return { isEpisode: true, season, episode, displayBadge: `S${sPad}E${ePad}`, seriesTitle };
    }

    // Pattern 4: Episode Y only
    const epOnly = baseName.match(/(?:^|[.\s_\-[])(?:e|ep|episode)[.\s_-]*(\d+)(?:\b|v\d+|[.\s_\-\]])/i);
    if (epOnly && epOnly[1]) {
        const episode = parseInt(epOnly[1], 10);
        const ePad = episode.toString().padStart(2, '0');
        const seasonMatch = baseName.match(/(?:s|season)[.\s_-]*(\d+)/i);
        const season = seasonMatch && seasonMatch[1] ? parseInt(seasonMatch[1], 10) : 1;
        const sPad = season.toString().padStart(2, '0');
        const titleMatch = baseName.split(epOnly[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;
        return { isEpisode: true, season, episode, displayBadge: season > 1 ? `S${sPad}E${ePad}` : `EP ${ePad}`, seriesTitle };
    }

    // Pattern 5: Anime dash format
    const dashNum = baseName.match(/[-–—]\s*(\d{1,3})(?:\s*\[|\s*\(|\s*\.|\s*$)/);
    if (dashNum && dashNum[1]) {
        const episode = parseInt(dashNum[1], 10);
        const ePad = episode.toString().padStart(2, '0');
        const titleMatch = baseName.split(dashNum[0])[0]?.replace(/^\[.*?\]\s*/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;
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

    const exactNextInSeason = videoFiles.find(f => {
        const info = parseEpisodeInfo(f.name);
        return info.isEpisode && (info.season ?? 1) === currentSeason && info.episode === nextEpisodeNum;
    });
    if (exactNextInSeason) return exactNextInSeason;

    const nextSeasonFirstEp = videoFiles.find(f => {
        const info = parseEpisodeInfo(f.name);
        return info.isEpisode && (info.season ?? 1) === nextSeasonNum && (info.episode === 1 || info.episode === 0);
    });
    if (nextSeasonFirstEp) return nextSeasonFirstEp;

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

function analyzeSeriesFolder(files) {
    const videoFiles = naturalSortFiles(files.filter(f => f.type !== 'folder' && isVideoFile(f.name)));
    let episodeCount = 0;
    const seasonsMap = new Map();
    const specials = [];
    let detectedTitle = '';

    for (const file of videoFiles) {
        const info = parseEpisodeInfo(file.name);
        if (info.isEpisode) {
            episodeCount++;
            if (!detectedTitle && info.seriesTitle) {
                detectedTitle = info.seriesTitle;
            }
            if (info.season !== null && info.season > 0) {
                const list = seasonsMap.get(info.season) || [];
                list.push(file);
                seasonsMap.set(info.season, list);
            } else {
                specials.push(file);
            }
        }
    }

    const isSeriesFolder = episodeCount >= 2 && (episodeCount / Math.max(videoFiles.length, 1)) >= 0.4;
    const seasons = [];

    if (isSeriesFolder) {
        seasons.push({
            seasonKey: 'all',
            seasonNumber: null,
            label: `All Episodes (${videoFiles.length})`,
            files: videoFiles,
        });

        const sortedSeasonNums = Array.from(seasonsMap.keys()).sort((a, b) => a - b);
        for (const sNum of sortedSeasonNums) {
            const sFiles = seasonsMap.get(sNum) || [];
            seasons.push({
                seasonKey: `s${sNum}`,
                seasonNumber: sNum,
                label: `Season ${sNum} (${sFiles.length})`,
                files: sFiles,
            });
        }

        if (specials.length > 0) {
            seasons.push({
                seasonKey: 'specials',
                seasonNumber: 0,
                label: `Specials (${specials.length})`,
                files: specials,
            });
        }
    }

    return {
        isSeriesFolder,
        seriesTitle: detectedTitle,
        seasons,
        allVideoFiles: videoFiles,
    };
}

test('parseEpisodeInfo parses standard S01E05 patterns', () => {
    const res = parseEpisodeInfo('Breaking.Bad.S01E05.1080p.mkv');
    assert.equal(res.isEpisode, true);
    assert.equal(res.season, 1);
    assert.equal(res.episode, 5);
    assert.equal(res.displayBadge, 'S01E05');
    assert.equal(res.seriesTitle, 'Breaking Bad');
});

test('parseEpisodeInfo parses The Office Season 9 9x01 format', () => {
    const res = parseEpisodeInfo('The Office (US) - 9x01 - New Guys.mp4');
    assert.equal(res.isEpisode, true);
    assert.equal(res.season, 9);
    assert.equal(res.episode, 1);
    assert.equal(res.displayBadge, 'S09E01');
    assert.equal(res.seriesTitle, 'The Office (US)');
});

test('parseEpisodeInfo parses The Office Superfan Season 8 S08e24 format', () => {
    const res = parseEpisodeInfo('The_Office_Superfan_Episodes_S08e24_Free_Family_Portrait_Studio.mp4');
    assert.equal(res.isEpisode, true);
    assert.equal(res.season, 8);
    assert.equal(res.episode, 24);
    assert.equal(res.displayBadge, 'S08E24');
    assert.equal(res.seriesTitle, 'The Office');
});

test('parseEpisodeInfo parses The Office Superfan Season 7 S07e24 format', () => {
    const res = parseEpisodeInfo('The_Office_Superfan_Episodes_S07e24_Search_Committee_Extended_Cut.mp4');
    assert.equal(res.isEpisode, true);
    assert.equal(res.season, 7);
    assert.equal(res.episode, 24);
    assert.equal(res.displayBadge, 'S07E24');
    assert.equal(res.seriesTitle, 'The Office');
});

test('analyzeSeriesFolder groups mixed naming of 9 seasons into Season 1 through Season 9 tabs', () => {
    const files = [
        { name: 'The_Office_Superfan_Episodes_S01e01_Pilot.mp4', id: 1, type: 'file' },
        { name: 'The_Office_Superfan_Episodes_S02e01_The_Dundies.mp4', id: 2, type: 'file' },
        { name: 'The_Office_Superfan_Episodes_S03e01_Gay_Witch_Hunt.mp4', id: 3, type: 'file' },
        { name: 'The_Office_Superfan_Episodes_S04e01_Fun_Run.mp4', id: 4, type: 'file' },
        { name: 'The_Office_Superfan_Episodes_S05e01_Weight_Loss.mp4', id: 5, type: 'file' },
        { name: 'The_Office_Superfan_Episodes_S06e01_Gossip.mp4', id: 6, type: 'file' },
        { name: 'The_Office_Superfan_Episodes_S07e24_Search_Committee.mp4', id: 7, type: 'file' },
        { name: 'The_Office_Superfan_Episodes_S08e24_Free_Family_Portrait_Studio.mp4', id: 8, type: 'file' },
        { name: 'The Office (US) - 9x01 - New Guys.mp4', id: 9, type: 'file' },
    ];

    const result = analyzeSeriesFolder(files);
    assert.equal(result.isSeriesFolder, true);
    // Seasons: All + Season 1..9 = 10 tabs
    assert.equal(result.seasons.length, 10);
    assert.equal(result.seasons[1].seasonNumber, 1);
    assert.equal(result.seasons[8].seasonNumber, 8);
    assert.equal(result.seasons[9].seasonNumber, 9);
});
