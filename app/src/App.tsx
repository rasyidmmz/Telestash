import React, { useState, useEffect, Suspense } from "react";
import { invoke } from "@tauri-apps/api/core";
import { load } from "@tauri-apps/plugin-store";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AuthWizard } from "./components/shared/AuthWizard";
import { ErrorBoundary } from "./components/shared/ErrorBoundary";
import { UpdateBanner } from "./components/shared/UpdateBanner";
import { UpdateProvider, useUpdate } from "./context/UpdateContext";
import "./App.css";

const DesktopDashboard = React.lazy(() => import("./components/desktop/DesktopDashboard").then(m => ({ default: m.Dashboard })));

import { Toaster, toast } from "sonner";
import { ConfirmProvider } from "./context/ConfirmContext";
import { ThemeProvider, useTheme } from "./context/ThemeContext";
import { SettingsProvider, useSettings } from "./context/SettingsContext";
import { useTranslation } from "react-i18next";

const queryClient = new QueryClient();

type AuthStatus = "loading" | "authenticated" | "unauthenticated";

function AppContent() {
  const [authStatus, setAuthStatus] = useState<AuthStatus>("loading");
  const { theme } = useTheme();
  const { settings, isLoaded } = useSettings();
  const { bannerAvailable: available, version, downloading, installing, restarting, progress, error: updateError, downloadAndInstall, remindLater } = useUpdate();
  const { i18n } = useTranslation();

  // Handle active language and RTL direction changes
  useEffect(() => {
    if (!isLoaded) return;
    i18n.changeLanguage(settings.language);
    document.documentElement.lang = settings.language;
    document.documentElement.dir = settings.language === 'ar' ? 'rtl' : 'ltr';
  }, [settings.language, isLoaded, i18n]);

  // Performance mode is the default: keep GPU-heavy effects disabled.
  useEffect(() => {
    document.body.classList.add('performance-mode');
  }, []);

  // On mount: check for a saved session and auto-restore it.
  // This is the SINGLE source of truth for the initial connection.
  useEffect(() => {
    const checkSession = async () => {
      try {
        const store = await load("config.json");
        const savedId = await store.get<string>("api_id");

        if (!savedId) {
          setAuthStatus("unauthenticated");
          return;
        }

        const apiId = parseInt(savedId, 10);
        if (isNaN(apiId)) {
          setAuthStatus("unauthenticated");
          return;
        }

        // Initialize the client with the saved API ID
        await invoke("cmd_connect", { apiId });

        // Verify the session is still valid with Telegram servers
        const ok = await invoke<boolean>("cmd_check_connection");
        if (ok) {
          setAuthStatus("authenticated");
        } else {
          setAuthStatus("unauthenticated");
        }
      } catch (err) {
        console.warn("Session restore failed, showing login:", err);
        toast.error(`Session restore failed: ${String(err).slice(0, 300)}`, { duration: 12000 });
        setAuthStatus("unauthenticated");
      }
    };

    let settled = false;
    setTimeout(() => {
      if (!settled) {
        toast.info("Session restore is taking unusually long — the Telegram connection may be hanging.", { duration: 15000 });
      }
    }, 30000);
    checkSession().finally(() => { settled = true; });
  }, []);

  // Show thank-you toast when user enters the app after clicking the ad
  useEffect(() => {
    if (authStatus !== "authenticated") return;

    const showThanks = async () => {
      try {
        const store = await load("config.json");
        const shouldThank = await store.get<boolean>("ad_click_thanks");
        if (shouldThank) {
          await store.delete("ad_click_thanks");
          await store.save();
          toast.success("Thanks for your support! ", {
            duration: 3000,
            style: {
              background: "rgba(255,255,255,0.08)",
              border: "1px solid rgba(255,255,255,0.1)",
            },
          });
        }
      } catch {
        // Non-critical
      }
    };

    // Small delay to let the dashboard finish mounting
    const timer = setTimeout(showThanks, 600);
    return () => clearTimeout(timer);
  }, [authStatus]);

  // Clean up PDF preview cache files on close/beforeunload
  useEffect(() => {
    const handleClose = () => {
      invoke("cmd_clean_preview_cache").catch(() => {});
    };

    window.addEventListener("beforeunload", handleClose);
    return () => {
      window.removeEventListener("beforeunload", handleClose);
      handleClose();
    };
  }, []);

  // Styled splash screen while verifying the session
  if (authStatus === "loading") {
    return (
      <main className="h-screen w-screen flex items-center justify-center bg-stash-bg">
        <div className="flex flex-col items-center gap-4">
          <img src="/telestash-logo.png" className="w-16 h-16 drop-shadow-lg animate-pulse" alt="TeleStash" />
          <p className="text-sm text-stash-subtext tracking-wide">Restoring session...</p>
        </div>
      </main>
    );
  }

  return (
    <main className="absolute inset-0 text-stash-text overflow-hidden selection:bg-stash-primary/30">
      <UpdateBanner
        available={available}
        version={version}
        downloading={downloading}
        installing={installing}
        restarting={restarting}
        progress={progress}
        error={updateError}
        onUpdate={downloadAndInstall}
        onDismiss={remindLater}
      />
      <Toaster theme={theme} position="bottom-center" />
      {authStatus === "authenticated" && (
        <Suspense fallback={
          <div className="h-screen w-screen flex flex-col items-center justify-center bg-stash-bg">
            <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-stash-primary"></div>
          </div>
        }>
          <ErrorBoundary>
            <DesktopDashboard onLogout={() => setAuthStatus("unauthenticated")} />
          </ErrorBoundary>
        </Suspense>
      )}
      {authStatus === "unauthenticated" && (
        <AuthWizard onLogin={() => setAuthStatus("authenticated")} />
      )}
    </main>
  );
}

function App() {
  return (
    <ErrorBoundary>
      <ThemeProvider>
        <QueryClientProvider client={queryClient}>
          <ConfirmProvider>
            <SettingsProvider>
              <UpdateProvider>
                <AppContent />
              </UpdateProvider>
            </SettingsProvider>
          </ConfirmProvider>
        </QueryClientProvider>
      </ThemeProvider>
    </ErrorBoundary>
  );
}

export default App;
