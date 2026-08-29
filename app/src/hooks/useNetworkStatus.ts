import { useState, useEffect } from 'react';

/**
 * Network detection for Tauri apps using lightweight backend check
 * 
 * Uses cmd_is_network_available which does a simple TCP connection test
 * to Telegram servers without using grammers (avoids stack overflow).
 * 
 * Polls every 10 seconds - very lightweight (~2ms per check).
 */
export function useNetworkStatus() {
    const [isOnline, setIsOnline] = useState(true);

    useEffect(() => {
        let interval: ReturnType<typeof setInterval> | null = null;
        let visibilityHandler: (() => void) | null = null;

        // Import Tauri invoke
        import('@tauri-apps/api/core').then(({ invoke }) => {
            // Check network status
            const checkNetwork = async () => {
                try {
                    // Use the lightweight TCP check (no grammers involved)
                    const available = await invoke<boolean>('cmd_is_network_available');
                    setIsOnline(available);
                } catch (error) {
                    // If the command fails, assume offline
                    setIsOnline(false);
                }
            };

            // While the window is hidden in the tray the webview must stay
            // idle; refresh immediately once it becomes visible again.
            const maybeCheck = () => {
                if (document.hidden) return;
                checkNetwork();
            };

            // Initial check
            checkNetwork();

            // Poll every 10 seconds (very lightweight, ~2ms per check)
            interval = setInterval(maybeCheck, 10000);
            visibilityHandler = maybeCheck;
            document.addEventListener('visibilitychange', maybeCheck);
        });

        return () => {
            if (interval) clearInterval(interval);
            if (visibilityHandler) {
                document.removeEventListener('visibilitychange', visibilityHandler);
            }
        };
    }, []);

    return isOnline;
}
