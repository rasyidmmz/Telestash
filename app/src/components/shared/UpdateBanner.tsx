import { motion, AnimatePresence } from 'framer-motion';
import { Download, X, RefreshCw, Sparkles } from './icons.tsx';

interface UpdateBannerProps {
    available: boolean;
    version: string | null;
    downloading: boolean;
    installing: boolean;
    restarting: boolean;
    progress: number;
    error: string | null;
    onUpdate: () => void;
    onDismiss: () => void;
}

export function UpdateBanner({
    available,
    version,
    downloading,
    installing,
    restarting,
    progress,
    error,
    onUpdate,
    onDismiss
}: UpdateBannerProps) {
    const busy = downloading || installing || restarting;
    return (
        <AnimatePresence>
            {available && (
                <motion.div
                    initial={{ opacity: 0, y: -50 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0, y: -50 }}
                    className="fixed top-0 left-0 right-0 z-50 p-3 bg-stash-surface border-b border-stash-border shadow-lg"
                >
                    <div className="flex items-center justify-center gap-4 max-w-screen-lg mx-auto">
                        <Sparkles className="w-5 h-5 text-stash-primary shrink-0" />

                        <span className="text-stash-text font-medium" role="status">
                            {restarting ? (
                                <>Restarting TeleStash...</>
                            ) : installing ? (
                                <>Installing update...</>
                            ) : downloading ? (
                                <>Downloading update... {progress}%</>
                            ) : error ? (
                                <>Update failed: {error}</>
                            ) : (
                                <>A new version ({version}) is available!</>
                            )}
                        </span>

                        {busy ? (
                            <div className="flex items-center gap-2">
                                <RefreshCw className="w-4 h-4 text-stash-primary animate-spin" />
                                <div className="w-32 h-2 bg-stash-hover rounded-full overflow-hidden">
                                    <motion.div
                                        className="h-full bg-stash-primary rounded-full"
                                        initial={{ width: 0 }}
                                        animate={{ width: `${progress}%` }}
                                    />
                                </div>
                            </div>
                        ) : (
                            <button
                                onClick={onUpdate}
                                className="auth-primary flex items-center gap-2 px-4 py-1.5 font-semibold rounded-lg transition-colors"
                            >
                                <Download className="w-4 h-4" />
                                Update Now
                            </button>
                        )}

                        {!busy && (
                            <button
                                onClick={onDismiss}
                                className="p-1 text-stash-subtext hover:text-stash-text transition-colors"
                                title="Dismiss"
                                aria-label="Dismiss update"
                            >
                                <X className="w-4 h-4" />
                            </button>
                        )}
                    </div>
                </motion.div>
            )}
        </AnimatePresence>
    );
}
