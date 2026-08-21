import React from 'react';
import { extractMediaBadges, MediaBadge } from '../../utils/mediaTags';

interface MediaBadgesListProps {
    filename: string;
    maxBadges?: number;
    className?: string;
}

const variantStyles: Record<MediaBadge['variant'], string> = {
    cyan: 'bg-cyan-950/80 text-cyan-400 border-cyan-500/30',
    emerald: 'bg-emerald-950/80 text-emerald-400 border-emerald-500/30',
    amber: 'bg-amber-950/80 text-amber-400 border-amber-500/30',
    purple: 'bg-purple-950/80 text-purple-400 border-purple-500/30',
    slate: 'bg-slate-900/90 text-slate-300 border-slate-700/60',
};

export const MediaBadgesList: React.FC<MediaBadgesListProps> = ({ filename, maxBadges = 4, className = '' }) => {
    const badges = extractMediaBadges(filename);
    if (badges.length === 0) return null;

    const visibleBadges = badges.slice(0, maxBadges);

    return (
        <div className={`flex flex-wrap items-center gap-1 ${className}`}>
            {visibleBadges.map((badge, idx) => (
                <span
                    key={`${badge.type}-${idx}`}
                    className={`inline-flex items-center text-[9px] font-mono font-bold tracking-tight px-1.5 py-0.5 rounded border ${variantStyles[badge.variant]}`}
                >
                    {badge.label}
                </span>
            ))}
        </div>
    );
};
