import { Archive, ChartPie, Gear, Moon, Sun, UploadSimple } from '@phosphor-icons/react';
import { RefreshCw } from '../../shared/icons';
import { useState } from 'react';
import { BandwidthStats } from '../../../types';
import { formatBytes } from '../../../utils';

interface VaultRailProps {
    onFolders: () => void;
    onUpload: () => void;
    onAnalytics: () => void;
    onSettings: () => void;
    onToggleTheme: () => void;
    theme: 'dark' | 'light';
    isConnected: boolean;
    isSyncing?: boolean;
    onSync?: () => void;
    bandwidth?: BandwidthStats | null;
    uploadCount?: number;
    downloadCount?: number;
}

export function VaultRail({ onFolders, onUpload, onAnalytics, onSettings, onToggleTheme, theme, isConnected, isSyncing = false, onSync, bandwidth = null, uploadCount = 0, downloadCount = 0 }: VaultRailProps) {
    const [syncOpen, setSyncOpen] = useState(false);

    const actions = [
        { label: 'Collections', icon: Archive, onClick: onFolders },
        { label: 'Upload', icon: UploadSimple, onClick: onUpload },
        { label: 'Analytics', icon: ChartPie, onClick: onAnalytics },
    ];

    const syncLabel = isSyncing ? 'Syncing' : 'Sync status';
    const usageBytes = (bandwidth?.up_bytes ?? 0) + (bandwidth?.down_bytes ?? 0);
    const usageLimit = 250 * 1024 * 1024 * 1024; // 250 GB
    const usagePercent = Math.min((usageBytes / usageLimit) * 100, 100);

    return (
        <aside className="vault-rail" aria-label="Vault navigation">
            <button className="vault-rail-brand" onClick={onFolders} data-tooltip="Collections" aria-label="Open collections">
                <span className="vault-rail-mark">◆</span>
            </button>
            <div className="vault-rail-rule" />
            <nav className="vault-rail-nav">
                {actions.map(({ label, icon: Icon, onClick }) => (
                    <button key={label} className="vault-rail-action" onClick={onClick} data-tooltip={label} aria-label={label}>
                        <Icon size={20} weight="regular" />
                        {label === 'Upload' && uploadCount + downloadCount > 0 && <span className="vault-rail-badge">{uploadCount + downloadCount}</span>}
                    </button>
                ))}
            </nav>
            <div className="vault-rail-bottom">
                <div
                    className="vault-sync-button"
                    data-open={syncOpen}
                    onMouseEnter={() => setSyncOpen(true)}
                    onMouseLeave={() => setSyncOpen(false)}
                    onClick={() => setSyncOpen(v => !v)}
                    onKeyDown={(e) => { if (e.key === 'Escape') setSyncOpen(false); }}
                    aria-label={syncLabel}
                    role="button"
                    tabIndex={0}
                >
                    <button
                        className="vault-rail-action"
                        data-tooltip={syncLabel}
                        aria-label={syncLabel}
                        aria-expanded={syncOpen}
                    >
                        <span className={`vault-connection-dot ${isConnected ? 'is-online' : 'is-offline'}`} />
                    </button>
                    {syncOpen && (
                        <div className="vault-sync-pop" role="dialog" aria-label="Sync status">
                            <div className="vault-sync-pop-head">
                                <span className="vault-eyebrow">Vault sync</span>
                                <strong>{isConnected ? 'Connected' : 'Disconnected'}</strong>
                            </div>
                            <div className="vault-sync-pop-status">
                                <span className={`vault-connection-dot ${isConnected ? 'is-online' : 'is-offline'}`} />
                                <span>{isConnected ? 'Live MTProto session' : 'No active session'}</span>
                            </div>
                            <div className="vault-sync-pop-usage">
                                <span>Used today</span>
                                <span>{formatBytes(usageBytes)}</span>
                            </div>
                            <div className="vault-sync-pop-track"><i style={{ width: `${usagePercent}%` }} /></div>
                            <div className="vault-sync-pop-legend">
                                <span>{formatBytes(bandwidth?.up_bytes ?? 0)} up</span>
                                <span>{formatBytes(bandwidth?.down_bytes ?? 0)} down</span>
                                <span>250 GB</span>
                            </div>
                            {onSync && (
                                <button className="vault-sync-pop-action" onClick={onSync} disabled={isSyncing}>
                                    <RefreshCw className={`w-3.5 h-3.5 ${isSyncing ? 'animate-spin' : ''}`} />
                                    {isSyncing ? 'Syncing folders…' : 'Sync folders'}
                                </button>
                            )}
                        </div>
                    )}
                </div>
                <button className="vault-rail-action" onClick={onSettings} data-tooltip="Settings" aria-label="Settings"><Gear size={20} /></button>
                <button className="vault-rail-action" onClick={onToggleTheme} data-tooltip={theme === 'dark' ? 'Light mode' : 'Dark mode'} aria-label="Toggle theme">
                    {theme === 'dark' ? <Sun size={20} /> : <Moon size={20} />}
                </button>
            </div>
        </aside>
    );
}
