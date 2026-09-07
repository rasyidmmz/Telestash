import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useSettings } from '../context/SettingsContext';

const VIDEO_EXTS = [
    'mp4', 'mkv', 'webm', 'mov', 'avi', 'ts', 'm2ts', 'mts', 'm4v', 'mpg', 'mpeg', 'flv', 'wmv',
];

export function isVideoFile(filename: string): boolean {
    const ext = filename.split('.').pop()?.toLowerCase() || '';
    return VIDEO_EXTS.includes(ext);
}

/**
 * Fallback video thumbnail: only fires when the file has no Telegram-native
 * thumbnail (cmd_get_thumbnail returned empty). Visibility-gated via
 * IntersectionObserver so off-screen cards never queue work, single-flight
 * per card, globally queued 1-at-a-time by the backend. Attach the returned
 * ref to the card's root element.
 */
export function useVideoThumbnail(
    fileId: number,
    fileName: string,
    folderId: number | null | undefined,
    telegramThumb: string | null,
): { generated: string | null; ref: (node: HTMLElement | null) => void } {
    const { settings } = useSettings();
    const enabled = settings.videoThumbnails === true;
    const [generated, setGenerated] = useState<string | null>(null);
    const [visible, setVisible] = useState(false);
    const requestedRef = useRef(false);

    const ref = (node: HTMLElement | null) => {
        if (!node) return;
        const observer = new IntersectionObserver(
            (entries) => {
                if (entries.some((e) => e.isIntersecting)) {
                    setVisible(true);
                    observer.disconnect();
                }
            },
            { rootMargin: '200px' },
        );
        observer.observe(node);
    };

    useEffect(() => {
        if (
            !enabled ||
            telegramThumb ||
            !isVideoFile(fileName) ||
            !visible ||
            requestedRef.current ||
            // Low-power rule: no generation while the app is hidden in tray.
            document.hidden
        ) {
            return;
        }
        requestedRef.current = true;

        invoke<string>('cmd_generate_video_thumbnail', {
            messageId: fileId,
            folderId: folderId ?? null,
        })
            .then((result) => {
                if (result) setGenerated(result);
            })
            .catch(() => {
                // Soft-fail: the card keeps its icon.
            });
    }, [enabled, fileId, fileName, folderId, telegramThumb, visible]);

    return { generated, ref };
}
