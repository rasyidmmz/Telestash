import i18n from '../i18n';

/**
 * Frontend port of the backend failure_classifier rules (src-tauri
 * failure_classifier.rs) so user-facing toasts speak human language while the
 * raw error text stays available in Transfer Logs.
 */

export type ErrorKind =
    | 'local_file'
    | 'manifest_split'
    | 'telegram_server'
    | 'flood_wait'
    | 'network_transport'
    | 'too_large'
    | 'cancelled'
    | 'unknown';

export function classifyErrorText(raw: string): ErrorKind {
    const text = raw.toLowerCase();

    if (text.includes('cancel')) return 'cancelled';
    if (text.includes('flood_wait') || text.includes('flood wait')) return 'flood_wait';
    if (
        text.includes('file_too_big') ||
        text.includes('too large') ||
        text.includes('exceeds')
    ) {
        return 'too_large';
    }
    if (
        text.includes('failed to open') ||
        text.includes('access is denied') ||
        text.includes('no such file') ||
        text.includes('cannot find the file')
    ) {
        return 'local_file';
    }
    if (
        text.includes('split') ||
        text.includes('manifest') ||
        text.includes('part missing') ||
        text.includes('size mismatch')
    ) {
        return 'manifest_split';
    }
    if (
        text.includes('flood_wait') ||
        text.includes('rpc error') ||
        text.includes('telegram') ||
        text.includes('upload.save')
    ) {
        return 'telegram_server';
    }
    if (
        text.includes('read 0 bytes') ||
        text.includes('reached eof before') ||
        text.includes('unexpected eof') ||
        text.includes('connection reset') ||
        text.includes('connection aborted') ||
        text.includes('forcibly closed') ||
        text.includes('os error 10054') ||
        text.includes('broken pipe') ||
        text.includes('timed out') ||
        text.includes('timeout')
    ) {
        return 'network_transport';
    }
    return 'unknown';
}

/**
 * Map a backend error into a localized, human-readable toast message.
 * `action` is an already-localized action label (e.g. t('errors.action_upload')).
 * The raw error stays visible in Transfer Logs / queue entries for diagnostics.
 */
export function humanizeError(e: unknown, action: string, t = i18n.t.bind(i18n)): string {
    const raw = String(e);
    const kind = classifyErrorText(raw);
    return t(`errors.kind_${kind}`, { action });
}
