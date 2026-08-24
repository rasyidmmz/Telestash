import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { VideoSubtitleInfo } from '../types';
import { isVideoFile } from '../utils';

export function useVideoSubtitles(
    messageId: number,
    folderId: number | null,
    filename: string
) {
    const isVideo = isVideoFile(filename);

    return useQuery<VideoSubtitleInfo[]>({
        queryKey: ['video-subtitles', folderId, messageId],
        queryFn: async () => {
            if (!isVideo) return [];
            return await invoke<VideoSubtitleInfo[]>('cmd_get_video_subtitles', {
                folderId,
                videoMessageId: messageId,
            });
        },
        enabled: isVideo && messageId > 0,
        staleTime: 60 * 1000, // 1 minute
    });
}
