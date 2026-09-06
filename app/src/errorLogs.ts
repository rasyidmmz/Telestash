import { useEffect, useState } from 'react';

export interface ErrorLogEntry {
    id: string;
    time: string;
    source: string;
    category?: string;
    message: string;
    details?: string;
    level?: 'error' | 'info';
}

const STORAGE_KEY = 'telestash.errorLogs';
const MAX_LOGS = 100;
const listeners = new Set<(logs: ErrorLogEntry[]) => void>();
let logs = readStoredLogs();

export function recordErrorLog(input: Omit<ErrorLogEntry, 'id' | 'time'>) {
    const entry: ErrorLogEntry = {
        id: createId(),
        time: new Date().toISOString(),
        ...input,
    };
    logs = [entry, ...logs].slice(0, MAX_LOGS);
    writeStoredLogs(logs);
    emit();
}

export function clearErrorLogs() {
    logs = [];
    writeStoredLogs(logs);
    emit();
}

export function useErrorLogs() {
    const [items, setItems] = useState(logs);

    useEffect(() => {
        listeners.add(setItems);
        setItems(logs);
        return () => {
            listeners.delete(setItems);
        };
    }, []);

    return items;
}

export function formatLogValue(value: unknown): string {
    if (value instanceof Error) {
        return [value.name, value.message, value.stack].filter(Boolean).join('\n');
    }
    if (typeof value === 'string') return value;
    try {
        return JSON.stringify(value);
    } catch {
        return String(value);
    }
}

function emit() {
    for (const listener of listeners) {
        listener(logs);
    }
}

function readStoredLogs(): ErrorLogEntry[] {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) return [];
        const parsed = JSON.parse(raw);
        return Array.isArray(parsed) ? parsed.slice(0, MAX_LOGS) : [];
    } catch {
        return [];
    }
}

function writeStoredLogs(items: ErrorLogEntry[]) {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
    } catch {
        // Storage failure should not break the app.
    }
}

function createId() {
    if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
        return crypto.randomUUID();
    }
    return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
