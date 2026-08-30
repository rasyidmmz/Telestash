import { motion, AnimatePresence } from 'framer-motion';
import { Download, X, RefreshCw, Sparkles } from 'lucide-react';

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
                    className="fixed top-0 left-0 right-0 z-50 p-3 bg-gradient-to-r from-stash-primary/90 via-blue-500/90 to-purple-500/90 backdrop-blur-sm shadow-lg"
                >
                    <div className="flex items-center justify-center gap-4 max-w-screen-lg mx-auto">
                        <Sparkles className="w-5 h-5 text-yellow-300 animate-pulse" />

                        <span className="text-white font-medium">
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
                                <RefreshCw className="w-4 h-4 text-white animate-spin" />
                                <div className="w-32 h-2 bg-white/30 rounded-full overflow-hidden">
                                    <motion.div
                                        className="h-full bg-white rounded-full"
                                        initial={{ width: 0 }}
                                        animate={{ width: `${progress}%` }}
                                    />
                                </div>
                            </div>
                        ) : (
                            <button
                                onClick={onUpdate}
                                className="flex items-center gap-2 px-4 py-1.5 bg-white text-stash-primary font-semibold rounded-full hover:bg-white/90 transition-colors shadow-md"
                            >
                                <Download className="w-4 h-4" />
                                Update Now
                            </button>
                        )}

                        {!busy && (
                            <button
                                onClick={onDismiss}
                                className="p-1 text-white/70 hover:text-white transition-colors"
                                title="Dismiss"
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
