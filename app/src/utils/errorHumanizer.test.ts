import { describe, it, expect } from 'vitest';
import { classifyErrorText, humanizeError } from './errorHumanizer';

// Mirrors src-tauri/failure_classifier.rs. If a backend rule changes, this is
// the test that should fail first, so the two stay in step.
describe('classifyErrorText', () => {
    it('classifies cancellation first so it wins over other keywords', () => {
        // A cancelled upload often carries unrelated words; cancel must win.
        expect(classifyErrorText('Transfer cancelled by user')).toBe('cancelled');
        expect(classifyErrorText('CANCEL')).toBe('cancelled');
    });

    it('classifies rate limiting', () => {
        expect(classifyErrorText('FLOOD_WAIT_42')).toBe('flood_wait');
        expect(classifyErrorText('flood wait 42 seconds')).toBe('flood_wait');
    });

    it('classifies oversized files', () => {
        expect(classifyErrorText('FILE_TOO_BIG')).toBe('too_large');
        expect(classifyErrorText('file exceeds the 2 GB limit')).toBe('too_large');
    });

    it('classifies local filesystem problems', () => {
        expect(classifyErrorText('Failed to open file')).toBe('local_file');
        expect(classifyErrorText('Access is denied')).toBe('local_file');
        expect(classifyErrorText('No such file or directory')).toBe('local_file');
    });

    it('classifies split manifest problems', () => {
        expect(classifyErrorText('part missing in split manifest')).toBe('manifest_split');
        expect(classifyErrorText('size mismatch')).toBe('manifest_split');
    });

    it('classifies transport failures', () => {
        expect(classifyErrorText('connection reset by peer')).toBe('network_transport');
        expect(classifyErrorText('os error 10054')).toBe('network_transport');
        expect(classifyErrorText('operation timed out')).toBe('network_transport');
        // Regression: a truncated read must not look like a clean EOF.
        expect(classifyErrorText('read 0 bytes')).toBe('network_transport');
        expect(classifyErrorText('reached EOF before the end')).toBe('network_transport');
    });

    it('falls back to unknown for unrecognized text', () => {
        expect(classifyErrorText('something entirely unexpected')).toBe('unknown');
        expect(classifyErrorText('')).toBe('unknown');
    });

    it('is case-insensitive', () => {
        expect(classifyErrorText('CONNECTION RESET')).toBe('network_transport');
        expect(classifyErrorText('Connection Reset')).toBe('network_transport');
    });
});

describe('humanizeError', () => {
    it('resolves a localized message for the classified kind', () => {
        // i18next's TFunction carries a type brand, so build the stub through a
        // cast rather than pretending to implement the whole signature.
        const t = ((key: string, opts?: Record<string, unknown>) =>
            `${key}|${opts?.action}`) as unknown as Parameters<typeof humanizeError>[2];

        expect(humanizeError('connection reset', 'Uploading', t)).toBe(
            'errors.kind_network_transport|Uploading',
        );
        expect(humanizeError('unknown gobbledygook', 'Downloading', t)).toBe(
            'errors.kind_unknown|Downloading',
        );
    });

    it('stringifies non-Error throwables instead of crashing', () => {
        const t = ((key: string) => key) as unknown as Parameters<typeof humanizeError>[2];
        expect(humanizeError('plain string', 'x', t)).toBe('errors.kind_unknown');
        expect(humanizeError(null, 'x', t)).toBe('errors.kind_unknown');
    });
});
