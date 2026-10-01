import type { Settings } from './context/SettingsContext';

export const BACKUP_SCHEMA = 'telestash-settings-backup';
export const BACKUP_VERSION = 1;

export interface BackupPayload {
    schema: typeof BACKUP_SCHEMA;
    version: number;
    exportedAt: string;
    settings: Settings;
    theme: 'light' | 'dark' | null;
    customThemes: unknown[];
    activeCustomThemeId: string | null;
}

const SETTINGS_KEYS: (keyof Settings)[] = [
    'viewMode', 'autoUpdate', 'maxConcurrentUploads', 'maxConcurrentDownloads',
    'language', 'sidebarCollapsed', 'hideGroups', 'windowsAutostart',
];

export function createBackup(settings: Settings): BackupPayload {
    return {
        schema: BACKUP_SCHEMA,
        version: BACKUP_VERSION,
        exportedAt: new Date().toISOString(),
        settings: { ...settings },
        theme: readTheme(),
        customThemes: readJsonArray('user-themes'),
        activeCustomThemeId: readString('active-custom-theme-id'),
    };
}

export function serializeBackup(settings: Settings): string {
    return JSON.stringify(createBackup(settings), null, 2);
}

export function parseBackup(raw: string): BackupPayload {
    let parsed: unknown;
    try {
        parsed = JSON.parse(raw);
    } catch {
        throw new Error('Berkas cadangan bukan JSON yang valid.');
    }
    if (!parsed || typeof parsed !== 'object') throw new Error('Format cadangan tidak valid.');
    const candidate = parsed as Record<string, unknown>;
    if (candidate.schema !== BACKUP_SCHEMA || candidate.version !== BACKUP_VERSION) {
        throw new Error(`Versi cadangan tidak didukung (schema ${String(candidate.schema)}, versi ${String(candidate.version)}).`);
    }
    if (!candidate.settings || typeof candidate.settings !== 'object') throw new Error('Bagian pengaturan tidak ditemukan.');
    const settings = candidate.settings as Record<string, unknown>;
    for (const key of SETTINGS_KEYS) {
        if (!(key in settings)) throw new Error(`Pengaturan wajib hilang: ${String(key)}.`);
    }
    if (!['grid', 'list', 'posters'].includes(String(settings.viewMode))) throw new Error('Mode tampilan cadangan tidak valid.');
    if (!Number.isInteger(settings.maxConcurrentUploads) || Number(settings.maxConcurrentUploads) < 1 || Number(settings.maxConcurrentUploads) > 10) throw new Error('Batas upload tidak valid.');
    if (!Number.isInteger(settings.maxConcurrentDownloads) || Number(settings.maxConcurrentDownloads) < 1 || Number(settings.maxConcurrentDownloads) > 10) throw new Error('Batas download tidak valid.');
    return {
        schema: BACKUP_SCHEMA,
        version: BACKUP_VERSION,
        exportedAt: typeof candidate.exportedAt === 'string' ? candidate.exportedAt : new Date().toISOString(),
        settings: settings as unknown as Settings,
        theme: candidate.theme === 'light' || candidate.theme === 'dark' ? candidate.theme : null,
        customThemes: Array.isArray(candidate.customThemes) ? candidate.customThemes : [],
        activeCustomThemeId: typeof candidate.activeCustomThemeId === 'string' ? candidate.activeCustomThemeId : null,
    };
}

export function applyBackup(payload: BackupPayload): void {
    localStorage.setItem('theme', payload.theme ?? 'dark');
    localStorage.setItem('user-themes', JSON.stringify(payload.customThemes));
    localStorage.setItem('active-custom-theme-id', payload.activeCustomThemeId ?? '');
}

function readTheme(): 'light' | 'dark' | null {
    const value = readString('theme');
    return value === 'light' || value === 'dark' ? value : null;
}

function readString(key: string): string | null {
    try { return localStorage.getItem(key); } catch { return null; }
}

function readJsonArray(key: string): unknown[] {
    try {
        const value = JSON.parse(localStorage.getItem(key) ?? '[]');
        return Array.isArray(value) ? value : [];
    } catch { return []; }
}
