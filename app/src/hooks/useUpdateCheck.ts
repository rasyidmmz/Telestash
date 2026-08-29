import { useState, useEffect, useCallback, useRef } from 'react';
import { check, Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

interface UpdateState {
    checking: boolean;
    available: boolean;
    downloading: boolean;
    installing: boolean;
    restarting: boolean;
    progress: number;
    error: string | null;
    version: string | null;
}

interface UseUpdateCheckOptions {
    autoCheck?: boolean;
    intervalMs?: number;
}

// Re-checking on window restore is throttled so alt-tabbing back and forth
// does not spam the update endpoint.
const VISIBILITY_RECHECK_THROTTLE_MS = 10 * 60 * 1000;

function updateErrorMessage(err: unknown, fallback: string) {
    if (err instanceof Error && err.message) return err.message;
    if (typeof err === 'string' && err.trim()) return err;
    if (err && typeof err === 'object' && 'message' in err) {
        const message = String((err as { message?: unknown }).message || '').trim();
        if (message) return message;
    }
    return fallback;
}

export function useUpdateCheck(options: UseUpdateCheckOptions = {}) {
    const { autoCheck = true, intervalMs } = options;
    const [state, setState] = useState<UpdateState>({
        checking: false,
        available: false,
        downloading: false,
        installing: false,
        restarting: false,
        progress: 0,
        error: null,
        version: null,
    });
    const [update, setUpdate] = useState<Update | null>(null);
    const checkingRef = useRef(false);
    const lastCheckAtRef = useRef(0);

    const checkForUpdates = useCallback(async (): Promise<Update | null | undefined> => {
        if (checkingRef.current) return undefined;
        checkingRef.current = true;
        setState(s => ({ ...s, checking: true, error: null }));
        try {
            const updateInfo = await check();
            lastCheckAtRef.current = Date.now();
            if (updateInfo) {
                setUpdate(updateInfo);
                setState(s => ({
                    ...s,
                    checking: false,
                    available: true,
                    version: updateInfo.version,
                }));
                return updateInfo;
            } else {
                setState(s => ({ ...s, checking: false, available: false, version: null }));
                return null;
            }
        } catch (err: unknown) {
            const message = updateErrorMessage(err, 'Failed to check for updates');
            lastCheckAtRef.current = Date.now();
            setState(s => ({
                ...s,
                checking: false,
                available: false,
                error: message,
            }));
            return undefined;
        } finally {
            checkingRef.current = false;
        }
    }, []);

    const downloadAndInstall = useCallback(async () => {
        if (!update) return;

        setState(s => ({ ...s, downloading: true, installing: false, restarting: false, error: null, progress: 0 }));
        let downloaded = 0;
        let contentLength = 0;

        try {
            await update.downloadAndInstall((event) => {
                if (event.event === 'Started') {
                    const data = event.data as { contentLength?: number };
                    contentLength = data.contentLength || 0;
                } else if (event.event === 'Progress') {
                    const data = event.data as { chunkLength?: number };
                    downloaded += data.chunkLength || 0;
                    if (contentLength > 0) {
                        const pct = Math.round((downloaded / contentLength) * 100);
                        setState(s => ({ ...s, progress: Math.min(pct, 100) }));
                    }
                } else if (event.event === 'Finished') {
                    setState(s => ({ ...s, downloading: false, installing: true, progress: 100 }));
                }
            });

        } catch (err: unknown) {
            const message = updateErrorMessage(err, 'Failed to install update');
            setState(s => ({
                ...s,
                downloading: false,
                installing: false,
                restarting: false,
                error: message,
            }));
            return;
        }

        setState(s => ({ ...s, installing: false, restarting: true, progress: 100 }));
        try {
            await relaunch();
        } catch (err: unknown) {
            const message = updateErrorMessage(err, 'Update installed, but TeleStash could not restart automatically');
            setState(s => ({
                ...s,
                restarting: false,
                error: message,
            }));
        }
    }, [update]);

    const dismissUpdate = useCallback(() => {
        setState(s => ({ ...s, available: false, error: null }));
        setUpdate(null);
    }, []);

    useEffect(() => {
        if (!autoCheck) return;
        // No update traffic while the window is hidden in the tray; the
        // visibility listener catches up as soon as it is visible again.
        const tick = () => {
            if (document.hidden) return;
            checkForUpdates().catch(console.error);
        };
        const timer = setTimeout(tick, 5000);
        // Keep looking for new releases while the app stays open so the
        // update banner appears without an app restart.
        const interval = intervalMs && intervalMs > 0
            ? setInterval(tick, intervalMs)
            : null;
        const onVisibility = () => {
            if (document.hidden) return;
            if (Date.now() - lastCheckAtRef.current < VISIBILITY_RECHECK_THROTTLE_MS) return;
            checkForUpdates().catch(console.error);
        };
        document.addEventListener('visibilitychange', onVisibility);
        return () => {
            clearTimeout(timer);
            if (interval) clearInterval(interval);
            document.removeEventListener('visibilitychange', onVisibility);
        };
    }, [autoCheck, intervalMs, checkForUpdates]);

    return {
        ...state,
        checkForUpdates,
        downloadAndInstall,
        dismissUpdate,
    };
}
