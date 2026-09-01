import { Archive, ChartPie, Gear, Moon, Sun, UploadSimple } from '@phosphor-icons/react';

interface VaultRailProps {
    onFolders: () => void;
    onUpload: () => void;
    onAnalytics: () => void;
    onSettings: () => void;
    onToggleTheme: () => void;
    theme: 'dark' | 'light';
    isConnected: boolean;
    uploadCount?: number;
    downloadCount?: number;
}

export function VaultRail({ onFolders, onUpload, onAnalytics, onSettings, onToggleTheme, theme, isConnected, uploadCount = 0, downloadCount = 0 }: VaultRailProps) {
    const actions = [
        { label: 'Collections', icon: Archive, onClick: onFolders },
        { label: 'Upload', icon: UploadSimple, onClick: onUpload },
        { label: 'Analytics', icon: ChartPie, onClick: onAnalytics },
    ];

    return (
        <aside className="vault-rail" aria-label="Vault navigation">
            <button className="vault-rail-brand" onClick={onFolders} aria-label="Open collections">
                <span className="vault-rail-mark">◆</span>
            </button>
            <div className="vault-rail-rule" />
            <nav className="vault-rail-nav">
                {actions.map(({ label, icon: Icon, onClick }) => (
                    <button key={label} className="vault-rail-action" onClick={onClick} title={label} aria-label={label}>
                        <Icon size={20} weight="regular" />
                        {label === 'Upload' && uploadCount + downloadCount > 0 && <span className="vault-rail-badge">{uploadCount + downloadCount}</span>}
                    </button>
                ))}
            </nav>
            <div className="vault-rail-bottom">
                <span className={`vault-connection-dot ${isConnected ? 'is-online' : 'is-offline'}`} title={isConnected ? 'Connected' : 'Disconnected'} />
                <button className="vault-rail-action" onClick={onSettings} title="Settings" aria-label="Settings"><Gear size={20} /></button>
                <button className="vault-rail-action" onClick={onToggleTheme} title="Toggle theme" aria-label="Toggle theme">
                    {theme === 'dark' ? <Sun size={20} /> : <Moon size={20} />}
                </button>
            </div>
        </aside>
    );
}
