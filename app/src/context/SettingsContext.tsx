import { createContext, useContext, useState, useEffect, ReactNode, useCallback } from 'react';
import { load } from '@tauri-apps/plugin-store';
import { SupportedLanguage } from '../i18n/languages';
import { folderPresentationKey, getFolderPresentation, sanitizeFolderPresentationMap } from '../folderPresentation';
import type { FolderPresentation, FolderPresentationMap } from '../folderPresentation';

export interface Settings {
    viewMode: 'grid' | 'list' | 'posters';
    folderPresentation: FolderPresentationMap;
    autoUpdate: boolean;
    maxConcurrentUploads: number;
    maxConcurrentDownloads: number;
    language: SupportedLanguage;

    // ── Sidebar ─────────────────────────────────────────────
    sidebarCollapsed: boolean;
    hideGroups: boolean;

    windowsAutostart: boolean;       // Launch on Windows Startup
}

const defaultSettings: Settings = {
    viewMode: 'grid',
    folderPresentation: {},
    autoUpdate: true,
    maxConcurrentUploads: 6,
    maxConcurrentDownloads: 6,
    language: 'en',

    // Sidebar
    sidebarCollapsed: false,
    hideGroups: false,

    windowsAutostart: false,
};

interface SettingsContextType {
    settings: Settings;
    updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
    updateFolderPresentation: (folderId: number | null, patch: Partial<FolderPresentation>) => void;
    resetSettings: () => void;
    replaceSettings: (next: Settings) => void;
    isLoaded: boolean;
}

const SettingsContext = createContext<SettingsContextType | undefined>(undefined);

export function SettingsProvider({ children }: { children: ReactNode }) {
    const [settings, setSettings] = useState<Settings>(defaultSettings);
    const [isLoaded, setIsLoaded] = useState(false);

    // Load settings from Tauri store on mount
    useEffect(() => {
        const loadSettings = async () => {
            try {
                const store = await load('settings.json');
                const saved = await store.get<Settings>('settings');
                if (saved) {
                    const savedSettings = { ...saved } as Record<string, unknown>;
                    for (const key of [
                        'performanceMode', 'vpnMode', 'timeoutMultiplier', 'retryAttempts',
                        'retryBaseBackoffSec', 'retryMaxBackoffSec', 'adaptivePolling',
                        'pollingMinSec', 'pollingMaxSec', 'preferredDC', 'dcFallbackAttempts',
                        'floodWaitRespect', 'peerCacheSize', 'bandwidthLimitUpKBs',
                        'bandwidthLimitDownKBs', 'chunkSizeKb', 'keepAliveIntervalSec',
                        'autoDetectVpn', 'archiveMaxBytes', 'transcodeCacheMaxGb',
                        // Legacy keys removed in earlier versions — purge stale copies.
                        'tmdbEnabled', 'tmdbApiKey', 'videoThumbnails',
                    ]) {
                        delete savedSettings[key];
                    }
                    // Merge with defaults so new keys are always present
                    const merged = {
                        ...defaultSettings,
                        ...savedSettings,
                        folderPresentation: sanitizeFolderPresentationMap(savedSettings.folderPresentation),
                    } as Settings;
                    setSettings(merged);
                }
            } catch {
                // Store not available or first run — use defaults
            } finally {
                setIsLoaded(true);
            }
        };
        loadSettings();
    }, []);

    const persistSettings = useCallback(async (next: Settings) => {
        try {
            const store = await load('settings.json');
            await store.set('settings', next);
            await store.save();
        } catch {
            // best-effort persistence
        }
    }, []);

    // Persist settings whenever they change after the initial load completes
    useEffect(() => {
        if (isLoaded) {
            persistSettings(settings);
        }
    }, [settings, isLoaded, persistSettings]);

    const updateSetting = useCallback(<K extends keyof Settings>(key: K, value: Settings[K]) => {
        setSettings(prev => ({ ...prev, [key]: value }));
    }, []);

    const updateFolderPresentation = useCallback((folderId: number | null, patch: Partial<FolderPresentation>) => {
        setSettings(prev => {
            const current = getFolderPresentation(prev.folderPresentation, folderId);
            const next = { ...current, ...patch, kind: patch.kind ?? current.kind };
            if (next.viewMode === undefined) delete next.viewMode;
            return {
                ...prev,
                folderPresentation: { ...prev.folderPresentation, [folderPresentationKey(folderId)]: next },
            };
        });
    }, []);

    const resetSettings = useCallback(() => {
        setSettings(defaultSettings);
    }, []);

    const replaceSettings = useCallback((next: Settings) => {
        setSettings(next);
    }, []);

    return (
        <SettingsContext.Provider value={{ settings, updateSetting, updateFolderPresentation, resetSettings, replaceSettings, isLoaded }}>
            {children}
        </SettingsContext.Provider>
    );
}

export const useSettings = () => {
    const context = useContext(SettingsContext);
    if (!context) throw new Error('useSettings must be used within a SettingsProvider');
    return context;
};
