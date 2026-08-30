import { HardDrive, LayoutGrid, Sun, Moon, Settings, Share2, X, Globe, ScrollText, Film, PieChart } from 'lucide-react';
import { useTheme } from '../../../context/ThemeContext';
import { useTranslation } from 'react-i18next';
import { useErrorLogs } from '../../../errorLogs';

interface TopBarProps {
    currentFolderName: string;
    selectedIds: number[];
    onShowMoveModal: () => void;
    onBulkDownload: () => void;
    onBulkDelete: () => void;
    onBulkShare: () => void;
    onDownloadFolder: () => void;
    onClearSelection: () => void;
    viewMode: 'grid' | 'list';
    setViewMode: (mode: 'grid' | 'list') => void;
    searchTerm: string;
    onSearchChange: (term: string) => void;
    onSettingsClick: () => void;
    onLogsClick: () => void;
    onWatchLogsClick?: () => void;
    onAnalyticsClick?: () => void;
    onRemoteUploadClick: () => void;
}

export function TopBar({
    currentFolderName, selectedIds, onShowMoveModal, onBulkDownload, onBulkDelete, onBulkShare,
    onDownloadFolder, onClearSelection, viewMode, setViewMode, searchTerm, onSearchChange, onSettingsClick,
    onLogsClick, onWatchLogsClick, onAnalyticsClick, onRemoteUploadClick
}: TopBarProps) {
    const { theme, toggleTheme } = useTheme();
    const { t } = useTranslation();
    const errorLogs = useErrorLogs();
    return (
        <header className="h-14 border-b border-stash-border flex items-center justify-between px-3 sm:px-4 gap-2 bg-stash-surface/80 backdrop-blur-md sticky top-0 z-10 min-w-0" onClick={e => e.stopPropagation()}>
            <div className="flex items-center justify-start gap-2 shrink-0 min-w-0 max-w-[160px] sm:max-w-xs md:max-w-none">
                <div className="flex items-center text-sm breadcrumbs text-stash-subtext select-none font-mono min-w-0">
                    <span className="text-xs text-cyan-400 shrink-0">root</span>
                    <span className="mx-1 text-gray-600 shrink-0">/</span>
                    <span className="text-stash-text font-medium text-xs truncate" title={currentFolderName}>{currentFolderName}</span>
                </div>
            </div>

            <div className="flex-1 shrink min-w-[120px] max-w-xs sm:max-w-sm md:max-w-md mx-1 sm:mx-3">
                <input
                    type="text"
                    placeholder={t('common.search_placeholder')}
                    className="w-full bg-stash-hover border border-stash-border rounded-lg px-3 py-1.5 text-xs sm:text-sm text-stash-text placeholder:text-stash-subtext focus:outline-none focus:border-stash-primary/50 transition-colors"
                    value={searchTerm}
                    onChange={(e) => onSearchChange(e.target.value)}
                />
            </div>

            <div className="flex items-center justify-end gap-1 sm:gap-1.5 shrink-0 overflow-x-auto scrollbar-none">
                {selectedIds.length > 0 && (
                    <div className="flex items-center gap-1.5 mr-2 animate-in fade-in slide-in-from-top-2 shrink-0">
                        <span className="text-xs text-stash-subtext hidden lg:inline mr-1">{t('files.items_selected', { count: selectedIds.length })}</span>
                        <button onClick={onClearSelection} className="p-1.5 hover:bg-stash-hover rounded-md text-xs text-stash-subtext hover:text-stash-text transition flex items-center gap-1 shrink-0" title={t('files.clear_selection')}><X className="w-3.5 h-3.5" /></button>
                        <button onClick={onShowMoveModal} className="px-2.5 py-1 bg-stash-primary/20 hover:bg-stash-primary/30 text-stash-primary rounded-md text-xs transition font-medium shrink-0">{t('files.move_to')}</button>
                        <button onClick={onBulkDownload} className="px-2.5 py-1 bg-stash-hover hover:bg-stash-border rounded-md text-xs text-stash-text transition shrink-0">{t('files.download_selected')}</button>
                        <button onClick={onBulkShare} className="px-2.5 py-1 bg-stash-primary/20 hover:bg-stash-primary/30 text-stash-primary rounded-md text-xs transition font-medium flex items-center gap-1 shrink-0"><Share2 className="w-3 h-3" />{t('files.share')} ({selectedIds.length})</button>
                        <button onClick={onBulkDelete} className="px-2.5 py-1 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded-md text-xs transition shrink-0">{t('files.delete')}</button>
                    </div>
                )}

                <button onClick={onDownloadFolder} className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-stash-text transition group relative shrink-0" title={t('files.download_folder')}>
                    <HardDrive className="w-4 h-4 sm:w-5 sm:h-5" />
                    <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                        {t('files.download_all')}
                    </span>
                </button>

                <button onClick={onRemoteUploadClick} className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-stash-text transition group relative shrink-0" title={t('files.remote_upload')}>
                    <Globe className="w-4 h-4 sm:w-5 sm:h-5" />
                    <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                        {t('files.remote_upload_url')}
                    </span>
                </button>

                <button
                    onClick={() => setViewMode(viewMode === 'grid' ? 'list' : 'grid')}
                    className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-stash-text transition relative group shrink-0"
                    title={t('files.toggle_layout')}
                >
                    <LayoutGrid className="w-4 h-4 sm:w-5 sm:h-5" />
                    <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                        {viewMode === 'grid' ? t('files.switch_list') : t('files.switch_grid')}
                    </span>
                </button>

                <div className="w-px h-5 sm:h-6 bg-stash-border mx-0.5 sm:mx-1 shrink-0"></div>

                {onAnalyticsClick && (
                    <button
                        onClick={onAnalyticsClick}
                        className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-cyan-400 transition relative group shrink-0"
                        title="Storage Analytics"
                    >
                        <PieChart className="w-4 h-4 sm:w-5 sm:h-5" />
                        <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                            Storage Analytics
                        </span>
                    </button>
                )}

                {onWatchLogsClick && (
                    <button
                        onClick={onWatchLogsClick}
                        className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-cyan-400 transition relative group shrink-0"
                        title="Watch History Logs"
                    >
                        <Film className="w-4 h-4 sm:w-5 sm:h-5" />
                        <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                            Watch Logs
                        </span>
                    </button>
                )}

                <button
                    onClick={onLogsClick}
                    className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-stash-text transition relative group shrink-0"
                    title="Error Logs"
                >
                    <ScrollText className="w-4 h-4 sm:w-5 sm:h-5" />
                    {errorLogs.length > 0 && (
                        <span className="absolute top-1 right-1 w-2 h-2 rounded-full bg-red-500 ring-2 ring-stash-surface" />
                    )}
                    <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                        Error Logs
                    </span>
                </button>

                <button
                    onClick={onSettingsClick}
                    className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-stash-text transition relative group shrink-0"
                    title={t('common.settings')}
                >
                    <Settings className="w-4 h-4 sm:w-5 sm:h-5" />
                    <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                        {t('common.settings')}
                    </span>
                </button>

                <button
                    onClick={toggleTheme}
                    className="p-1.5 sm:p-2 hover:bg-stash-hover rounded-md text-stash-subtext hover:text-stash-text transition relative group shrink-0"
                    title={theme === 'dark' ? t('common.switch_light') : t('common.switch_dark')}
                >
                    {theme === 'dark' ? <Sun className="w-4 h-4 sm:w-5 sm:h-5" /> : <Moon className="w-4 h-4 sm:w-5 sm:h-5" />}
                    <span className="absolute -bottom-8 left-1/2 -translate-x-1/2 text-[10px] bg-stash-surface border border-stash-border px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 shadow-lg">
                        {theme === 'dark' ? t('common.light_mode') : t('common.dark_mode')}
                    </span>
                </button>
            </div>
        </header>
    )
}
