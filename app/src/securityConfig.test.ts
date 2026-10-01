import { describe, expect, it } from 'vitest';
import config from '../src-tauri/tauri.conf.json';

describe('security configuration regression', () => {
    it('keeps scripts self-contained and does not restore unsafe eval', () => {
        const csp = config.app.security.csp;
        expect(csp).toContain("script-src 'self'");
        expect(csp).not.toContain('unsafe-eval');
        expect(csp).not.toMatch(/script-src[^;]*https:/);
    });
});
