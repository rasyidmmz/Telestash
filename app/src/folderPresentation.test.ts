import { describe, expect, it } from 'vitest';
import * as presentation from './folderPresentation';

describe('folder presentation', () => {
    it('uses a stable folder ID, with a separate home preference and a general default', () => {
        expect(typeof presentation.getFolderPresentation).toBe('function');
        const saved = {
            home: { kind: 'movies' as const },
            '42': { kind: 'series' as const, viewMode: 'grid' as const },
        };
        expect(presentation.getFolderPresentation(saved, null)).toEqual({ kind: 'movies' });
        expect(presentation.getFolderPresentation(saved, 42)).toEqual({ kind: 'series', viewMode: 'grid' });
        expect(presentation.getFolderPresentation(saved, 43)).toEqual({ kind: 'general' });
    });

    it('uses explicit views before kind defaults and the global view for general folders', () => {
        expect(typeof presentation.getEffectiveViewMode).toBe('function');
        expect(presentation.getEffectiveViewMode({ kind: 'series' }, 'grid')).toBe('list');
        expect(presentation.getEffectiveViewMode({ kind: 'movies' }, 'grid')).toBe('posters');
        expect(presentation.getEffectiveViewMode({ kind: 'general' }, 'list')).toBe('list');
        expect(presentation.getEffectiveViewMode({ kind: 'series', viewMode: 'posters' }, 'grid')).toBe('posters');
        expect(presentation.getEffectiveViewMode({ kind: 'movies', viewMode: 'grid' }, 'list')).toBe('grid');
    });
});
