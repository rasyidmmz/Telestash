// Shared test setup: jsdom plus stubs for the Tauri APIs that app code imports
// at module load. Without these, importing a hook throws before any assertion
// runs because `@tauri-apps/api/core` is only functional inside the webview.

import { vi } from 'vitest';

// Tauri IPC. Tests can override per-case with `vi.mocked(invoke)`.
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn(async () => () => {}),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn(async () => null),
    save: vi.fn(async () => null),
}));

vi.mock('@tauri-apps/plugin-store', () => ({
    load: vi.fn(async () => null),
}));

vi.mock('@tauri-apps/plugin-shell', () => ({
    open: vi.fn(async () => {}),
}));

// sonner renders toasts; the tests only care that the call happened.
vi.mock('sonner', () => ({
    toast: {
        success: vi.fn(),
        error: vi.fn(),
        info: vi.fn(),
        warning: vi.fn(),
    },
}));

// App code (Sidebar width persistence, watch history, themes, backups) reads
// localStorage at import or render time. CI workers can boot without the
// jsdom-provided global, so back it with an in-memory Storage when absent.
if (typeof globalThis.localStorage === 'undefined') {
    const store = new Map<string, string>();
    const storage: Storage = {
        get length() {
            return store.size;
        },
        clear: () => store.clear(),
        getItem: (key) => (store.has(key) ? (store.get(key) as string) : null),
        key: (index) => Array.from(store.keys())[index] ?? null,
        removeItem: (key) => {
            store.delete(key);
        },
        setItem: (key, value) => {
            store.set(key, String(value));
        },
    };
    Object.defineProperty(globalThis, 'localStorage', {
        value: storage,
        configurable: true,
    });
}
