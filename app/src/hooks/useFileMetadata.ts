import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useSettings } from '../context/SettingsContext';
import { useTranslation } from 'react-i18next';
import { parseMediaTitle } from '../utils/mediaTitle';
import { isVideoFile } from './useVideoThumbnail';

export interface FileMetadata {
    folder_id: number | null;
    message_id: number;
    media_type: string;
    tmdb_id: number | null;
    title: string;
    original_title?: string | null;
    year?: number | null;
    overview?: string | null;
    rating?: number | null;
    genres_json?: string | null;
    poster_path?: string | null;
}

interface TmdbSearchResponse {
    results?: Array<{
        id: number;
        title?: string;
        name?: string;
        original_title?: string;
        original_name?: string;
        release_date?: string;
        first_air_date?: string;
        overview?: string;
        vote_average?: number;
        genre_ids?: number[];
    }>;
}

const TMDB_GENRES: Record<number, string> = {
    28: 'Action', 12: 'Adventure', 16: 'Animation', 35: 'Comedy', 80: 'Crime',
    99: 'Documentary', 18: 'Drama', 10751: 'Family', 14: 'Fantasy', 36: 'History',
    27: 'Horror', 10402: 'Music', 9648: 'Mystery', 10749: 'Romance',
    878: 'Science Fiction', 10770: 'TV Movie', 53: 'Thriller', 10752: 'War', 37: 'Western',
    10759: 'Action & Adventure', 10762: 'Kids', 10763: 'News', 10764: 'Reality',
    10765: 'Sci-Fi & Fantasy', 10766: 'Soap', 10767: 'Talk', 10768: 'War & Politics',
};

function languageTag(lang: string): string {
    const map: Record<string, string> = {
        id: 'id-ID', en: 'en-US', ar: 'ar-SA', de: 'de-DE', es: 'es-ES',
        fr: 'fr-FR', hi: 'hi-IN', ja: 'ja-JP', ko: 'ko-KR', 'pt-BR': 'pt-BR',
        ru: 'ru-RU', tr: 'tr-TR', 'zh-CN': 'zh-CN',
    };
    return map[lang] || 'en-US';
}

/**
 * Opt-in TMDB metadata for one video file. Fires only when the feature is
 * enabled, the user configured their own API key, the file is a video, and the
 * card is visible on screen (IntersectionObserver, like useVideoThumbnail).
 * Results are persisted to the backend `file_metadata` table so they never
 * need a second lookup.
 */
