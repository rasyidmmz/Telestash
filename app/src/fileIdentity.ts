import type { TelegramFile } from './types';

export function fileKey(file: Pick<TelegramFile, 'id' | 'folder_id'>): string {
    return `${file.folder_id ?? 'home'}:${file.id}`;
}

export function selectedFiles(files: TelegramFile[], keys: string[]): TelegramFile[] {
    const selected = new Set(keys);
    return files.filter(file => selected.has(fileKey(file)));
}

export function groupFilesByFolder(files: TelegramFile[]): Map<number | null, TelegramFile[]> {
    const groups = new Map<number | null, TelegramFile[]>();
    for (const file of files) {
        const folder = file.folder_id ?? null;
        const group = groups.get(folder) ?? [];
        group.push(file);
        groups.set(folder, group);
    }
    return groups;
}
