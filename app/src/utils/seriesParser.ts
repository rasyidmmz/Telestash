import { TelegramFile } from '../types';
import { isVideoFile } from '../utils';

export interface EpisodeInfo {
    isEpisode: boolean;
    season: number | null;
    episode: number | null;
    displayBadge?: string;
    seriesTitle?: string;
}

export interface SeasonGroup {
    seasonKey: string; // 'all' | 's1' | 's2' | 'specials'
    seasonNumber: number | null;
    label: string;
    files: TelegramFile[];
}

export interface SeriesGroupResult {
    isSeriesFolder: boolean;
    seriesTitle: string;
    seasons: SeasonGroup[];
    allVideoFiles: TelegramFile[];
}

/**
 * Clean and normalize extracted series title.
 */
function cleanSeriesTitle(raw: string): string {
    return raw
        .replace(/[._]/g, ' ')
        .replace(/^\[.*?\]\s*/, '')
        .replace(/Superfan Episodes/i, '')
        .replace(/\s+/g, ' ')
        .trim();
}

/**
 * Parse season and episode information from video filename.
 * Supports: S01E05, S08e24, 9x01, Season 2 Episode 3, Season 8 - 24, EP04, Ep 12, Anime [01], etc.
 */
export function parseEpisodeInfo(filename: string): EpisodeInfo {
    if (!isVideoFile(filename)) {
        return { isEpisode: false, season: null, episode: null };
    }

    // Clean extension
    const baseName = filename.replace(/\.[^/.]+$/, '');

    // Pattern 1: Standard SxxExx, Season X Episode Y, S08e24, S8.E24, S08-E24, S08E24-E25
    const sxE = baseName.match(/(?:^|[.\s_\-[])(?:s|season)[.\s_-]*(\d+)[.\s_-]*(?:e|ep|episode)[.\s_-]*(\d+)(?:[.\s_-]*(?:e|ep|episode|-)[.\s_-]*(\d+))?/i);
    if (sxE && sxE[1] && sxE[2]) {
        const season = parseInt(sxE[1], 10);
        const episode = parseInt(sxE[2], 10);
        const sPad = season.toString().padStart(2, '0');
        const ePad = episode.toString().padStart(2, '0');
        
        // Multi-episode support (e.g. S08E24-E25)
        const multiEp = sxE[3] ? `-E${parseInt(sxE[3], 10).toString().padStart(2, '0')}` : '';

        // Extract potential series title before the season tag
        const titleMatch = baseName.split(sxE[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;

        return {
            isEpisode: true,
            season,
            episode,
            displayBadge: `S${sPad}E${ePad}${multiEp}`,
            seriesTitle
        };
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

        return {
            isEpisode: true,
            season,
            episode,
            displayBadge: `S${sPad}E${ePad}`,
            seriesTitle
        };
    }

    // Pattern 3: Season X - Y (e.g. "Season 8 - 24" or "S08 - 24" or "S8 24")
    const sDashE = baseName.match(/(?:^|[.\s_\-[])(?:s|season)[.\s_-]*(\d+)[.\s_–—-]+(?:ep|episode)?\s*(\d{1,3})(?:[.\s_\-–—\]]|$)/i);
    if (sDashE && sDashE[1] && sDashE[2]) {
        const season = parseInt(sDashE[1], 10);
        const episode = parseInt(sDashE[2], 10);
        const sPad = season.toString().padStart(2, '0');
        const ePad = episode.toString().padStart(2, '0');

        const titleMatch = baseName.split(sDashE[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;

        return {
            isEpisode: true,
            season,
            episode,
            displayBadge: `S${sPad}E${ePad}`,
            seriesTitle
        };
    }

    // Pattern 4: Episode Y only with or without separate Season elsewhere
    const epOnly = baseName.match(/(?:^|[.\s_\-[])(?:e|ep|episode)[.\s_-]*(\d+)(?:\b|v\d+|[.\s_\-\]])/i);
    if (epOnly && epOnly[1]) {
        const episode = parseInt(epOnly[1], 10);
        const ePad = episode.toString().padStart(2, '0');
        
        // Check if there is a Season elsewhere
        const seasonMatch = baseName.match(/(?:s|season)[.\s_-]*(\d+)/i);
        const season = seasonMatch && seasonMatch[1] ? parseInt(seasonMatch[1], 10) : 1;
        const sPad = season.toString().padStart(2, '0');

        const titleMatch = baseName.split(epOnly[0])[0]?.replace(/[._-]+$/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;

        return {
            isEpisode: true,
            season,
            episode,
            displayBadge: season > 1 ? `S${sPad}E${ePad}` : `EP ${ePad}`,
            seriesTitle
        };
    }

    // Pattern 5: Anime dash format: "Title - 05 [1080p]" or "[Group] Title - 05"
    const dashNum = baseName.match(/[-–—]\s*(\d{1,3})(?:\s*\[|\s*\(|\s*\.|\s*$)/);
    if (dashNum && dashNum[1]) {
        const episode = parseInt(dashNum[1], 10);
        const ePad = episode.toString().padStart(2, '0');
        const titleMatch = baseName.split(dashNum[0])[0]?.replace(/^\[.*?\]\s*/, '').trim();
        const seriesTitle = titleMatch ? cleanSeriesTitle(titleMatch) : undefined;

        return {
            isEpisode: true,
            season: 1,
            episode,
            displayBadge: `EP ${ePad}`,
            seriesTitle
        };
    }

    return { isEpisode: false, season: null, episode: null };
}

/**
 * Natural sort comparator for files ensuring Ep 1, Ep 2, ... Ep 9, Ep 10, Ep 11 order.
 */
export function naturalSortFiles(files: TelegramFile[]): TelegramFile[] {
    return [...files].sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' })
    );
}

/**
 * Groups files in a folder into Season tabs if multiple episode files are detected.
 */
export function analyzeSeriesFolder(files: TelegramFile[]): SeriesGroupResult {
    const videoFiles = naturalSortFiles(files.filter(f => f.type !== 'folder' && isVideoFile(f.name)));
    
    let episodeCount = 0;
    const seasonsMap = new Map<number, TelegramFile[]>();
    const specials: TelegramFile[] = [];
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

    // If at least 2 episode files exist or >= 40% of video files are episodes, treat as series
    const isSeriesFolder = episodeCount >= 2 && (episodeCount / Math.max(videoFiles.length, 1)) >= 0.4;

    const seasons: SeasonGroup[] = [];

    if (isSeriesFolder) {
        // "All" tab
        seasons.push({
            seasonKey: 'all',
            seasonNumber: null,
            label: `All Episodes (${videoFiles.length})`,
            files: videoFiles,
        });

        // Sorted season tabs
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

import { WatchHistoryEntry } from './watchHistory';

/**
 * Finds the next episode in a series based on the currently watched episode.
 * Supports advancing within the same season (S01E05 -> S01E06) and crossing over
 * to the next season (S01E10 -> S02E01).
 */
export function getNextEpisode(currentFile: TelegramFile, folderFiles: TelegramFile[]): TelegramFile | null {
    const currentInfo = parseEpisodeInfo(currentFile.name);
    if (!currentInfo.isEpisode || currentInfo.episode === null) return null;

    const currentSeason = currentInfo.season ?? 1;
    const nextEpisodeNum = currentInfo.episode + 1;
    const nextSeasonNum = currentSeason + 1;

    const videoFiles = naturalSortFiles(folderFiles.filter(f => f.type !== 'folder' && isVideoFile(f.name)));

    // 1. Look for next episode in current season (e.g. S01E05 -> S01E06)
    const exactNextInSeason = videoFiles.find(f => {
        const info = parseEpisodeInfo(f.name);
        return info.isEpisode && (info.season ?? 1) === currentSeason && info.episode === nextEpisodeNum;
    });
    if (exactNextInSeason) return exactNextInSeason;

    // 2. Look for first episode of next season (e.g. S01E10 -> S02E01)
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

/**
 * Deduplicate and consolidate recent watch history entries so that each TV series,
 * folder/channel, or standalone movie appears exactly once with its most recent playback progress.
 */
export function groupRecentWatchEntries(entries: WatchHistoryEntry[]): WatchHistoryEntry[] {
    if (!entries || entries.length === 0) return [];
    const seenGroups = new Set<string>();
    const consolidated: WatchHistoryEntry[] = [];

    for (const entry of entries) {
        const info = parseEpisodeInfo(entry.file_name);
        let groupKey: string;

        if (info.isEpisode) {
            // Group by series title if extracted, otherwise by folder_id + series pattern
            const titleKey = info.seriesTitle ? info.seriesTitle.toLowerCase().trim() : '';
            if (titleKey) {
                groupKey = `series-title:${titleKey}`;
            } else if (entry.folder_id !== null && entry.folder_id !== undefined) {
                groupKey = `series-folder:${entry.folder_id}`;
            } else {
                groupKey = `series-file:${entry.file_id}`;
            }
        } else {
            // Standalone video/movie
            groupKey = `movie:${entry.file_id}`;
        }

        if (!seenGroups.has(groupKey)) {
            seenGroups.add(groupKey);
            consolidated.push(entry);
        }
    }

    return consolidated;
}
