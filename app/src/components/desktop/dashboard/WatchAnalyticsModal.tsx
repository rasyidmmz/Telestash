import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X, History, Sparkles, Clock, Flame } from '../../shared/icons.tsx';
import { useTranslation } from 'react-i18next';

interface WatchAnalytics {
    unique_titles: number;
    total_watch_secs: number;
    activity_30d: { day: number; plays: number }[];
    top_titles: { file_name: string; folder_id: number | null; plays: number; last_watched: number }[];
    current_streak: number;
}

interface WatchAnalyticsModalProps {
    onClose: () => void;
}

function formatDuration(totalSecs: number, t: (k: string, o?: Record<string, unknown>) => string): string {
    if (totalSecs < 60) return `${Math.round(totalSecs)}s`;
    const mins = Math.round(totalSecs / 60);
    if (mins < 60) return t('analytics.mins', { n: mins });
    const hours = Math.floor(mins / 60);
    const rem = mins % 60;
    return rem > 0 ? t('analytics.hours_mins', { h: hours, m: rem }) : t('analytics.hours', { n: hours });
}

/**
 * Watch analytics dashboard over the SQLite watch_history table:
 * summary cards, a 30-day activity bar strip (pure CSS bars, no chart lib),
 * and the top-10 most-played titles. Skeleton matches WatchLogsModal.
 */
export function WatchAnalyticsModal({ onClose }: WatchAnalyticsModalProps) {
    const { t } = useTranslation();
    const [data, setData] = useState<WatchAnalytics | null>(null);
    const [error, setError] = useState(false);

    useEffect(() => {
        let cancelled = false;
        invoke<WatchAnalytics>('cmd_watch_analytics')
            .then((d) => { if (!cancelled) setData(d); })
            .catch(() => { if (!cancelled) setError(true); });
        return () => { cancelled = true; };
    }, []);

    const maxPlays = data ? Math.max(1, ...data.activity_30d.map((d) => d.plays)) : 1;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4" onClick={onClose}>
            <div
                className="bg-stash-surface border border-stash-border rounded-xl w-full max-w-3xl h-[620px] flex flex-col overflow-hidden shadow-2xl"
                role="dialog"
                aria-modal="true"
                aria-label={t('analytics.title')}
                onClick={(e) => e.stopPropagation()}
            >
                {/* Header */}
                <div className="flex items-center justify-between px-5 py-4 border-b border-stash-border">
                    <div className="flex items-center gap-2">
                        <History className="w-5 h-5 text-stash-primary" />
                        <h2 className="text-base font-semibold text-stash-text">{t('analytics.title')}</h2>
                    </div>
                    <button onClick={onClose} className="p-1.5 rounded-lg hover:bg-stash-hover text-stash-subtext hover:text-stash-text transition" aria-label={t('common.close')}>
                        <X className="w-4 h-4" />
                    </button>
                </div>

                {/* Body */}
                <div className="flex-1 overflow-y-auto p-5 space-y-5">
                    {error && (
                        <div className="text-sm text-stash-subtext text-center py-10">{t('analytics.load_failed')}</div>
                    )}
                    {!error && !data && (
                        <div className="grid grid-cols-3 gap-3" aria-busy="true" aria-label={t('analytics.title')}>
                            {Array.from({ length: 3 }).map((_, i) => (
                                <div key={i} className="h-20 rounded-lg bg-stash-bg animate-pulse" />
                            ))}
                            <div className="col-span-3 h-40 rounded-lg bg-stash-bg animate-pulse" />
                        </div>
                    )}
                    {data && (
                        <>
                            {/* Summary cards */}
                            <div className="grid grid-cols-3 gap-3">
                                <div className="p-3.5 rounded-lg bg-stash-bg border border-stash-border">
                                    <div className="flex items-center gap-1.5 text-stash-subtext text-xs">
                                        <Sparkles className="w-3.5 h-3.5" />
                                        {t('analytics.unique_titles')}
                                    </div>
                                    <p className="text-2xl font-semibold text-stash-text mt-1.5 font-mono tabular-nums">{data.unique_titles}</p>
                                </div>
                                <div className="p-3.5 rounded-lg bg-stash-bg border border-stash-border">
                                    <div className="flex items-center gap-1.5 text-stash-subtext text-xs">
                                        <Clock className="w-3.5 h-3.5" />
                                        {t('analytics.total_watch')}
                                    </div>
                                    <p className="text-2xl font-semibold text-stash-text mt-1.5 font-mono tabular-nums">
                                        {formatDuration(data.total_watch_secs, t)}
                                    </p>
                                </div>
                                <div className="p-3.5 rounded-lg bg-stash-bg border border-stash-border">
                                    <div className="flex items-center gap-1.5 text-stash-subtext text-xs">
                                        <Flame className="w-3.5 h-3.5" />
                                        {t('analytics.streak')}
                                    </div>
                                    <p className="text-2xl font-semibold text-stash-text mt-1.5 font-mono tabular-nums">
                                        {t('analytics.streak_days', { n: data.current_streak })}
                                    </p>
                                </div>
                            </div>

                            {/* 30-day activity */}
                            <div>
                                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider mb-2.5">{t('analytics.activity_30d')}</h3>
                                <div className="flex items-end gap-1 h-28">
                                    {data.activity_30d.map((d) => (
                                        <div
                                            key={d.day}
                                            className="flex-1 rounded-t-sm bg-stash-primary/70 hover:bg-stash-primary transition-colors min-h-[2px]"
                                            style={{ height: `${Math.max(2, (d.plays / maxPlays) * 100)}%` }}
                                            title={`${d.plays}`}
                                        />
                                    ))}
                                </div>
                            </div>

                            {/* Top titles */}
                            <div>
                                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider mb-2.5">{t('analytics.top_titles')}</h3>
                                {data.top_titles.length === 0 ? (
                                    <div className="text-sm text-stash-subtext py-6 text-center">{t('analytics.empty')}</div>
                                ) : (
                                    <ol className="space-y-1.5">
                                        {data.top_titles.map((title, i) => (
                                            <li key={`${title.file_name}-${title.folder_id}`} className="flex items-center gap-3 px-3 py-2 rounded-lg bg-stash-bg border border-stash-border">
                                                <span className="text-xs font-mono text-stash-subtext w-5 text-center shrink-0">{i + 1}</span>
                                                <span className="text-sm text-stash-text truncate flex-1" title={title.file_name}>{title.file_name}</span>
                                                <span className="text-xs font-mono text-stash-subtext shrink-0">{t('analytics.plays', { n: title.plays })}</span>
                                            </li>
                                        ))}
                                    </ol>
                                )}
                            </div>
                        </>
                    )}
                </div>
            </div>
        </div>
    );
}
