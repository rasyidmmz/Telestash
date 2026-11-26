import { describe, expect, it } from 'vitest';
import { createBackup, parseBackup, serializeBackup } from './backup';
import type { Settings } from './context/SettingsContext';

const settings: Settings = {
    viewMode: 'grid',
    folderPresentation: {},
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
    it('round trips home and folder presentation preferences through a backup', () => {
        const current: Settings = {
            ...settings,
            folderPresentation: {
                home: { kind: 'general', viewMode: 'list' },
                '42': { kind: 'series' },
                '43': { kind: 'movies', viewMode: 'grid' },
            },
        };
        expect(parseBackup(serializeBackup(current)).settings.folderPresentation).toEqual({
            home: { kind: 'general', viewMode: 'list' },
            '42': { kind: 'series' },
            '43': { kind: 'movies', viewMode: 'grid' },
        });
    });

    it('loads version 1 backups without folder preferences as unassigned', () => {
        const backup = createBackup(settings);
        const { folderPresentation: _removed, ...legacySettings } = backup.settings as typeof backup.settings & { folderPresentation?: unknown };
        expect(parseBackup(JSON.stringify({ ...backup, settings: legacySettings })).settings.folderPresentation).toEqual({});
    });

    it.each([
        null, [], 'broken',
        { 12: { kind: 'documentary' } },
        { 12: { kind: 'series', viewMode: 'tiles' } },
        { 12: { kind: 'movies', viewMode: null } },
        { 12: { kind: 'movies', secret: 'not allowed' } },
        { arbitrary: { kind: 'general' } },
    ].map(value => [value]))('rejects malformed folder preferences before importing: %j', folderPresentation => {
        const backup = createBackup(settings);
        expect(() => parseBackup(JSON.stringify({ ...backup, settings: { ...settings, folderPresentation } }))).toThrow();
    });

});
