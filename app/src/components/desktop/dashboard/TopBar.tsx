import { HardDrive, SquaresFour, List, X, Scroll, FilmStrip, MagnifyingGlass } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';
import { useErrorLogs } from '../../../errorLogs';

interface TopBarProps {
    currentFolderName: string;
    selectedIds: readonly (number | string)[];
    onShowMoveModal: () => void;
    onBulkDownload: () => void;
    onBulkDelete: () => void;
    onBulkShare: () => void;
    onDownloadFolder: () => void;
    onClearSelection: () => void;
    viewMode: 'grid' | 'list' | 'posters';
    setViewMode: (mode: 'grid' | 'list' | 'posters') => void;
    searchTerm: string;
    onSearchChange: (term: string) => void;
    onLogsClick: () => void;
    onWatchLogsClick?: () => void;
    /** Context label above the folder name, e.g. "Saved Messages" or "Channel". */
    contextLabel?: string;
}

export function TopBar({
    currentFolderName, selectedIds, onShowMoveModal, onBulkDownload, onBulkDelete, onBulkShare,
    onDownloadFolder, onClearSelection, viewMode, setViewMode, searchTerm, onSearchChange,
    onLogsClick, onWatchLogsClick, contextLabel = 'Workspace'
}: TopBarProps) {
    const { t } = useTranslation();
    const errorLogs = useErrorLogs();
    return (
        <header className="vault-command-bar" onClick={e => e.stopPropagation()}>
            <div className="vault-command-context">
                <span className="vault-eyebrow">{contextLabel}</span>
                <strong title={currentFolderName}>{currentFolderName}</strong>
                <span className="vault-command-separator">·</span>
                <span className="vault-command-count">{selectedIds.length > 0 ? `${selectedIds.length} selected` : 'Private vault'}</span>
            </div>

            <div className="vault-command-search">
                <MagnifyingGlass className="vault-search-glyph" size={16} aria-hidden="true" />
                <input type="text" placeholder={t('common.search_placeholder')} aria-label={t('common.search_placeholder')} value={searchTerm} onChange={(e) => onSearchChange(e.target.value)} />
                <span className="vault-search-hint">Ctrl+F</span>
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
                <button className="vault-icon-button" onClick={onDownloadFolder} data-tooltip={t('files.download_folder')} aria-label={t('files.download_folder')}><HardDrive size={18} /></button>
                <div className="vault-view-switcher" role="group" aria-label="Library view">
                    {([
                        ['list', 'List view', List],
                        ['grid', 'Grid view', SquaresFour],
                        ['posters', 'Poster view', FilmStrip],
                    ] as const).map(([mode, label, Icon]) => (
                        <button key={mode} type="button" className="vault-icon-button"
                            onClick={() => setViewMode(mode)} aria-label={label} title={label}
                            aria-pressed={viewMode === mode}><Icon size={18} /></button>
                    ))}
                </div>
                <span className="vault-command-divider" />
                {onWatchLogsClick && <button className="vault-icon-button" onClick={onWatchLogsClick} data-tooltip="Watch history" aria-label="Watch history"><FilmStrip size={18} /></button>}
                <button className="vault-icon-button" onClick={onLogsClick} data-tooltip="Error logs" aria-label="Error logs"><Scroll size={18} />{errorLogs.length > 0 && <i className="vault-alert-dot" />}</button>
            </div>
        </header>
    )
}
