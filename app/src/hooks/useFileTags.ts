import { useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';

export interface FileTag {
    id: number;
    name: string;
}

export function useTags() {
    return useQuery({
        queryKey: ['file-tags'],
        queryFn: () => invoke<FileTag[]>('cmd_list_tags'),
    });
}

export function useFolderTagMap(folderId: number | null) {
    return useQuery({
        queryKey: ['folder-tag-map', folderId],
        queryFn: () => invoke<Record<string, string[]>>('cmd_get_folder_tag_map', { folderId }),
    });
}

export function useTagMutations() {
    const queryClient = useQueryClient();
    const invalidate = useCallback(async () => {
        await queryClient.invalidateQueries({ queryKey: ['file-tags'] });
        await queryClient.invalidateQueries({ queryKey: ['folder-tag-map'] });
    }, [queryClient]);

    const createTag = useCallback(
        async (name: string) => {
            const tag = await invoke<FileTag>('cmd_create_tag', { name });
            await invalidate();
            return tag;
        },
        [invalidate],
    );

    const deleteTag = useCallback(
        async (tagId: number) => {
            await invoke('cmd_delete_tag', { tagId });
            await invalidate();
        },
        [invalidate],
    );

    const setFileTags = useCallback(
        async (folderId: number | null, messageId: number, tagIds: number[]) => {
            await invoke('cmd_set_file_tags', { folderId, messageId, tagIds });
            await invalidate();
        },
        [invalidate],
    );

    const getFileTagIds = useCallback(async (folderId: number | null, messageId: number) => {
        return invoke<number[]>('cmd_get_file_tag_ids', { folderId, messageId });
    }, []);

    return { createTag, deleteTag, setFileTags, getFileTagIds };
}
