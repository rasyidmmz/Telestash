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
