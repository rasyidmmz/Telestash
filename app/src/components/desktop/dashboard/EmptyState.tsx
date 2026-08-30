import { SearchX, Upload } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface EmptyStateProps {
    onUpload: () => void;
    searchTerm: string;
    onClearSearch: () => void;
}

export function EmptyState({ onUpload, searchTerm, onClearSearch }: EmptyStateProps) {
    const isSearchEmpty = searchTerm.trim().length > 0;
    const { t } = useTranslation();

    return (
        <div className="flex flex-col items-center justify-center py-20 px-8 text-center">
            {/* Custom SVG Illustration */}
            <svg
                className="w-48 h-48 mb-8"
                viewBox="0 0 200 200"
                fill="none"
            >
                {/* Cloud shape - Light: Blue-ish tint, Dark: Subtle overlay */}
                <ellipse cx="100" cy="120" rx="70" ry="40" className="fill-blue-100 dark:fill-stash-primary/5 opacity-50 dark:opacity-30" />

                {/* Folder base - Light: White with Blue Border, Dark: Dark Blue with Border */}
                <path
                    d="M40 80 L40 140 C40 145 44 150 50 150 L150 150 C156 150 160 145 160 140 L160 80 Z"
                    className="fill-white dark:fill-[#1e3a5f] stroke-blue-200 dark:stroke-stash-primary/30"
                    strokeWidth="1"
                />

                {/* Folder tab */}
                <path
                    d="M40 80 L40 70 C40 65 44 60 50 60 L80 60 L90 70 L90 80 Z"
                    className="fill-white dark:fill-[#1e3a5f] stroke-blue-200 dark:stroke-stash-primary/30"
                    strokeWidth="1"
                />

                {/* Plus icon in center */}
                <circle cx="100" cy="110" r="20" className="fill-blue-50 dark:fill-stash-primary/10 stroke-blue-300 dark:stroke-stash-primary/50" strokeWidth="2" strokeDasharray="4 2" />
                <path d="M100 100 L100 120 M90 110 L110 110" className="stroke-stash-primary" strokeWidth="2" strokeLinecap="round" />

                {/* Floating documents */}
                <g className="animate-pulse">
                    <rect x="130" y="50" width="25" height="30" rx="3" className="fill-blue-500" />
                    <rect x="135" y="56" width="15" height="2" rx="1" className="fill-white/80" />
                    <rect x="135" y="62" width="12" height="2" rx="1" className="fill-white/80" />
                </g>

                <g opacity="0.6">
                    <rect x="45" y="40" width="20" height="25" rx="3" className="fill-gray-300 dark:fill-gray-500" />
                    <rect x="49" y="45" width="12" height="2" rx="1" className="fill-white/80" />
                    <rect x="49" y="50" width="8" height="2" rx="1" className="fill-white/80" />
                </g>
            </svg>

            <h3 className="text-xl font-semibold text-stash-text mb-2">
                {isSearchEmpty ? t('files.search_empty_title') : t('files.folder_empty_title')}
            </h3>
            <p className="text-stash-subtext text-sm mb-6 max-w-xs">
                {isSearchEmpty
                    ? t('files.search_empty_description', { query: searchTerm })
                    : t('files.folder_empty_description')}
            </p>

            {isSearchEmpty ? (
                <button onClick={onClearSearch} className="inline-flex items-center gap-2 px-4 py-2.5 bg-stash-hover text-stash-text font-medium rounded-lg hover:bg-stash-border transition-colors">
                    <SearchX className="w-4 h-4" />
                    {t('files.clear_search')}
                </button>
            ) : (
                <button
                    onClick={onUpload}
                    className="inline-flex items-center gap-2 px-6 py-3 bg-stash-primary text-black font-medium rounded-xl hover:bg-stash-primary/90 transition-all hover:scale-105 shadow-lg shadow-stash-primary/20"
                >
                    <Upload className="w-5 h-5" />
                    {t('files.upload_files')}
                </button>
            )}

            <p className="text-xs text-stash-subtext/50 mt-6">
                {t('files.search_tip_prefix')} <kbd className="px-1.5 py-0.5 bg-stash-hover rounded text-stash-subtext">Ctrl + F</kbd> {t('files.search_tip_suffix')}
            </p>
        </div>
    );
}
