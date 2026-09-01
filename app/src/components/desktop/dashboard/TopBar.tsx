import { HardDrive, SquaresFour, List, Sun, Moon, Gear, X, Scroll, FilmStrip, ChartDonut } from '@phosphor-icons/react';
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
}

export function TopBar({
    currentFolderName, selectedIds, onShowMoveModal, onBulkDownload, onBulkDelete, onBulkShare,
    onDownloadFolder, onClearSelection, viewMode, setViewMode, searchTerm, onSearchChange, onSettingsClick,
    onLogsClick, onWatchLogsClick, onAnalyticsClick
}: TopBarProps) {
    const { theme, toggleTheme } = useTheme();
    const { t } = useTranslation();
    const errorLogs = useErrorLogs();
    return (
        <header className="vault-command-bar" onClick={e => e.stopPropagation()}>
            <div className="vault-command-context">
                <span className="vault-eyebrow">Workspace</span>
                <strong title={currentFolderName}>{currentFolderName}</strong>
                <span className="vault-command-separator">·</span>
                <span className="vault-command-count">{selectedIds.length > 0 ? `${selectedIds.length} selected` : 'Private vault'}</span>
            </div>

            <div className="vault-command-search">
                <span className="vault-search-glyph">⌘</span>
                <input type="text" placeholder={t('common.search_placeholder')} aria-label={t('common.search_placeholder')} value={searchTerm} onChange={(e) => onSearchChange(e.target.value)} />
                <span className="vault-search-hint">/</span>
            </div>

            <div className="vault-command-actions">
                {selectedIds.length > 0 && (
                    <div className="vault-selection-strip">
                        <span>{t('files.items_selected', { count: selectedIds.length })}</span>
                        <button onClick={onShowMoveModal}>{t('files.move_to')}</button>
                        <button onClick={onBulkDownload}>{t('files.download_selected')}</button>
                        <button onClick={onBulkShare}>{t('files.share')}</button>
                        <button className="is-danger" onClick={onBulkDelete}>{t('files.delete')}</button>
                        <button className="vault-icon-button" onClick={onClearSelection} aria-label={t('files.clear_selection')}><X size={16} /></button>
                    </div>
                )}
                <button className="vault-icon-button" onClick={onDownloadFolder} title={t('files.download_folder')} aria-label={t('files.download_folder')}><HardDrive size={18} /></button>
                <button className="vault-icon-button" onClick={() => setViewMode(viewMode === 'grid' ? 'list' : 'grid')} title={t('files.toggle_layout')} aria-label={t('files.toggle_layout')}>
                    {viewMode === 'grid' ? <SquaresFour size={18} /> : <List size={18} />}
                </button>
                <span className="vault-command-divider" />
                {onAnalyticsClick && <button className="vault-icon-button" onClick={onAnalyticsClick} title="Storage analytics" aria-label="Storage analytics"><ChartDonut size={18} /></button>}
                {onWatchLogsClick && <button className="vault-icon-button" onClick={onWatchLogsClick} title="Watch history" aria-label="Watch history"><FilmStrip size={18} /></button>}
                <button className="vault-icon-button" onClick={onLogsClick} title="Error logs" aria-label="Error logs"><Scroll size={18} />{errorLogs.length > 0 && <i className="vault-alert-dot" />}</button>
                <button className="vault-icon-button" onClick={onSettingsClick} title={t('common.settings')} aria-label={t('common.settings')}><Gear size={18} /></button>
                <button className="vault-icon-button" onClick={toggleTheme} title={theme === 'dark' ? t('common.switch_light') : t('common.switch_dark')} aria-label="Toggle theme">{theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}</button>
            </div>
        </header>
    )
}
