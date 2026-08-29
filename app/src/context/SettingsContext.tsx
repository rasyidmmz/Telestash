import { createContext, useContext, useState, useEffect, ReactNode, useCallback } from 'react';
import { load } from '@tauri-apps/plugin-store';
import { SupportedLanguage } from '../i18n/languages';

export interface Settings {
    viewMode: 'grid' | 'list';
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
    resetSettings: () => void;
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
                    ]) {
                        delete savedSettings[key];
                    }
                    // Merge with defaults so new keys are always present
                    const merged = { ...defaultSettings, ...savedSettings } as Settings;
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

    const resetSettings = useCallback(() => {
        setSettings(defaultSettings);
    }, []);

    return (
        <SettingsContext.Provider value={{ settings, updateSetting, resetSettings, isLoaded }}>
            {children}
        </SettingsContext.Provider>
    );
}

export const useSettings = () => {
    const context = useContext(SettingsContext);
    if (!context) throw new Error('useSettings must be used within a SettingsProvider');
    return context;
};
