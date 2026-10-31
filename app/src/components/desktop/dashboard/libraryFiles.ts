import { TelegramFile } from '../../../types';
import { naturalSortFiles, parseEpisodeInfo, SeasonGroup } from '../../../utils/seriesParser';
import { WatchHistoryEntry } from '../../../utils/watchHistory';

export function getSeriesSections(files: TelegramFile[]): SeasonGroup[] {
    const episodes = naturalSortFiles(files.filter(file => file.type !== 'folder' && parseEpisodeInfo(file.name).isEpisode));
    const seasons = new Map<number, TelegramFile[]>();
    for (const file of episodes) {
        const season = parseEpisodeInfo(file.name).season ?? 1;
        seasons.set(season, [...(seasons.get(season) ?? []), file]);
    }
    const episodeSet = new Set(episodes);
    const otherFiles = files.filter(file => !episodeSet.has(file));
    const sections: SeasonGroup[] = [{ seasonKey: 'all', seasonNumber: null, label: `All files (${files.length})`, files }];
    for (const [season, seasonFiles] of [...seasons].sort(([a], [b]) => a - b)) {
        sections.push({
            seasonKey: season === 0 ? 'specials' : `s${season}`,
            seasonNumber: season,
            label: `${season === 0 ? 'Specials' : `Season ${season}`} (${seasonFiles.length})`,
            files: seasonFiles,
        });
    }
    if (otherFiles.length) sections.push({ seasonKey: 'other', seasonNumber: null, label: `Other files (${otherFiles.length})`, files: otherFiles });
    return sections;
}

export function getFolderWatchProgress(entries: WatchHistoryEntry[], files: TelegramFile[], folderId: number | null) {
    const sameFolder = files.filter(file => file.type !== 'folder' && (file.folder_id ?? null) === folderId);
    const byId = new Map(sameFolder.map(file => [file.id, file]));
    const lastWatchedEntry = [...entries]
        .filter(entry => entry.folder_id === folderId && byId.has(entry.file_id))
        .sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp))[0];
    if (!lastWatchedEntry) return null;

    const lastWatchedFile = byId.get(lastWatchedEntry.file_id)!;
    const orderedEpisodes = naturalSortFiles(sameFolder.filter(file => parseEpisodeInfo(file.name).isEpisode));
    const index = orderedEpisodes.indexOf(lastWatchedFile);
    const nextEpisodeFile = index < 0 ? null : orderedEpisodes[index + 1] ?? null;
    return {
        lastWatchedFile,
        lastWatchedEntry,
        lastWatchedInfo: parseEpisodeInfo(lastWatchedFile.name),
        nextEpisodeFile,
        nextEpisodeInfo: nextEpisodeFile ? parseEpisodeInfo(nextEpisodeFile.name) : null,
        orderedEpisodes,
    };
}
