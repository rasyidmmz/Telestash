import { createContext, useCallback, useContext, useMemo, useState, ReactNode } from 'react';
import { useSettings } from './SettingsContext';
import { useUpdateCheck } from '../hooks/useUpdateCheck';

// How often the app looks for new releases while it stays open. The banner
// is driven by the same shared state, so a new release shows up live without
// an app restart.
const UPDATE_CHECK_INTERVAL_MS = 30 * 60 * 1000;

// "Remind me later" hides the banner for this long. The manual install button
// in Settings stays available even while snoozed.
const UPDATE_SNOOZE_MS = 24 * 60 * 60 * 1000;
const UPDATE_SNOOZE_KEY = 'telestash_update_snooze_until';

interface UpdateContextType extends ReturnType<typeof useUpdateCheck> {
    /** `available` minus the snooze window; drives the top banner only. */
    bannerAvailable: boolean;
    remindLater: () => void;
}

const UpdateContext = createContext<UpdateContextType | undefined>(undefined);

function readSnooze(): number {
    try {
        const raw = localStorage.getItem(UPDATE_SNOOZE_KEY);
        const parsed = raw ? parseInt(raw, 10) : NaN;
        return Number.isFinite(parsed) ? parsed : 0;
    } catch {
        return 0;
    }
}

export function UpdateProvider({ children }: { children: ReactNode }) {
    const { settings, isLoaded } = useSettings();
    const update = useUpdateCheck({
        autoCheck: isLoaded && settings.autoUpdate,
        intervalMs: UPDATE_CHECK_INTERVAL_MS,
    });
    const [snoozedUntil, setSnoozedUntil] = useState<number>(() => readSnooze());

    const remindLater = useCallback(() => {
        const until = Date.now() + UPDATE_SNOOZE_MS;
        setSnoozedUntil(until);
        try {
            localStorage.setItem(UPDATE_SNOOZE_KEY, String(until));
        } catch {
            // Persistence is best-effort; the snooze still holds for this session.
        }
    }, []);

    const value = useMemo<UpdateContextType>(() => ({
        ...update,
        bannerAvailable: update.available && Date.now() >= snoozedUntil,
        remindLater,
    }), [update, snoozedUntil, remindLater]);

    return (
        <UpdateContext.Provider value={value}>
            {children}
        </UpdateContext.Provider>
    );
}

export const useUpdate = () => {
    const context = useContext(UpdateContext);
    if (!context) throw new Error('useUpdate must be used within an UpdateProvider');
    return context;
};
