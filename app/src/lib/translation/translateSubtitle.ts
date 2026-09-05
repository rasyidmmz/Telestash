import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { parseSrt, writeSrt } from './srtParser';
import { getLanguageLabel } from '../../utils/subtitleMatcher';

const BATCH_SIZE = 24;

interface TranslationModelStatus {
    installed: boolean;
    wasm_ready: boolean;
    files: { name: string; ready: boolean }[];
}

interface TranslationAssets {
    wasm_js: number[];
    wasm_bin: number[];
    model: number[];
    lex: number[];
    vocab: number[];
}

export interface TranslateProgress {
    phase: 'downloading' | 'translating';
    file?: string;
    done: number;
    total: number;
    cueDone?: number;
    cueTotal?: number;
}

let worker: Worker | null = null;
let workerReady: Promise<void> | null = null;

function spawnWorker(): Worker {
    const w = new Worker('/bergamot/translate-worker.js');
    return w;
}

async function ensureEngine(onProgress: (p: TranslateProgress) => void): Promise<Worker> {
    if (worker && workerReady) return worker;

    const status: TranslationModelStatus = await invoke('cmd_get_translation_model_status');
    if (!status.installed) {
        onProgress({ phase: 'downloading', file: 'engine + model pack', done: 0, total: 0 });
        const unlisten = await listen<{ file: string; downloaded: number; total: number }>(
            'translation-model-progress',
            (event) => {
                onProgress({
                    phase: 'downloading',
                    file: event.payload.file,
                    done: event.payload.downloaded,
                    total: event.payload.total,
                });
            },
        );
        try {
            await invoke('cmd_download_translation_model');
        } finally {
            unlisten();
        }
    }

    const assets: TranslationAssets = await invoke('cmd_read_translation_assets');
    worker = spawnWorker();
    workerReady = new Promise<void>((resolve, reject) => {
        const onMessage = (e: MessageEvent) => {
            const d = e.data;
            if (d.type === 'ready') {
                worker!.removeEventListener('message', onMessage);
                worker!.addEventListener('message', onTranslateMessage);
                resolve();
            } else if (d.type === 'error') {
                worker!.terminate();
                worker = null;
                workerReady = null;
                reject(new Error(d.error));
            }
        };
        worker!.addEventListener('message', onMessage);
        const transfer = [
            new Uint8Array(assets.wasm_bin).buffer,
            new Uint8Array(assets.model).buffer,
            new Uint8Array(assets.lex).buffer,
            new Uint8Array(assets.vocab).buffer,
        ];
        worker!.postMessage(
            { type: 'init', wasmBinary: new Uint8Array(assets.wasm_bin).buffer, model: new Uint8Array(assets.model).buffer, lex: new Uint8Array(assets.lex).buffer, vocab: new Uint8Array(assets.vocab).buffer },
            transfer,
        );
    });
    return worker;
}

// Result routing for in-flight batches
const pending = new Map<string, (r: string[]) => void>();
const pendingErrors = new Map<string, (e: string) => void>();

function onTranslateMessage(e: MessageEvent) {
    const d = e.data;
    if (d.type === 'result' && pending.has(d.id)) {
        pending.get(d.id)!(d.results);
        pending.delete(d.id);
        pendingErrors.delete(d.id);
    } else if (d.type === 'error' && d.id && pendingErrors.has(d.id)) {
        pendingErrors.get(d.id)!(d.error);
        pending.delete(d.id);
        pendingErrors.delete(d.id);
    }
}

let cancelled = false;

export function cancelTranslation(): void {
    cancelled = true;
}

/** Translate a batch of texts through the worker. */
function translateBatch(worker_: Worker, texts: string[]): Promise<string[]> {
    return new Promise((resolve, reject) => {
        const id = Math.random().toString(36).slice(2);
        pending.set(id, resolve);
        pendingErrors.set(id, reject);
        worker_.postMessage({ type: 'translate', id, texts });
    });
}

export interface TranslateSubtitleResult {
    srtPath: string;
    cueCount: number;
}

/**
 * Full pipeline: source subtitle (by language) -> Bergamot en→id -> .id.srt
 * temp file. Upload/cache/registration is done by cmd_attach_video_subtitles
 * from the caller.
 */
export async function translateSubtitleToIndonesian(
    file: { id: number; folder_id?: number | null; name: string },
    onProgress: (p: TranslateProgress) => void,
): Promise<TranslateSubtitleResult> {
    cancelled = false;

    // 1. Source subtitle content (English preferred)
    let sourceBytes: number[] | null = null;
    try {
        sourceBytes = await invoke<number[]>('cmd_get_subtitle_content', {
            folderId: file.folder_id ?? null,
            videoMessageId: file.id,
            language: 'en',
        });
    } catch {
        // fall through — caller surfaces the error
    }
    if (!sourceBytes || sourceBytes.length === 0) {
        throw new Error('NO_SOURCE_SUBTITLE');
    }

    // 2. Parse
    const decoder = new TextDecoder('utf-8');
    const sourceText = decoder.decode(new Uint8Array(sourceBytes));
    const cues = parseSrt(sourceText);
    if (cues.length === 0) {
        throw new Error('SOURCE_SRT_EMPTY');
    }

    // 3. Engine + translate in batches
    await ensureEngine(onProgress);
    const texts = cues.map((c) => c.lines.join('\n'));
    const translations: string[] = [];
    let done = 0;

    for (let i = 0; i < texts.length; i += BATCH_SIZE) {
        if (cancelled) throw new Error('CANCELLED');
        const batch = texts.slice(i, i + BATCH_SIZE);
        const batchResults = await translateBatch(worker!, batch);
        translations.push(...batchResults);
        done += batch.length;
        onProgress({ phase: 'translating', done, total: texts.length, cueDone: done, cueTotal: texts.length });
    }

    // 4. Assemble .id.srt
    const outCues = cues.map((cue, i) => ({ ...cue, lines: (translations[i] ?? cue.lines.join('\n')).split('\n') }));
    const srtContent = writeSrt(outCues);
    const bytes = Array.from(new TextEncoder().encode(srtContent));
    const srtPath = await invoke<string>('cmd_write_temp_srt', {
        folderId: file.folder_id ?? null,
        videoMessageId: file.id,
        bytes,
    });

    // 5. Attach: upload + caption + cache + register (existing pipeline)
    await invoke('cmd_attach_video_subtitles', {
        folderId: file.folder_id ?? null,
        videoMessageId: file.id,
        videoFileName: file.name,
        primaryPath: srtPath,
        pairedPath: null,
        format: 'srt',
        language: 'id',
        label: `Indonesian (translated from ${getLanguageLabel('en')})`,
    });

    return { srtPath, cueCount: cues.length };
}