export function useFileMetadata(
    fileId: number,
    fileName: string,
    folderId: number | null | undefined,
    visible: boolean,
) {
    const { settings, updateSetting } = useSettings();
    const { t, i18n } = useTranslation();
    const queryClient = useQueryClient();
    const requestedRef = useRef(false);
    const [unavailable, setUnavailable] = useState(false);

    const enabled =
        settings.tmdbEnabled === true &&
        typeof settings.tmdbApiKey === 'string' &&
        settings.tmdbApiKey.length >= 8 &&
        isVideoFile(fileName);

    const cached = useQuery({
        queryKey: ['file-metadata', folderId, fileId],
        queryFn: () =>
            invoke<FileMetadata | null>('cmd_get_file_metadata', {
                messageId: fileId,
                folderId: folderId ?? null,
            }),
        enabled,
        staleTime: 24 * 60 * 60 * 1000,
        retry: 1,
    });

    useEffect(() => {
        if (!enabled || !visible || unavailable || requestedRef.current) return;
        if (document.hidden) return;
        if (cached.data !== undefined && cached.data !== null) return;
        requestedRef.current = true;

        const info = parseMediaTitle(fileName);
        const apiKey = settings.tmdbApiKey as string;
        const lang = languageTag(i18n.language);
        const endpoint = info.kind === 'tv'
            ? 'https://api.themoviedb.org/3/search/tv'
            : 'https://api.themoviedb.org/3/search/movie';
        const url = new URL(endpoint);
        url.searchParams.set('api_key', apiKey);
        url.searchParams.set('query', info.title);
        url.searchParams.set('language', lang);
        url.searchParams.set('page', '1');
        if (info.year && info.kind === 'movie') url.searchParams.set('year', String(info.year));
        if (info.year && info.kind === 'tv') url.searchParams.set('first_air_date_year', String(info.year));

        (async () => {
            try {
                const res = await fetch(url.toString());
                if (!res.ok) {
                    // 401 = invalid key; disable further attempts for the session.
                    if (res.status === 401) setUnavailable(true);
                    return;
                }
                const data: TmdbSearchResponse = await res.json();
                const best = data.results?.[0];
                if (!best) return;

                const title = best.title || best.name || info.title;
                const date = best.release_date || best.first_air_date || '';
                // Fetch + cache the poster locally first so the row records
                // the TMDB file path it can be re-fetched from.
                const posterFilePath = best.id
                    ? await fetchAndCachePoster(apiKey, best.id, info.kind, fileId, folderId ?? null)
                    : null;
                const row: FileMetadata = {
                    folder_id: folderId ?? null,
                    message_id: fileId,
                    media_type: info.kind,
                    tmdb_id: best.id,
                    title,
                    original_title: best.original_title || best.original_name || null,
                    year: date ? parseInt(date.slice(0, 4), 10) : info.year ?? null,
                    overview: best.overview || null,
                    rating: typeof best.vote_average === 'number' && best.vote_average > 0 ? best.vote_average : null,
                    genres_json: JSON.stringify((best.genre_ids || []).map((g) => TMDB_GENRES[g] || String(g))),
                    poster_path: posterFilePath,
                };
                await invoke('cmd_upsert_file_metadata', { entry: row });
                queryClient.invalidateQueries({ queryKey: ['file-metadata', folderId, fileId] });
            } catch {
                // Network unavailable or offline: leave silent, icon stays.
            }
        })();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [enabled, visible, fileId, fileName, folderId, cached.data, unavailable]);

    return {
        metadata: cached.data ?? null,
        isLoading: enabled && visible && cached.isLoading,
        t,
        updateSetting,
    };
}

async function fetchAndCachePoster(
    apiKey: string,
    tmdbId: number,
    kind: 'movie' | 'tv',
    fileId: number,
    folderId: number | null,
): Promise<string | null> {
    const path = kind === 'tv'
        ? `https://api.themoviedb.org/3/tv/${tmdbId}/images`
        : `https://api.themoviedb.org/3/movie/${tmdbId}/images`;
    const url = new URL(path);
    url.searchParams.set('api_key', apiKey);
    const res = await fetch(url.toString());
    if (!res.ok) return null;
    const data = await res.json();
    const filePath: string | undefined = (data.posters || [])[0]?.file_path;
    if (!filePath) return null;
    const imgRes = await fetch(`https://image.tmdb.org/t/p/w342${filePath}`);
    if (!imgRes.ok) return null;
    const blob = await imgRes.blob();
    if (!blob.type.includes('jpeg') && !blob.type.includes('jpg')) return null;
    const buf = new Uint8Array(await blob.arrayBuffer());
    let binary = '';
    for (let i = 0; i < buf.length; i++) binary += String.fromCharCode(buf[i]);
    await invoke('cmd_save_tmdb_poster', {
        messageId: fileId,
        folderId,
        base64Jpeg: btoa(binary),
    });
    return filePath;
}

/** Serve a locally cached poster as a data URL (empty string when absent). */
export function useCachedPoster(fileId: number, folderId: number | null | undefined, enabled: boolean) {
    const [poster, setPoster] = useState<string | null>(null);
    useEffect(() => {
        if (!enabled) return;
        let cancelled = false;
        invoke<string>('cmd_get_tmdb_poster', { messageId: fileId, folderId: folderId ?? null })
            .then((p) => {
                if (!cancelled && p) setPoster(p);
            })
            .catch(() => { /* no poster cached */ });
        return () => { cancelled = true; };
    }, [enabled, fileId, folderId]);
    return poster;
}
