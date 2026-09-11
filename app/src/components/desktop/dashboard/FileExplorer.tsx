import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import { Plus, ArrowUpDown, ArrowUp, ArrowDown, ZoomIn, ZoomOut, Tv, Play, Sparkles, Subtitles, Star, Tag } from '../../shared/icons.tsx';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import { useSettings } from '../../../context/SettingsContext';
import { FileCard } from './FileCard';
import { EmptyState } from './EmptyState';
import { TelegramFile, TelegramFolder } from '../../../types';
import { ContextMenu } from './ContextMenu';
import { FileListItem } from './FileListItem';
import { AttachSubtitlesModal } from './AttachSubtitlesModal';
import { ManageSubtitlesModal } from './ManageSubtitlesModal';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { analyzeSeriesFolder, parseEpisodeInfo, getNextEpisode } from '../../../utils/seriesParser';
import { WatchHistoryEntry } from '../../../utils/watchHistory';
import { formatBytes } from '../../../utils';
import { VideoSubtitleInfo } from '../../../types';

type SortField = 'name' | 'size' | 'date';
type SortDirection = 'asc' | 'desc';

interface FileExplorerProps {
    files: TelegramFile[];
    loading: boolean;
    error: Error | null;
    viewMode: 'grid' | 'list' | 'posters';
    selectedIds: number[];
    activeFolderId: number | null;
    onFileClick: (e: React.MouseEvent, id: number) => void;
    onDelete: (id: number) => void;
    onDownload: (id: number, name: string) => void;
    onPreview: (file: TelegramFile, orderedFiles?: TelegramFile[]) => void;
    onManualUpload: () => void;
    onToggleSelection: (id: number) => void;
    onDrop?: (e: React.DragEvent, folderId: number) => void;
    onDragStart?: (fileIds: number[]) => void;
    onDragEnd?: () => void;
    onShare?: (file: TelegramFile) => void;
    onRename?: (file: TelegramFile) => void;
    onFileMove?: (file: TelegramFile) => void;
    folders?: TelegramFolder[];
    cardScale: number;
    onCardScaleChange: (scale: number) => void;
    searchTerm: string;
    onClearSearch: () => void;
    onRetry: () => void;
    watchHistory?: WatchHistoryEntry[];
    /** Message ids favorited in the active folder (or all ids when showAllFavorites). */
    favoriteIds?: Set<number>;
    onToggleFavorite?: (file: TelegramFile) => void;
    /** Hide non-favorite files (chip filter). */
    favoritesOnly?: boolean;
    onFavoritesOnlyChange?: (value: boolean) => void;
    /** When showing the virtual All Favorites view, skip upload slot / season UI. */
    isAllFavoritesView?: boolean;
    tagMap?: Record<number, string[]>;
    onEditTags?: (file: TelegramFile) => void;
    activeTagFilter?: string | null;
    onActiveTagFilterChange?: (tag: string | null) => void;
    availableTags?: string[];
}


function useGridColumns(containerRef: React.RefObject<HTMLDivElement | null>) {
    const [columns, setColumns] = useState(4);
    const [containerWidth, setContainerWidth] = useState(800);

    useEffect(() => {
        if (!containerRef.current) return;

        const updateColumns = () => {
            const el = containerRef.current;
            if (!el) return;
            // clientWidth includes padding — subtract it so card size
            // calculations match the actual grid content area.
            const cs = getComputedStyle(el);
            const padX = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
            const width = el.clientWidth - padX;
            setContainerWidth(width > 0 ? width : 800);
            if (width < 640) setColumns(2);
            else if (width < 768) setColumns(3);
            else if (width < 1024) setColumns(4);
            else if (width < 1280) setColumns(5);
            else setColumns(6);
        };

        updateColumns();
        const observer = new ResizeObserver(updateColumns);
        observer.observe(containerRef.current);
        return () => observer.disconnect();
    }, [containerRef]);

    return { columns, containerWidth };
}

