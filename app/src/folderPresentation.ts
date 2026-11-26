export type FolderKind = 'series' | 'movies' | 'general';
export type ViewMode = 'grid' | 'list' | 'posters';

export interface FolderPresentation {
    kind: FolderKind;
    viewMode?: ViewMode;
}

export type FolderPresentationMap = Record<string, FolderPresentation>;

function isFolderPresentationEntry(key: string, entry: unknown): entry is FolderPresentation {
    if (key !== 'home' && (!Number.isSafeInteger(Number(key)) || String(Number(key)) !== key)) return false;
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return false;
    const presentation = entry as Record<string, unknown>;
    return Object.keys(presentation).every(field => field === 'kind' || field === 'viewMode')
        && (presentation.kind === 'series' || presentation.kind === 'movies' || presentation.kind === 'general')
        && (presentation.viewMode === undefined || presentation.viewMode === 'grid' || presentation.viewMode === 'list' || presentation.viewMode === 'posters');
}

export function isFolderPresentationMap(value: unknown): value is FolderPresentationMap {
    return !!value && typeof value === 'object' && !Array.isArray(value)
        && Object.entries(value).every(([key, entry]) => isFolderPresentationEntry(key, entry));
}

export function sanitizeFolderPresentationMap(value: unknown): FolderPresentationMap {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    return Object.fromEntries(Object.entries(value).filter(([key, entry]) => isFolderPresentationEntry(key, entry))) as FolderPresentationMap;
}

export function folderPresentationKey(folderId: number | null): string {
    return folderId === null ? 'home' : String(folderId);
}

export function getFolderPresentation(presentations: FolderPresentationMap, folderId: number | null): FolderPresentation {
    return presentations[folderPresentationKey(folderId)] ?? { kind: 'general' };
}

export function getEffectiveViewMode(presentation: FolderPresentation, globalViewMode: ViewMode): ViewMode {
    return presentation.viewMode ?? (presentation.kind === 'series' ? 'list' : presentation.kind === 'movies' ? 'posters' : globalViewMode);
}
