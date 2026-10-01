import { describe, expect, it } from 'vitest';
import { createBackup, parseBackup, serializeBackup } from './backup';
import type { Settings } from './context/SettingsContext';

const settings: Settings = {
    viewMode: 'grid',
    autoUpdate: true,
    maxConcurrentUploads: 3,
    maxConcurrentDownloads: 4,
    language: 'id',
    sidebarCollapsed: false,
    hideGroups: false,
    windowsAutostart: false,
};

describe('settings backup', () => {
    it('serializes only non-secret settings', () => {
        const raw = serializeBackup(settings);
        expect(raw).toContain('telestash-settings-backup');
        expect(raw).not.toContain('api_hash');
        expect(raw).not.toContain('session');
        expect(parseBackup(raw).settings).toEqual(settings);
    });

    it('rejects unsupported schema and invalid limits', () => {
        expect(() => parseBackup(JSON.stringify({ schema: 'other', version: 1 }))).toThrow('Versi cadangan');
        const backup = createBackup(settings);
        expect(() => parseBackup(JSON.stringify({ ...backup, settings: { ...settings, maxConcurrentUploads: 99 } }))).toThrow('Batas upload');
    });
});
