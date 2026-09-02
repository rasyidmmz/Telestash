import { useState, useEffect, useRef } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export interface FloodWaitEvent {
    wait_seconds: number;
    attempt: number;
    max_attempts: number;
}

export interface FloodWaitState {
    /** Total seconds Telegram asked us to wait (capped at 300 by the backend). */
    waitSeconds: number;
    /** When the wait started (epoch ms), for live countdown. */
    startedAt: number;
    attempt: number;
    maxAttempts: number;
}

/**
 * Listens for `flood-wait` events emitted by the Rust backend whenever a
 * transfer enters Telegram FLOOD_WAIT, and exposes the latest active wait
 * with a live-refreshing countdown. Clears once the countdown reaches zero.
 */
export function useFloodWait() {
    const [state, setState] = useState<FloodWaitState | null>(null);
    const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

    useEffect(() => {
        let unlisten: UnlistenFn | undefined;
        listen<FloodWaitEvent>('flood-wait', (event) => {
            setState({
                waitSeconds: event.payload.wait_seconds,
                startedAt: Date.now(),
                attempt: event.payload.attempt,
                maxAttempts: event.payload.max_attempts,
            });
        }).then(fn => { unlisten = fn; });
        return () => { unlisten?.(); };
    }, []);

    // Drive a 1s tick so the countdown text stays live, and clear when done.
    useEffect(() => {
        if (!state) {
            if (timerRef.current) { clearInterval(timerRef.current); timerRef.current = null; }
            return;
        }
        timerRef.current = setInterval(() => {
            setState(prev => {
                if (!prev) return null;
                const elapsed = Math.floor((Date.now() - prev.startedAt) / 1000);
                if (elapsed >= prev.waitSeconds) return null;
                return prev;
            });
        }, 1000);
        return () => {
            if (timerRef.current) { clearInterval(timerRef.current); timerRef.current = null; }
        };
    }, [state?.startedAt]);

    if (!state) return null;

    const elapsedSec = Math.floor((Date.now() - state.startedAt) / 1000);
    const remainingSec = Math.max(0, state.waitSeconds - elapsedSec);
    if (remainingSec <= 0) return null;

    return {
        remainingSec,
        waitSeconds: state.waitSeconds,
        attempt: state.attempt,
        maxAttempts: state.maxAttempts,
    };
}

/** Format a duration in seconds as a short human countdown (e.g. "2m 5s", "47s"). */
export function formatCountdown(totalSec: number): string {
    if (totalSec < 60) return `${totalSec}s`;
    const m = Math.floor(totalSec / 60);
    const s = totalSec % 60;
    return s > 0 ? `${m}m ${s}s` : `${m}m`;
}
