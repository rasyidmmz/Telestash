import { useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';

export function favoriteKey(folderId: number | null, messageId: number): string {
    return `${folderId === null ? 'home' : folderId}:${messageId}`;
}

export function useFolderFavorites(folderId: number | null) {
    return useQuery({
        queryKey: ['folder-favorites', folderId],
        queryFn: () => invoke<number[]>('cmd_list_folder_favorites', { folderId }),
    });
}

export function useFavoriteSet(folderId: number | null): Set<string> {
    const { data } = useFolderFavorites(folderId);
    return new Set((data ?? []).map((id) => favoriteKey(folderId, id)));
}

export function useToggleFavorite() {
    const queryClient = useQueryClient();
    return useCallback(
        async (folderId: number | null, messageId: number) => {
            const next = await invoke<boolean>('cmd_toggle_file_favorite', {
                folderId,
                messageId,
            });
            await queryClient.invalidateQueries({ queryKey: ['folder-favorites', folderId] });
            await queryClient.invalidateQueries({ queryKey: ['folder-favorites'] });
            await queryClient.invalidateQueries({ queryKey: ['all-favorite-files'] });
            return next;
        },
        [queryClient],
    );
}

export function useAllFavoriteFiles(enabled: boolean) {
    return useQuery({
        queryKey: ['all-favorite-files'],
        enabled,
        queryFn: () => invoke('cmd_get_all_favorite_files'),
    });
}