export function FileExplorer({
    files, loading, error, viewMode, selectedIds, activeFolderId,
    onFileClick, onDelete, onDownload, onPreview, onManualUpload, onToggleSelection, onDrop, onDragStart, onDragEnd, onShare, onRename, onFileMove,
    folders, cardScale, onCardScaleChange, searchTerm, onClearSearch, onRetry, watchHistory,
    favoriteIds, onToggleFavorite, favoritesOnly = false, onFavoritesOnlyChange, isAllFavoritesView = false,
    tagMap = {}, onEditTags, activeTagFilter = null, onActiveTagFilterChange, availableTags = []
}: FileExplorerProps) {
    const [sortField, setSortField] = useState<SortField>('name');
    const [sortDirection, setSortDirection] = useState<SortDirection>('asc');
    const [contextMenu, setContextMenu] = useState<{ x: number; y: number; file: TelegramFile; subtitles?: VideoSubtitleInfo[] } | null>(null);
    const [subtitlesTarget, setSubtitlesTarget] = useState<{ file: TelegramFile; subtitles: VideoSubtitleInfo[] } | null>(null);
    const { t } = useTranslation();
    const { settings } = useSettings();

    const parentRef = useRef<HTMLDivElement>(null);
    const { columns: baseColumns, containerWidth } = useGridColumns(parentRef);

    // Scale columns by cardScale: higher scale = fewer columns = larger cards
    const columns = Math.max(1, Math.round(baseColumns / cardScale));

    const GAP = 6;
    const cardWidth = (containerWidth - (GAP * (columns - 1))) / columns;
    // Posters use a 2:3 movie-poster ratio; grid cards use 4:3.
    const cardHeight = cardWidth * (viewMode === 'posters' ? 1.5 : 0.75);

    const handleContextMenu = useCallback(async (e: React.MouseEvent, file: TelegramFile) => {
        e.preventDefault();
        e.stopPropagation();
        let subtitles: VideoSubtitleInfo[] | undefined;
        if (file.type !== 'folder') {
            try {
                subtitles = await invoke<VideoSubtitleInfo[]>('cmd_get_video_subtitles', {
                    folderId: file.folder_id ?? null,
                    videoMessageId: file.id,
                });
            } catch (err) {
                console.error(err);
            }
        }
        setContextMenu({ x: e.clientX, y: e.clientY, file, subtitles });
    }, []);

    const [selectedSeasonKey, setSelectedSeasonKey] = useState<string>('all');
    const [isAttachSubtitlesOpen, setIsAttachSubtitlesOpen] = useState(false);

    // Reset season selection when folder changes
    useEffect(() => {
        setSelectedSeasonKey('all');
    }, [activeFolderId]);

    // Restore per-folder sort prefs (SQLite) when the active folder changes
    useEffect(() => {
        let cancelled = false;
        invoke<{ sort_field: string; sort_direction: string }>('cmd_get_folder_view_prefs', {
            folderId: activeFolderId,
        })
            .then((prefs) => {
                if (cancelled) return;
                const field = (['name', 'size', 'date'] as const).includes(
                    prefs.sort_field as SortField,
                )
                    ? (prefs.sort_field as SortField)
                    : 'name';
                const direction = prefs.sort_direction === 'desc' ? 'desc' : 'asc';
                setSortField(field);
                setSortDirection(direction);
            })
            .catch((err) => {
                console.error('Failed to load sort prefs', err);
            });
        return () => {
            cancelled = true;
        };
    }, [activeFolderId]);

    // Filter out subtitle sidecars from the general file list to maintain a 100% clean view
    const cleanFiles = useMemo(() => {
        const base = files.filter(f => !f.name.startsWith('#telestash_sub') && !f.name.includes('#telestash_sub'));
        let out = base;
        if (favoritesOnly && favoriteIds) out = out.filter(f => favoriteIds.has(f.id));
        if (activeTagFilter) out = out.filter(f => (tagMap[f.id] ?? []).includes(activeTagFilter));
        return out;
    }, [files, favoritesOnly, favoriteIds, activeTagFilter, tagMap]);

    const seriesAnalysis = useMemo(() => {
        return analyzeSeriesFolder(cleanFiles);
    }, [cleanFiles]);

    // Find latest watch history entry that belongs to this active folder or this series
    const folderWatchProgress = useMemo(() => {
        if (!watchHistory || watchHistory.length === 0 || cleanFiles.length === 0) return null;

        const fileIdsInFolder = new Set(cleanFiles.map(f => f.id));
        const seriesTitle = seriesAnalysis.seriesTitle?.toLowerCase().trim();

        // Find the latest history entry matching this folder's files or series title
        const latestMatch = watchHistory.find(entry => {
            if (fileIdsInFolder.has(entry.file_id)) return true;
            if (activeFolderId !== null && entry.folder_id === activeFolderId) return true;
            if (seriesTitle) {
                const entryInfo = parseEpisodeInfo(entry.file_name);
                if (entryInfo.seriesTitle && entryInfo.seriesTitle.toLowerCase().trim() === seriesTitle) {
                    return true;
                }
            }
            return false;
        });

        if (!latestMatch) return null;

        // Resolve matching TelegramFile object from folder files if available
        const lastWatchedFile: TelegramFile = cleanFiles.find(f => f.id === latestMatch.file_id) || {
            id: latestMatch.file_id,
            name: latestMatch.file_name,
            size: latestMatch.file_size,
            sizeStr: formatBytes(latestMatch.file_size),
            folder_id: latestMatch.folder_id ?? undefined,
            type: 'file'
        };

        const nextEpisodeFile = getNextEpisode(lastWatchedFile, cleanFiles);
        const lastWatchedInfo = parseEpisodeInfo(latestMatch.file_name);
        const nextEpisodeInfo = nextEpisodeFile ? parseEpisodeInfo(nextEpisodeFile.name) : null;

        return {
            lastWatchedFile,
            lastWatchedEntry: latestMatch,
            lastWatchedInfo,
            nextEpisodeFile,
            nextEpisodeInfo,
        };
    }, [watchHistory, cleanFiles, activeFolderId, seriesAnalysis]);

    // Filter files if a specific season is selected
    const activeSeasonFiles = useMemo(() => {
        if (!seriesAnalysis.isSeriesFolder || selectedSeasonKey === 'all') {
            return cleanFiles;
        }
        const foundSeason = seriesAnalysis.seasons.find(s => s.seasonKey === selectedSeasonKey);
        if (!foundSeason) return cleanFiles;
        // Keep non-series folders visible, plus episodes in selected season
        const folderFiles = cleanFiles.filter(f => f.type === 'folder');
        return [...folderFiles, ...foundSeason.files];
    }, [cleanFiles, seriesAnalysis, selectedSeasonKey]);

    const sortedFiles = useMemo(() => {
        return [...activeSeasonFiles].sort((a, b) => {
            let comparison = 0;
            switch (sortField) {
                case 'name':
                    comparison = a.name.localeCompare(b.name, settings.language, { numeric: true, sensitivity: 'base' });
                    break;
                case 'size':
                    comparison = (a.size || 0) - (b.size || 0);
                    break;
                case 'date':
                    comparison = (a.created_at || '').localeCompare(b.created_at || '');
                    break;
            }
            return sortDirection === 'asc' ? comparison : -comparison;
        });
    }, [activeSeasonFiles, sortField, sortDirection, settings.language]);

    const handleBingePlay = useCallback(() => {
        const targetSeason = seriesAnalysis.seasons.find(s => s.seasonKey === selectedSeasonKey) || seriesAnalysis.seasons[0];
        const episodesToPlay = targetSeason && targetSeason.files.length > 0 ? targetSeason.files : seriesAnalysis.allVideoFiles;
        if (episodesToPlay.length > 0) {
            const firstEp = episodesToPlay[0];
            toast.success(`Starting Binge Mode: ${firstEp.name}`);
            onPreview(firstEp, episodesToPlay);
        }
    }, [seriesAnalysis, selectedSeasonKey, onPreview]);

    const handlePreviewRequest = useCallback((file: TelegramFile) => {
        onPreview(file, sortedFiles);
    }, [onPreview, sortedFiles]);


    // Upload entry leads the grid so it stays visible above the fold — but not
    // during search, where results span folders and upload targets the active one.
    const isSearchActive = searchTerm.trim().length > 2;
    const showUploadSlot = !isSearchActive && !isAllFavoritesView && !favoritesOnly;

    const gridRows = useMemo(() => {
        const rows: (TelegramFile | 'upload')[][] = [];
        const itemsWithUpload: (TelegramFile | 'upload')[] = showUploadSlot ? ['upload', ...sortedFiles] : sortedFiles;
        for (let i = 0; i < itemsWithUpload.length; i += columns) {
            rows.push(itemsWithUpload.slice(i, i + columns));
        }
        return rows;
    }, [sortedFiles, columns, showUploadSlot]);


    const listItems = useMemo(() => {
        return showUploadSlot ? (['upload' as const, ...sortedFiles]) : sortedFiles;
    }, [sortedFiles, showUploadSlot]);


    const gridVirtualizer = useVirtualizer({
        count: gridRows.length,
        getScrollElement: () => parentRef.current,
        estimateSize: useCallback(() => cardHeight, [cardHeight]),
        overscan: 2,
        gap: GAP,
    });

    const listVirtualizer = useVirtualizer({
        count: listItems.length,
        getScrollElement: () => parentRef.current,
        estimateSize: () => 48,
        overscan: 5,
    });

    useEffect(() => {
        if (parentRef.current) {
            parentRef.current.scrollTop = 0;
        }
        gridVirtualizer.scrollToOffset(0);
        listVirtualizer.scrollToOffset(0);
    }, [activeFolderId, gridVirtualizer, listVirtualizer]);

    // Remeasure the grid virtualizer when columns or cardHeight changes to prevent overlapping
    useEffect(() => {
        gridVirtualizer.measure();
    }, [columns, cardHeight, gridVirtualizer]);

    const handleSort = (field: SortField) => {
        let nextField: SortField = field;
        let nextDirection: SortDirection = 'asc';
        if (sortField === field) {
            nextDirection = sortDirection === 'asc' ? 'desc' : 'asc';
            setSortDirection(nextDirection);
        } else {
            setSortField(field);
            setSortDirection('asc');
        }
        void invoke('cmd_set_folder_view_prefs', {
            folderId: activeFolderId,
            sortField: nextField,
            sortDirection: nextDirection,
        }).catch((err) => console.error('Failed to persist sort prefs', err));
    };

    const SortIcon = ({ field }: { field: SortField }) => {
        if (sortField !== field) return <ArrowUpDown className="w-3 h-3 opacity-30" />;
        return sortDirection === 'asc'
            ? <ArrowUp className="w-3 h-3 text-stash-primary" />
            : <ArrowDown className="w-3 h-3 text-stash-primary" />;
    };

    if (loading) {
        return (
            <div className="flex-1 p-6 flex justify-center items-center text-stash-subtext flex-col gap-4">
                <div className="w-8 h-8 border-4 border-stash-primary border-t-transparent rounded-full animate-spin"></div>
                Loading your files...
            </div>
        )
    }

    if (error) {
        return (
            <div className="flex-1 p-6 flex flex-col justify-center items-center gap-3 text-center">
                <p className="text-red-400 font-medium">{t('files.load_failed')}</p>
                <p className="max-w-md text-sm text-stash-subtext break-words">{error.message}</p>
                <button onClick={onRetry} className="px-3 py-1.5 rounded-md bg-stash-primary text-black text-sm font-medium hover:bg-stash-primary/90 transition-colors">
                    {t('files.retry')}
                </button>
            </div>
        );
    }

    if (files.length === 0) {
        return (
            <div className="flex-1 p-6 overflow-auto">
                <EmptyState onUpload={onManualUpload} searchTerm={searchTerm} onClearSearch={onClearSearch} />
            </div>
        );
    }

    return (
        <div
            ref={parentRef}
            className="vault-content-canvas"
        >
            {/* Series & Season Auto-Grouping Bar */}
            {seriesAnalysis.isSeriesFolder ? (
                <div className="mb-4 rounded-xl bg-slate-900/90 border border-slate-800 shadow-sm overflow-hidden divide-y divide-slate-800/80">
                    {/* Top Row: Series Title + Season Tabs + Binge Series Button */}
                    <div className="p-3 flex items-center justify-between gap-3">
                        <div className="flex flex-wrap items-center gap-1.5 flex-1 min-w-0">
                            <div className="flex items-center gap-1.5 text-xs font-mono font-bold text-cyan-400 mr-1.5 flex-shrink-0">
                                <Tv className="w-4 h-4" />
                                <span className="uppercase">{seriesAnalysis.seriesTitle || 'SERIES'}</span>
                            </div>
                            {seriesAnalysis.seasons.map((season) => {
                                const isActive = selectedSeasonKey === season.seasonKey;
                                return (
                                    <button
                                        key={season.seasonKey}
                                        onClick={() => setSelectedSeasonKey(season.seasonKey)}
                                        className={`px-2.5 py-1 rounded-lg text-xs font-medium font-mono transition-all whitespace-nowrap ${
                                            isActive
                                                ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 shadow-sm'
                                                : 'bg-slate-800/60 hover:bg-slate-800 text-gray-400 hover:text-gray-200 border border-transparent'
                                        }`}
                                    >
                                        {season.label}
                                    </button>
                                );
                            })}
                        </div>

                        <div className="flex items-center gap-2 flex-shrink-0 self-start sm:self-center">
                            <button
                                onClick={() => setIsAttachSubtitlesOpen(true)}
                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-500/20 hover:bg-indigo-500/30 text-indigo-300 border border-indigo-500/40 text-xs font-bold font-mono transition-all shadow-sm active:scale-95 flex-shrink-0"
                                title="Attach external subtitles (.sub/.idx, .srt, .ass, .vtt) to this series"
                            >
                                <Subtitles className="w-3.5 h-3.5" />
                                <span>Attach Subtitles</span>
                            </button>

                            <button
                                onClick={handleBingePlay}
                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold font-mono transition-all shadow-md hover:shadow-cyan-500/20 active:scale-95 flex-shrink-0"
                                title="Play all episodes in continuous MPV playlist"
                            >
                                <Play className="w-3.5 h-3.5 fill-current" />
                                <span>Binge Series</span>
                            </button>
                        </div>
                    </div>

                    {/* Bottom Row: Dynamic In-Folder Series Progress Tracker (The Office / Silo / etc.) */}
                    {folderWatchProgress && (
                        <div className="px-3.5 py-2.5 bg-slate-950/40 flex flex-wrap items-center justify-between gap-3">
                            <div className="flex items-center gap-2 min-w-0">
                                <span className="text-[9.5px] font-mono font-bold px-2 py-0.5 rounded bg-slate-800 border border-slate-700/80 text-slate-300 tracking-wide flex-shrink-0">
                                    TRACKING PROGRESS
                                </span>
                                <div className="text-xs text-slate-300 truncate">
                                    <span className="text-slate-400">Last watched: </span>
                                    <span className="font-semibold text-white">
                                        {folderWatchProgress.lastWatchedInfo.displayBadge ? `${folderWatchProgress.lastWatchedInfo.displayBadge} · ` : ''}
                                        {folderWatchProgress.lastWatchedEntry.file_name}
                                    </span>
                                </div>
                            </div>

                            <div className="flex items-center gap-2 flex-shrink-0">
                                <button
                                    onClick={() => onPreview(folderWatchProgress.lastWatchedFile, seriesAnalysis.allVideoFiles)}
                                    className="flex items-center gap-1 px-2.5 py-1 rounded-md bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono font-medium border border-slate-700 transition-colors"
                                    title={`Resume ${folderWatchProgress.lastWatchedEntry.file_name}`}
                                >
                                    <Play className="w-3 h-3 fill-slate-300" />
                                    <span>Resume {folderWatchProgress.lastWatchedInfo.displayBadge || 'Episode'}</span>
                                </button>

                                {folderWatchProgress.nextEpisodeFile && (
                                    <button
                                        onClick={() => onPreview(folderWatchProgress.nextEpisodeFile!, seriesAnalysis.allVideoFiles)}
                                        className="flex items-center gap-1 px-2.5 py-1 rounded-md bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 text-xs font-mono font-bold border border-cyan-500/40 transition-colors shadow-sm"
                                        title={`Play Next: ${folderWatchProgress.nextEpisodeFile.name}`}
                                    >
                                        <Sparkles className="w-3 h-3 fill-cyan-400" />
                                        <span>Next Up ({folderWatchProgress.nextEpisodeInfo?.displayBadge || 'Next'})</span>
                                    </button>
                                )}
                            </div>
                        </div>
                    )}
                </div>
            ) : folderWatchProgress ? (
                <div className="mb-4 px-3.5 py-2.5 rounded-xl bg-slate-900/90 border border-slate-800 shadow-sm flex flex-wrap items-center justify-between gap-3">
                    <div className="flex items-center gap-2 min-w-0">
                        <span className="text-[9.5px] font-mono font-bold px-2 py-0.5 rounded bg-slate-800 border border-slate-700/80 text-slate-300 tracking-wide flex-shrink-0">
                            LAST WATCHED
                        </span>
                        <div className="text-xs text-slate-300 truncate">
                            <span className="font-semibold text-white">
                                {folderWatchProgress.lastWatchedEntry.file_name}
                            </span>
                        </div>
                    </div>

                    <button
                        onClick={() => onPreview(folderWatchProgress.lastWatchedFile, files)}
                        className="flex items-center gap-1 px-2.5 py-1 rounded-md bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-mono font-medium border border-slate-700 transition-colors"
                    >
                        <Play className="w-3 h-3 fill-slate-300" />
                        <span>Resume</span>
                    </button>
                </div>
            ) : null}

            {viewMode === 'grid' || viewMode === 'posters' ? (
                <>

                    <div className="vault-control-row">
                        <span>Sort by:</span>
                        <button
                            onClick={() => handleSort('name')}
                            className={`px-2 py-1 rounded flex items-center gap-1 hover:bg-white/5 ${sortField === 'name' ? 'text-stash-primary' : ''}`}
                        >
                            Name <SortIcon field="name" />
                        </button>
                        <button
                            onClick={() => handleSort('size')}
                            className={`px-2 py-1 rounded flex items-center gap-1 hover:bg-white/5 ${sortField === 'size' ? 'text-stash-primary' : ''}`}
                        >
                            Size <SortIcon field="size" />
                        </button>
                        <button
                            onClick={() => handleSort('date')}
                            className={`px-2 py-1 rounded flex items-center gap-1 hover:bg-white/5 ${sortField === 'date' ? 'text-stash-primary' : ''}`}
                        >
                            Date <SortIcon field="date" />
                        </button>
                        {onFavoritesOnlyChange && !isAllFavoritesView && (
                            <button
                                onClick={() => onFavoritesOnlyChange(!favoritesOnly)}
                                className={`px-2 py-1 rounded flex items-center gap-1 hover:bg-white/5 border ${favoritesOnly ? 'text-amber-400 border-amber-400/40 bg-amber-400/10' : 'border-transparent text-stash-subtext'}`}
                                title="Show only favorites"
                            >
                                <Star className="w-3 h-3" weight={favoritesOnly ? 'fill' : 'regular'} />
                                Favorites
                            </button>
                        )}
                        {onActiveTagFilterChange && availableTags.length > 0 && !isAllFavoritesView && (
                            availableTags.map((tag) => (
                                <button
                                    key={tag}
                                    onClick={() => onActiveTagFilterChange(activeTagFilter === tag ? null : tag)}
                                    className={`px-2 py-1 rounded flex items-center gap-1 hover:bg-white/5 border text-xs ${activeTagFilter === tag ? 'text-cyan-300 border-cyan-400/40 bg-cyan-400/10' : 'border-transparent text-stash-subtext'}`}
                                >
                                    <Tag className="w-3 h-3" />
                                    {tag}
                                </button>
                            ))
                        )}

                        {/* Zoom slider */}
                        <div className="ml-auto flex items-center gap-1.5">
                            <button
                                onClick={() => onCardScaleChange(Math.max(0.5, cardScale - 0.25))}
                                className="p-1 rounded hover:bg-white/10 text-stash-subtext hover:text-stash-text transition-colors"
                                title="Smaller cards"
                                disabled={cardScale <= 0.5}
                            >
                                <ZoomOut className="w-3.5 h-3.5" />
                            </button>
                            <input
                                type="range"
                                min="0.5"
                                max="2"
                                step="0.25"
                                value={cardScale}
                                onChange={(e) => onCardScaleChange(parseFloat(e.target.value))}
                                className="w-20 h-1 bg-stash-border rounded-full appearance-none cursor-pointer [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-stash-primary [&::-webkit-slider-thumb]:cursor-pointer [&::-webkit-slider-thumb]:transition-transform [&::-webkit-slider-thumb]:hover:scale-125"
                                title={`Card zoom: ${Math.round(cardScale * 100)}%`}
                            />
                            <button
                                onClick={() => onCardScaleChange(Math.min(2, cardScale + 0.25))}
                                className="p-1 rounded hover:bg-white/10 text-stash-subtext hover:text-stash-text transition-colors"
                                title="Larger cards"
                                disabled={cardScale >= 2}
                            >
                                <ZoomIn className="w-3.5 h-3.5" />
                            </button>
                            <span className="text-[10px] text-stash-subtext/60 w-10 text-right tabular-nums">{Math.round(cardScale * 100)}%</span>
                        </div>
                    </div>


                    <div
                        className="relative w-full"
                        style={{ height: `${gridVirtualizer.getTotalSize()}px` }}
                    >
                        {gridVirtualizer.getVirtualItems().map((virtualRow) => {
                            const row = gridRows[virtualRow.index];
                            return (
                                <div
                                    key={virtualRow.key}
                                    className="absolute top-0 left-0 w-full grid"
                                    style={{
                                        height: `${cardHeight}px`,
                                        transform: `translateY(${virtualRow.start}px)`,
                                        gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                                        gap: `${GAP}px`,
                                    }}
                                >
                                    {row.map((item) => {
                                        if (item === 'upload') {
                                            return (
                                                <button
                                                    key="upload"
                                                    onClick={(e) => { e.stopPropagation(); onManualUpload(); }}
                                                    className="border-2 border-dashed border-stash-border rounded-xl flex flex-col items-center justify-center text-stash-subtext hover:border-stash-primary hover:text-stash-primary transition-all group overflow-hidden"
                                                    style={{ height: `${cardHeight}px` }}
                                                >
                                                    <Plus className="w-8 h-8 mb-2 group-hover:scale-110 transition-transform" />
                                                    <span className="text-sm font-medium">{t('common.upload_file')}</span>
                                                </button>
                                            );
                                        }
                                        const file = item;
                                        return (
                                            <FileCard
                                                key={file.id}
                                                file={file}
                                                variant={viewMode === 'posters' ? 'poster' : 'grid'}
                                                isSelected={selectedIds.includes(file.id)}
                                                onClick={(e) => onFileClick(e, file.id)}
                                                onContextMenu={(e) => handleContextMenu(e, file)}
                                                onDelete={() => onDelete(file.id)}
                                                onDownload={() => onDownload(file.id, file.name)}
                                                onPreview={() => handlePreviewRequest(file)}
                                                onDrop={onDrop}
                                                onDragStart={onDragStart}
                                                onDragEnd={onDragEnd}
                                                activeFolderId={activeFolderId}
                                                height={cardHeight}
                                                onToggleSelection={() => onToggleSelection(file.id)}
                                                onShare={onShare ? () => onShare(file) : undefined}
                                                selectedIds={selectedIds}
                                                isFavorite={favoriteIds?.has(file.id) ?? false}
                                                onToggleFavorite={onToggleFavorite ? () => onToggleFavorite(file) : undefined}
                                                tags={tagMap[file.id] ?? []}
                                            />
                                        );
                                    })}
                                </div>
                            );
                        })}
                    </div>
                </>
            ) : (
                <div className="flex flex-col w-full">
                    {/* List Header */}
                    <div className="grid grid-cols-[2rem_2fr_6rem_8rem] gap-4 px-4 py-2 text-[11px] font-semibold text-stash-subtext border-b border-stash-border mb-2 select-none items-center">
                        <div className="text-center">#</div>
                        <button onClick={() => handleSort('name')} className="flex items-center gap-1 hover:text-stash-text transition-colors">
                            {t('common.name')} <SortIcon field="name" />
                        </button>
                        <button onClick={() => handleSort('size')} className="flex items-center gap-1 justify-end hover:text-stash-text transition-colors">
                            {t('common.size')} <SortIcon field="size" />
                        </button>
                        <button onClick={() => handleSort('date')} className="flex items-center gap-1 justify-end hover:text-stash-text transition-colors">
                            {t('common.date')} <SortIcon field="date" />
                        </button>
                    </div>

                    <div
                        className="relative w-full"
                        style={{ height: `${listVirtualizer.getTotalSize()}px` }}
                    >
                        {listVirtualizer.getVirtualItems().map((virtualItem) => {
                            const item = listItems[virtualItem.index];
                            if (item === 'upload') {
                                return (
                                    <div
                                        key="upload"
                                        className="absolute top-0 left-0 w-full"
                                        style={{ transform: `translateY(${virtualItem.start}px)` }}
                                    >
                                        <button
                                            onClick={(e) => { e.stopPropagation(); onManualUpload(); }}
                                            className="flex items-center gap-4 px-4 py-3 rounded-lg cursor-pointer border border-dashed border-stash-border text-stash-subtext hover:text-stash-text hover:bg-stash-hover w-full"
                                        >
                                            <div className="w-5 h-5 flex items-center justify-center"><Plus className="w-4 h-4" /></div>
                                            <span className="text-sm font-medium">{t('common.upload_file')}...</span>
                                        </button>
                                    </div>
                                );
                            }
                            const file = item;
                            return (
                                <div
                                    key={file.id}
                                    className="absolute top-0 left-0 w-full"
                                    style={{ transform: `translateY(${virtualItem.start}px)` }}
                                >
                                    <FileListItem
                                        file={file}
                                        selectedIds={selectedIds}
                                        onFileClick={onFileClick}
                                        handleContextMenu={handleContextMenu}
                                        onDragStart={onDragStart}
                                        onDragEnd={onDragEnd}
                                        onDrop={onDrop}
                                    />
                                </div>
                            );
                        })}
                    </div>
                </div>
            )}

            {contextMenu && (
                <ContextMenu
                    x={contextMenu.x}
                    y={contextMenu.y}
                    file={contextMenu.file}
                    onClose={() => setContextMenu(null)}
                    onDownload={() => {
                        onDownload(contextMenu.file.id, contextMenu.file.name);
                        setContextMenu(null);
                    }}
                    onDelete={() => {
                        onDelete(contextMenu.file.id);
                        setContextMenu(null);
                    }}
                    onPreview={() => {
                        if (contextMenu.file.type === 'folder') {
                            onFileClick({ preventDefault: () => { }, stopPropagation: () => { } } as React.MouseEvent, contextMenu.file.id);
                        } else {
                            handlePreviewRequest(contextMenu.file);
                        }
                        setContextMenu(null);
                    }}
                    onShare={onShare ? () => {
                        onShare(contextMenu.file);
                        setContextMenu(null);
                    } : undefined}
                    isFavorite={favoriteIds?.has(contextMenu.file.id) ?? false}
                    onToggleFavorite={onToggleFavorite ? () => onToggleFavorite(contextMenu.file) : undefined}
                    onEditTags={onEditTags ? () => onEditTags(contextMenu.file) : undefined}
                    onRename={onRename ? () => {
                        onRename(contextMenu.file);
                        setContextMenu(null);
                    } : undefined}
                    onMove={onFileMove ? () => {
                        onFileMove(contextMenu.file);
                        setContextMenu(null);
                    } : undefined}
                    folders={folders}
                    activeFolderId={activeFolderId}
                    subtitleCount={contextMenu.subtitles?.length ?? 0}
                    onRemoveSubtitles={contextMenu.subtitles?.length ? () => {
                        setSubtitlesTarget({ file: contextMenu.file, subtitles: contextMenu.subtitles ?? [] });
                        setContextMenu(null);
                    } : undefined}
                />
            )}

            {subtitlesTarget && (
                <ManageSubtitlesModal target={subtitlesTarget} onClose={() => setSubtitlesTarget(null)} />
            )}

            <AttachSubtitlesModal
                isOpen={isAttachSubtitlesOpen}
                onClose={() => setIsAttachSubtitlesOpen(false)}
                folderId={activeFolderId}
                files={cleanFiles}
            />
        </div>
    )
}
