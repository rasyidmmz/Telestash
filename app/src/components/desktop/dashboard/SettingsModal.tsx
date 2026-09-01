import { useState, useEffect, useCallback, useRef } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { X, RotateCcw, Download, Upload, Trash2, HardDrive, Globe, Key, Copy, Check, RefreshCw, ChevronDown, Link, Sparkles, Info, Clipboard, Monitor, Loader2, Languages, Palette, Plus, Tag } from '../../shared/icons.tsx';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { toast } from 'sonner';
import { useSettings } from '../../../context/SettingsContext';
import { useConfirm } from '../../../context/ConfirmContext';
import { useTranslation } from 'react-i18next';
import { useUpdate } from '../../../context/UpdateContext';
import { LANGUAGES } from '../../../i18n/languages';
import { ShareInfo } from '../../../types';
import { version as appVersion } from '../../../../package.json';
import { useTheme } from '../../../context/ThemeContext';
import { CustomTheme, ThemeColorPalette, generateThemeId } from '../../../theme/themeEngine';
import { getDefaultPalette } from '../../../theme/presets';
import { useModalDialog } from '../../../hooks/useModalDialog';

interface SettingsModalProps {
    isOpen: boolean;
    onClose: () => void;
}

interface ApiSettings {
    enabled: boolean;
    port: number;
    key_set: boolean;
    running: boolean;
}

type SettingsTab = 'general' | 'themes' | 'sharing' | 'about';

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const dialogRef = useModalDialog(isOpen, onClose, closeButtonRef);
    const { settings, updateSetting, resetSettings } = useSettings();
    const { confirm } = useConfirm();
    const { t } = useTranslation();
    const [clearing, setClearing] = useState(false);

    const [activeTab, setActiveTab] = useState<SettingsTab>('general');

    const {
        checking: updateChecking,
        available: updateAvailable,
        downloading: updateDownloading,
        installing: updateInstalling,
        restarting: updateRestarting,
        progress: updateProgress,
        version: updateVersion,
        error: updateError,
        checkForUpdates,
        downloadAndInstall,
        remindLater,
    } = useUpdate();

    // Diagnostics state
    const [diagLoading, setDiagLoading] = useState(false);

    const handleCheckForUpdates = useCallback(async () => {
        const updateInfo = await checkForUpdates();
        if (updateInfo === undefined) return;
        if (updateInfo) {
            const downloadNow = await confirm({
                title: t('settings.update_dialog_title'),
                message: t('settings.update_dialog_desc', { version: updateInfo.version }),
                confirmText: t('settings.download_now'),
                cancelText: t('settings.remind_later'),
                variant: 'info',
            });
            if (downloadNow) {
                await downloadAndInstall();
            } else {
                remindLater();
                toast.success(t('settings.remind_later_toast'));
            }
        } else {
            toast.success(t('settings.latest_version_toast'));
        }
    }, [checkForUpdates, downloadAndInstall, remindLater, confirm, t]);

    const handleInstallUpdate = useCallback(async () => {
        await downloadAndInstall();
    }, [downloadAndInstall]);

    // Sharing settings state
    const [shares, setShares] = useState<ShareInfo[]>([]);
    const [refreshing, setRefreshing] = useState(false);
    const [copiedId, setCopiedId] = useState<string | null>(null);
    const [globalDomain, setGlobalDomain] = useState('');

    const fetchShares = useCallback(async () => {
        setRefreshing(true);
        try {
            const list = await invoke<ShareInfo[]>('cmd_list_shares');
            setShares(list);
        } catch (e) {
            toast.error(t('settings.load_shares_failed', { error: e }));
        } finally {
            setRefreshing(false);
        }
    }, [t]);

    useEffect(() => {
        if (isOpen && activeTab === 'sharing') {
            fetchShares();
        }
    }, [isOpen, activeTab, fetchShares]);

    const handleRevokeShare = async (id: string) => {
        const ok = await confirm({
            title: t('settings.revoke_link_title'),
            message: t('settings.revoke_link_desc'),
            confirmText: t('settings.revoke'),
            variant: 'danger',
        });
        if (!ok) return;

        try {
            await invoke('cmd_revoke_share', { id });
            toast.success(t('settings.link_revoked'));
            fetchShares();
        } catch (e) {
            toast.error(t('settings.link_revoke_failed', { error: e }));
        }
    };

    const handleCopyShare = (id: string) => {
        const share = shares.find(s => s.id === id);
        if (!share) return;
        
        let link = `http://127.0.0.1:14201/d/${share.id}`;
        if (globalDomain.trim()) {
            link = `http://${globalDomain.trim()}/d/${share.id}`;
        }
        
        navigator.clipboard.writeText(link);
        setCopiedId(share.id);
        setTimeout(() => setCopiedId(null), 2000);
    };

    // API settings state
    const [apiSettings, setApiSettings] = useState<ApiSettings>({ enabled: false, port: 8550, key_set: false, running: false });
    const [apiPort, setApiPort] = useState('8550');
    const [apiLoading, setApiLoading] = useState(false);
    const [generatedKey, setGeneratedKey] = useState<string | null>(null);
    const [keyCopied, setKeyCopied] = useState(false);

    const fetchApiSettings = useCallback(async () => {
        try {
            const result = await invoke<ApiSettings>('cmd_get_api_settings');
            setApiSettings(result);
            setApiPort(result.port.toString());
        } catch {
            // API settings not available
        }
    }, []);

    // Load API settings when modal opens
    useEffect(() => {
        if (isOpen) {
            fetchApiSettings();
            setGeneratedKey(null);
            setKeyCopied(false);
        }
    }, [isOpen, fetchApiSettings]);

    // Poll API status while modal is open and API is enabled
    useEffect(() => {
        if (!isOpen || !apiSettings.enabled) return;
        const interval = setInterval(fetchApiSettings, 3000);
        return () => clearInterval(interval);
    }, [isOpen, apiSettings.enabled, fetchApiSettings]);

    const handleApiToggle = async () => {
        setApiLoading(true);
        try {
            const port = parseInt(apiPort, 10);
            if (isNaN(port) || port < 1024 || port > 65535) {
                toast.error(t('settings.port_range_error'));
                setApiLoading(false);
                return;
            }
            const result = await invoke<ApiSettings>('cmd_update_api_settings', {
                enabled: !apiSettings.enabled,
                port,
            });
            setApiSettings(result);
            toast.success(result.enabled ? t('settings.api_server_started') : t('settings.api_server_stopped'));
        } catch (e) {
            toast.error(t('settings.api_update_failed', { error: e }));
        } finally {
            setApiLoading(false);
        }
    };

    const handlePortApply = async () => {
        const port = parseInt(apiPort, 10);
        if (isNaN(port) || port < 1024 || port > 65535) {
            toast.error(t('settings.port_range_error'));
            return;
        }
        if (port === apiSettings.port) return;
        setApiLoading(true);
        try {
            const result = await invoke<ApiSettings>('cmd_update_api_settings', {
                enabled: apiSettings.enabled,
                port,
            });
            setApiSettings(result);
            toast.success(t('settings.api_port_updated', { port }));
        } catch (e) {
            toast.error(t('settings.api_port_update_failed', { error: e }));
        } finally {
            setApiLoading(false);
        }
    };

    const handleGenerateKey = async () => {
        const ok = await confirm({
            title: t('settings.generate_api_key_title'),
            message: apiSettings.key_set
                ? t('settings.regenerate_api_key_desc')
                : t('settings.generate_api_key_desc'),
            confirmText: apiSettings.key_set ? t('settings.regenerate') : t('settings.generate'),
            variant: apiSettings.key_set ? 'danger' : 'info',
        });
        if (!ok) return;
        try {
            const key = await invoke<string>('cmd_regenerate_api_key');
            setGeneratedKey(key);
            setKeyCopied(false);
            setApiSettings(prev => ({ ...prev, key_set: true }));
            toast.success(t('settings.api_key_generated'));
        } catch (e) {
            toast.error(t('settings.api_key_generate_failed', { error: e }));
        }
    };

    const handleCopyKey = async () => {
        if (!generatedKey) return;
        try {
            await navigator.clipboard.writeText(generatedKey);
            setKeyCopied(true);
            setTimeout(() => setKeyCopied(false), 2000);
        } catch {
            toast.error(t('settings.copy_clipboard_failed'));
        }
    };

    return (
        <AnimatePresence>
            {isOpen && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm"
                    onClick={onClose}
                >
                    <motion.div
                        ref={dialogRef}
                        layout
                        initial={{ opacity: 0, scale: 0.95, y: 10 }}
                        animate={{ opacity: 1, scale: 1, y: 0 }}
                        exit={{ opacity: 0, scale: 0.95, y: 10 }}
                        transition={{ type: 'spring', damping: 25, stiffness: 220 }}
                        className="bg-stash-surface border border-stash-border rounded-xl w-[440px] shadow-2xl overflow-hidden flex flex-col"
                        onClick={e => e.stopPropagation()}
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="settings-modal-title"
                        tabIndex={-1}
                    >
                        {/* Header */}
                        <div className="px-5 py-4 border-b border-stash-border flex justify-between items-center">
                            <h2 id="settings-modal-title" className="text-stash-text font-semibold text-base">{t('settings.title')}</h2>
                            <button
                                ref={closeButtonRef}
                                onClick={onClose}
                                className="p-1.5 hover:bg-stash-hover rounded-lg text-stash-subtext hover:text-stash-text transition"
                                aria-label="Close settings"
                            >
                                <X className="w-4 h-4" />
                            </button>
                        </div>

                        {/* Tab Bar */}
                        <div className="px-5 pt-3 pb-0 flex gap-1 justify-start overflow-x-auto border-b border-stash-border scrollbar-none">
                            {([['general', Globe], ['themes', Palette], ['sharing', Link], ['about', Info]] as const).map(([key, Icon]) => (
                                <button
                                    key={key}
                                    onClick={() => setActiveTab(key as SettingsTab)}
                                    className={`flex items-center gap-1.5 px-3 py-2 text-xs font-medium rounded-t-lg transition-colors shrink-0 ${
                                        activeTab === key
                                            ? 'text-stash-primary border-b-2 border-stash-primary bg-stash-primary/5'
                                            : 'text-stash-subtext hover:text-stash-text hover:bg-stash-hover/50'
                                    }`}
                                >
                                    <Icon className="w-3.5 h-3.5" />
                                    {t(`settings.tab_${key}`)}
                                </button>
                            ))}
                        </div>

                        {/* Body */}
                        <motion.div layout className="px-5 py-4 max-h-[70vh] overflow-y-auto overflow-x-hidden relative">
                            <AnimatePresence mode="popLayout" initial={false}>

                                {activeTab === 'general' && (
                                    <motion.div
                                        key="general"
                                        initial={{ opacity: 0, x: -20 }}
                                        animate={{ opacity: 1, x: 0 }}
                                        exit={{ opacity: 0, x: 20 }}
                                        transition={{ type: 'spring', damping: 25, stiffness: 220, opacity: { duration: 0.15 } }}
                                        className="space-y-6 w-full"
                                    >

                            {/* Transfers Section */}
                            <section className="space-y-3">
                                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                                    <Upload className="w-3.5 h-3.5" />
                                    {t('settings.transfers')}
                                </h3>

                                {/* Max Concurrent Uploads */}
                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div className="flex items-center gap-2">
                                        <Upload className="w-4 h-4 text-stash-subtext" />
                                        <div>
                                            <p className="text-sm text-stash-text font-medium">{t('settings.concurrent_uploads')}</p>
                                            <p className="text-xs text-stash-subtext">{t('settings.max_uploads_desc')}</p>
                                        </div>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        <button
                                            onClick={() => updateSetting('maxConcurrentUploads', Math.max(1, settings.maxConcurrentUploads - 1))}
                                            className="w-7 h-7 flex items-center justify-center rounded-md bg-stash-bg text-stash-subtext hover:text-stash-text hover:bg-stash-border transition text-sm font-medium"
                                        >
                                            -
                                        </button>
                                        <span className="text-sm text-stash-text font-medium w-5 text-center">
                                            {settings.maxConcurrentUploads}
                                        </span>
                                        <button
                                            onClick={() => updateSetting('maxConcurrentUploads', Math.min(10, settings.maxConcurrentUploads + 1))}
                                            className="w-7 h-7 flex items-center justify-center rounded-md bg-stash-bg text-stash-subtext hover:text-stash-text hover:bg-stash-border transition text-sm font-medium"
                                        >
                                            +
                                        </button>
                                    </div>
                                </div>

                                {/* Max Concurrent Downloads */}
                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div className="flex items-center gap-2">
                                        <Download className="w-4 h-4 text-stash-subtext" />
                                        <div>
                                            <p className="text-sm text-stash-text font-medium">{t('settings.concurrent_downloads')}</p>
                                            <p className="text-xs text-stash-subtext">{t('settings.max_downloads_desc')}</p>
                                        </div>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        <button
                                            onClick={() => updateSetting('maxConcurrentDownloads', Math.max(1, settings.maxConcurrentDownloads - 1))}
                                            className="w-7 h-7 flex items-center justify-center rounded-md bg-stash-bg text-stash-subtext hover:text-stash-text hover:bg-stash-border transition text-sm font-medium"
                                        >
                                            -
                                        </button>
                                        <span className="text-sm text-stash-text font-medium w-5 text-center">
                                            {settings.maxConcurrentDownloads}
                                        </span>
                                        <button
                                            onClick={() => updateSetting('maxConcurrentDownloads', Math.min(10, settings.maxConcurrentDownloads + 1))}
                                            className="w-7 h-7 flex items-center justify-center rounded-md bg-stash-bg text-stash-subtext hover:text-stash-text hover:bg-stash-border transition text-sm font-medium"
                                        >
                                            +
                                        </button>
                                    </div>
                                </div>

                                {/* Hide Folder Groups */}
                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div className="flex items-center gap-2">
                                        <Tag className="w-4 h-4 text-stash-subtext" />
                                        <div>
                                            <p className="text-sm text-stash-text font-medium">{t('common.hide_groups')}</p>
                                            <p className="text-xs text-stash-subtext">{t('common.hide_groups_desc')}</p>
                                        </div>
                                    </div>
                                    <button
                                        onClick={() => updateSetting('hideGroups', !settings.hideGroups)}
                                        className={`relative w-11 h-6 rounded-full transition-colors duration-200 ${settings.hideGroups ? 'bg-stash-primary' : 'bg-stash-border'}`}
                                    >
                                        <span className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform duration-200 ${settings.hideGroups ? 'translate-x-5' : 'translate-x-0'}`} />
                                    </button>
                                </div>

                                {/* Windows Autostart */}
                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div className="flex items-center gap-2">
                                        <Monitor className="w-4 h-4 text-stash-subtext" />
                                        <div>
                                            <p className="text-sm text-stash-text font-medium">{t('settings.windows_autostart')}</p>
                                            <p className="text-xs text-stash-subtext">{t('settings.windows_autostart_desc')}</p>
                                        </div>
                                    </div>
                                    <button
                                        onClick={async () => {
                                            const nextVal = !settings.windowsAutostart;
                                            updateSetting('windowsAutostart', nextVal);
                                            try {
                                                await invoke('cmd_set_autostart', { enabled: nextVal });
                                                toast.success(nextVal ? "Autostart enabled" : "Autostart disabled");
                                            } catch (e: any) {
                                                toast.error(e.toString());
                                            }
                                        }}
                                        className={`relative w-11 h-6 rounded-full transition-colors duration-200 ${settings.windowsAutostart ? 'bg-stash-primary' : 'bg-stash-border'}`}
                                    >
                                        <span className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform duration-200 ${settings.windowsAutostart ? 'translate-x-5' : 'translate-x-0'}`} />
                                    </button>
                                </div>
                            </section>

                            {/* Language & Region Section */}
                            <section className="space-y-3">
                                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                                    <Languages className="w-3.5 h-3.5" />
                                    {t('settings.language_region')}
                                </h3>

                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div className="flex items-center gap-2">
                                        <Globe className="w-4 h-4 text-stash-subtext" />
                                        <div>
                                            <p className="text-sm text-stash-text font-medium">{t('settings.app_language')}</p>
                                            <p className="text-xs text-stash-subtext">{t('settings.choose_language')}</p>
                                        </div>
                                    </div>
                                    <div className="relative">
                                        <select
                                            value={settings.language}
                                            onChange={e => updateSetting('language', e.target.value as any)}
                                            className="appearance-none bg-stash-bg border border-stash-border rounded-md pl-3 pr-8 py-1.5 text-sm text-stash-text focus:outline-none focus:border-stash-primary/50 transition cursor-pointer"
                                        >
                                            {LANGUAGES.map(lang => (
                                                <option key={lang.code} value={lang.code}>
                                                    {lang.nativeLabel}
                                                </option>
                                            ))}
                                        </select>
                                        <ChevronDown className="w-4 h-4 text-stash-subtext absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
                                    </div>
                                </div>
                            </section>

                            {/* REST API Section */}
                            <section className="space-y-3">
                                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                                    <Globe className="w-3.5 h-3.5" />
                                    {t('settings.rest_api')}
                                </h3>

                                {/* Enable Toggle */}
                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div className="flex items-center gap-2">
                                        <div className={`w-2 h-2 rounded-full ${apiSettings.running ? 'bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.5)]' : 'bg-gray-500'}`} />
                                        <div>
                                            <p className="text-sm text-stash-text font-medium">{t('settings.enable_api_server')}</p>
                                            <p className="text-xs text-stash-subtext">
                                                {apiSettings.running ? t('settings.api_running', { port: apiSettings.port }) : t('settings.api_stopped')}
                                            </p>
                                        </div>
                                    </div>
                                    <button
                                        onClick={handleApiToggle}
                                        disabled={apiLoading}
                                        className={`relative w-11 h-6 rounded-full transition-colors duration-200 ${apiSettings.enabled ? 'bg-stash-primary' : 'bg-stash-border'} disabled:opacity-50`}
                                    >
                                        <span className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform duration-200 ${apiSettings.enabled ? 'translate-x-5' : 'translate-x-0'}`} />
                                    </button>
                                </div>

                                {/* Port */}
                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div>
                                        <p className="text-sm text-stash-text font-medium">{t('common.port')}</p>
                                        <p className="text-xs text-stash-subtext">1024 - 65535</p>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        <input
                                            type="number"
                                            min="1024"
                                            max="65535"
                                            value={apiPort}
                                            onChange={e => setApiPort(e.target.value)}
                                            onBlur={handlePortApply}
                                            onKeyDown={e => { if (e.key === 'Enter') handlePortApply(); }}
                                            className="w-20 bg-stash-bg border border-stash-border rounded-md px-2 py-1 text-sm text-stash-text text-center focus:outline-none focus:border-stash-primary/50 transition"
                                        />
                                    </div>
                                </div>

                                {/* API Key */}
                                <div className="p-3 rounded-lg bg-stash-hover/50 space-y-2.5">
                                    <div className="flex items-center justify-between">
                                        <div className="flex items-center gap-2">
                                            <Key className="w-4 h-4 text-stash-subtext" />
                                            <div>
                                                <p className="text-sm text-stash-text font-medium">{t('settings.api_key')}</p>
                                                <p className="text-xs text-stash-subtext">
                                                    {apiSettings.key_set ? t('settings.api_key_configured') : t('settings.api_key_unset')}
                                                </p>
                                            </div>
                                        </div>
                                        <button
                                            onClick={handleGenerateKey}
                                            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-stash-primary/10 text-stash-primary hover:bg-stash-primary/20 transition"
                                        >
                                            <RefreshCw className="w-3 h-3" />
                                            {apiSettings.key_set ? t('settings.regenerate') : t('settings.generate')}
                                        </button>
                                    </div>

                                    {/* One-time key reveal */}
                                    {generatedKey && (
                                        <div className="mt-2 p-2.5 bg-stash-bg rounded-lg border border-yellow-500/20">
                                            <p className="text-[10px] text-yellow-400/80 uppercase tracking-wider font-semibold mb-1.5">
                                                {t('settings.api_copy_alert')}
                                            </p>
                                            <div className="flex items-center gap-2">
                                                <code className="flex-1 text-xs text-stash-text font-mono bg-stash-hover rounded px-2 py-1.5 overflow-x-auto select-all">
                                                    {generatedKey}
                                                </code>
                                                <button
                                                    onClick={handleCopyKey}
                                                    className="p-1.5 rounded-md hover:bg-stash-hover text-stash-subtext hover:text-stash-text transition flex-shrink-0"
                                                    title="Copy to clipboard"
                                                >
                                                    {keyCopied ? <Check className="w-4 h-4 text-green-400" /> : <Copy className="w-4 h-4" />}
                                                </button>
                                            </div>
                                        </div>
                                    )}
                                </div>
                            </section>

                            {/* Storage Section */}
                            <section className="space-y-3">
                                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                                    <HardDrive className="w-3.5 h-3.5" />
                                    {t('settings.storage')}
                                </h3>

                                <div className="flex items-center justify-between p-3 rounded-lg bg-stash-hover/50">
                                    <div className="flex items-center gap-2">
                                        <Trash2 className="w-4 h-4 text-stash-subtext" />
                                        <div>
                                            <p className="text-sm text-stash-text font-medium">{t('settings.clear_local_cache')}</p>
                                            <p className="text-xs text-stash-subtext">{t('settings.clear_local_cache_desc')}</p>
                                        </div>
                                    </div>
                                    <button
                                        disabled={clearing}
                                        onClick={async () => {
                                            const ok = await confirm({
                                                title: t('settings.clear_cache_title'),
                                                message: t('settings.clear_cache_desc'),
                                                confirmText: t('settings.clear'),
                                                variant: 'danger',
                                            });
                                            if (!ok) return;
                                            setClearing(true);
                                            try {
                                                await invoke('cmd_clean_cache');
                                                toast.success(t('settings.cache_cleared'));
                                            } catch {
                                                toast.error(t('settings.cache_clear_failed'));
                                            } finally {
                                                setClearing(false);
                                            }
                                        }}
                                        className="px-3 py-1.5 rounded-lg text-xs font-medium bg-red-500/10 text-red-400 hover:bg-red-500/20 transition disabled:opacity-50 disabled:cursor-not-allowed"
                                    >
                                        {clearing ? t('settings.clearing') : t('settings.clear')}
                                    </button>
                                </div>

                            </section>

                            {/* Updates Section */}
                            <section className="space-y-3">
                                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                                    <Sparkles className="w-3.5 h-3.5" />
                                    {t('settings.updates')}
                                </h3>

                                <div className="p-3 rounded-lg bg-stash-hover/50 space-y-3">
                                    <div className="flex items-center justify-between">
                                        <div className="flex items-center gap-2">
                                            <Download className="w-4 h-4 text-stash-subtext" />
                                            <div>
                                                <p className="text-sm text-stash-text font-medium">{t('settings.check_for_updates')}</p>
                                                <p className="text-xs text-stash-subtext">
                                                    {updateVersion ? t('settings.update_available', { version: updateVersion }) : t('settings.check_updates_desc')}
                                                </p>
                                            </div>
                                        </div>
                                        {updateAvailable && !updateDownloading && !updateInstalling && !updateRestarting ? (
                                            <button
                                                onClick={handleInstallUpdate}
                                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-stash-primary text-white hover:bg-stash-primary/90 transition"
                                            >
                                                <Download className="w-3 h-3" />
                                                {t('settings.update_restart')}
                                            </button>
                                        ) : updateDownloading || updateInstalling || updateRestarting ? (
                                            <div className="flex items-center gap-2">
                                                <RefreshCw className="w-3.5 h-3.5 text-stash-primary animate-spin" />
                                                <span className="text-xs text-stash-primary font-mono">
                                                    {updateRestarting ? 'restart' : updateInstalling ? 'install' : `${updateProgress}%`}
                                                </span>
                                            </div>
                                        ) : (
                                            <button
                                                onClick={handleCheckForUpdates}
                                                disabled={updateChecking}
                                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-stash-primary/10 text-stash-primary hover:bg-stash-primary/20 transition disabled:opacity-50"
                                            >
                                                <RefreshCw className={`w-3 h-3 ${updateChecking ? 'animate-spin' : ''}`} />
                                                {updateChecking ? t('settings.checking') : t('settings.check_now')}
                                            </button>
                                        )}
                                    </div>
                                    {(updateDownloading || updateInstalling || updateRestarting) && (
                                        <div className="w-full h-1.5 bg-stash-border rounded-full overflow-hidden">
                                            <div
                                                className="h-full bg-stash-primary rounded-full transition-all duration-300"
                                                style={{ width: `${updateProgress}%` }}
                                            />
                                        </div>
                                    )}
                                    {updateError && (
                                        <p className="text-xs text-red-400 break-words">{updateError}</p>
                                    )}
                                </div>
                            </section>

                                    </motion.div>
                                )}

                        {activeTab === 'sharing' && (
                                    <motion.section
                                        key="sharing"
                                        initial={{ opacity: 0, x: -20 }}
                                        animate={{ opacity: 1, x: 0 }}
                                        exit={{ opacity: 0, x: 20 }}
                                        transition={{ type: 'spring', damping: 25, stiffness: 220, opacity: { duration: 0.15 } }}
                                        className="space-y-4 w-full"
                                    >
                                        <div className="flex items-center justify-between">
                                            <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                                                <Link className="w-3.5 h-3.5 text-stash-primary" />
                                                {t('settings.shared_links', { count: shares.length })}
                                            </h3>
                                            <button 
                                                onClick={fetchShares} 
                                                className="text-stash-subtext hover:text-stash-text p-1 rounded hover:bg-stash-hover transition"
                                                title={t('settings.refresh_links')}
                                            >
                                                <RefreshCw className={`w-3.5 h-3.5 ${refreshing ? 'animate-spin' : ''}`} />
                                            </button>
                                        </div>

                                        <div className="bg-stash-hover/30 border border-stash-border/50 rounded-lg p-3 space-y-2">
                                            <div className="text-[11px] font-semibold text-stash-text flex items-center gap-1">🌐 {t('settings.ip_override')}</div>
                                            <input
                                                type="text"
                                                placeholder="e.g. 100.115.22.45 or my-pc:14201"
                                                value={globalDomain}
                                                onChange={(e) => setGlobalDomain(e.target.value)}
                                                className="w-full bg-stash-surface border border-stash-border rounded-md px-2.5 py-1.5 text-xs text-stash-text focus:outline-none focus:border-stash-primary/50 placeholder:text-stash-subtext/40"
                                            />
                                            <p className="text-[10px] text-stash-subtext">
                                                {t('settings.ip_override_desc')}
                                            </p>
                                        </div>

                                        {shares.length === 0 ? (
                                            <div className="py-8 text-center space-y-2">
                                                <Link className="w-8 h-8 text-stash-subtext/40 mx-auto" />
                                                <p className="text-sm font-medium text-stash-text">{t('settings.no_active_links')}</p>
                                                <p className="text-xs text-stash-subtext">{t('settings.no_active_links_desc')}</p>
                                            </div>
                                        ) : (
                                            <div className="space-y-2 max-h-[300px] overflow-y-auto pr-1 custom-scrollbar">
                                                {shares.map((share) => {
                                                    const isExpired = share.expires_at ? (share.expires_at < Math.floor(Date.now() / 1000)) : false;
                                                    return (
                                                        <div key={share.id} className="p-3 rounded-lg bg-stash-hover/40 border border-stash-border/50 flex flex-col gap-2 relative">
                                                              <div className="flex justify-between items-start gap-4">
                                                                <div className="min-w-0 flex-1">
                                                                    <div className="text-xs font-semibold text-stash-text truncate" title={share.file_name}>
                                                                        {share.file_name}
                                                                    </div>
                                                                    <div className="flex gap-2 items-center mt-1 flex-wrap text-[10px]">
                                                                        <span className="text-stash-subtext">
                                                                            {new Date(share.created_at * 1000).toLocaleDateString()}
                                                                        </span>
                                                                        <span className="w-1 h-1 rounded-full bg-stash-border" />
                                                                        {share.has_password ? (
                                                                            <span className="text-emerald-400 bg-emerald-500/10 px-1.5 py-0.5 rounded flex items-center gap-0.5 font-medium">
                                                                                <Key className="w-2.5 h-2.5" /> {t('settings.protected')}
                                                                            </span>
                                                                        ) : (
                                                                            <span className="text-blue-400 bg-blue-500/10 px-1.5 py-0.5 rounded font-medium">{t('settings.public')}</span>
                                                                        )}
                                                                        <span className="w-1 h-1 rounded-full bg-stash-border" />
                                                                        {share.expires_at ? (
                                                                            isExpired ? (
                                                                                <span className="text-red-400 bg-red-500/10 px-1.5 py-0.5 rounded font-medium">{t('settings.expired')}</span>
                                                                            ) : (
                                                                                <span className="text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded font-medium">
                                                                                    {t('settings.expires_at', { date: new Date(share.expires_at * 1000).toLocaleDateString() })}
                                                                                </span>
                                                                            )
                                                                        ) : (
                                                                            <span className="text-teal-400 bg-teal-500/10 px-1.5 py-0.5 rounded font-medium">{t('settings.never_expires')}</span>
                                                                        )}
                                                                    </div>
                                                                </div>
                                                                
                                                                <div className="flex gap-1">
                                                                    <button
                                                                        onClick={() => handleCopyShare(share.id)}
                                                                        className={`p-1.5 rounded bg-stash-surface border border-stash-border text-stash-text hover:bg-stash-hover transition ${
                                                                            copiedId === share.id ? 'text-emerald-400 border-emerald-500/30 bg-emerald-500/5' : ''
                                                                        }`}
                                                                        title={t('settings.copy_share_link')}
                                                                    >
                                                                        {copiedId === share.id ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
                                                                    </button>
                                                                    <button
                                                                        onClick={() => handleRevokeShare(share.id)}
                                                                        className="p-1.5 rounded bg-stash-surface border border-stash-border text-red-400 hover:bg-red-500/10 hover:border-red-500/30 transition"
                                                                        title={t('settings.revoke_link')}
                                                                    >
                                                                        <Trash2 className="w-3.5 h-3.5" />
                                                                    </button>
                                                                </div>
                                                            </div>
                                                        </div>
                                                    );
                                                })}
                                            </div>
                                        )}
                                    </motion.section>
                                )}
                                {activeTab === 'themes' && (
                                    <ThemesTab />
                                )}
                                {activeTab === 'about' && (
                                    <motion.section
                                        key="about"
                                        initial={{ opacity: 0, x: -20 }}
                                        animate={{ opacity: 1, x: 0 }}
                                        exit={{ opacity: 0, x: 20 }}
                                        transition={{ type: 'spring', damping: 25, stiffness: 220, opacity: { duration: 0.15 } }}
                                        className="space-y-4 w-full"
                                    >
                                        <div className="flex flex-col items-center py-6 space-y-5">
                                            {/* Logo */}
                                            <img src="/telestash-logo.png" className="w-16 h-16 drop-shadow-lg" alt="TeleStash" />
                                            
                                            {/* App Name & Version */}
                                            <div className="text-center">
                                                <h3 className="text-base font-bold text-stash-text">TeleStash</h3>
                                                <p className="text-xs text-stash-primary font-medium mt-0.5">Personal Cinema Cloud & High-Speed Media Vault</p>
                                                <p className="text-xs text-stash-subtext mt-0.5">v{appVersion}</p>
                                            </div>

                                            {/* Divider */}
                                            <div className="w-12 h-px bg-stash-border" />

                                            {/* Diagnostics */}
                                            <button
                                                onClick={async () => {
                                                    setDiagLoading(true);
                                                    try {
                                                        const info = await invoke<string>('cmd_get_system_diagnostics');
                                                        await navigator.clipboard.writeText(info);
                                                        toast.success(t('settings.diagnostics_copied'));
                                                    } catch (e) {
                                                        toast.error(t('settings.diagnostics_copy_failed', { error: e }));
                                                    } finally {
                                                        setDiagLoading(false);
                                                    }
                                                }}
                                                disabled={diagLoading}
                                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-stash-hover border border-stash-border text-stash-subtext hover:text-stash-text hover:bg-stash-border/30 transition disabled:opacity-50"
                                            >
                                                {diagLoading ? (
                                                    <Loader2 className="w-3 h-3 animate-spin" />
                                                ) : (
                                                    <Clipboard className="w-3 h-3" />
                                                )}
                                                {t('settings.copy_diagnostics')}
                                            </button>

                                            {/* Creator Info */}
                                            <div className="text-center space-y-3">
                                                <div>
                                                    <p className="text-sm font-semibold text-stash-text">Cameron Amer</p>
                                                </div>

                                                {/* Website Link */}
                                                <button
                                                    onClick={(e) => { e.preventDefault(); open('https://www.cameronamer.com'); }}
                                                    className="flex items-center justify-center gap-1.5 text-xs text-stash-primary hover:text-stash-primary/80 transition-colors cursor-pointer"
                                                >
                                                    <Globe className="w-3.5 h-3.5" />
                                                    www.cameronamer.com
                                                </button>

                                                {/* GitHub Link */}
                                                <button
                                                    onClick={(e) => { e.preventDefault(); open('https://github.com/rasyidmmz/Telestash'); }}
                                                    className="flex items-center justify-center gap-1.5 text-xs text-stash-primary hover:text-stash-primary/80 transition-colors cursor-pointer"
                                                >
                                                    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="currentColor">
                                                        <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
                                                    </svg>
                                                    github.com/rasyidmmz/Telestash
                                                </button>
                                            </div>

                                            {/* Tagline */}
                                            <p className="text-[11px] text-stash-subtext/60 leading-relaxed max-w-[280px] text-center">
                                                {t('settings.tagline')}
                                            </p>
                                        </div>
                                    </motion.section>
                                )}
                            </AnimatePresence>
                        </motion.div>

                        {/* Footer */}
                        <div className="px-5 py-3 border-t border-stash-border flex items-center justify-between">
                            <button
                                onClick={resetSettings}
                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs text-stash-subtext hover:text-red-400 hover:bg-red-500/10 transition font-medium"
                            >
                                <RotateCcw className="w-3.5 h-3.5" />
                                {t('settings.reset_defaults')}
                            </button>
                            <button
                                onClick={onClose}
                                className="px-4 py-1.5 rounded-lg text-xs font-medium bg-stash-primary text-white hover:bg-stash-primary/90 transition"
                            >
                                {t('settings.done')}
                            </button>
                        </div>
                    </motion.div>
                </motion.div>
            )}
        </AnimatePresence>
    );
}

// ── Themes Tab ──────────────────────────────────────────────────────
// Inline component (follows the pattern of the other tabs in this file).

const PALETTE_KEYS: { key: keyof ThemeColorPalette; labelKey: string }[] = [
    { key: 'bg', labelKey: 'settings.color_bg' },
    { key: 'surface', labelKey: 'settings.color_surface' },
    { key: 'primary', labelKey: 'settings.color_primary' },
    { key: 'secondary', labelKey: 'settings.color_secondary' },
    { key: 'text', labelKey: 'settings.color_text' },
    { key: 'subtext', labelKey: 'settings.color_subtext' },
];

function ThemesTab() {
    const { t } = useTranslation();
    const {
        customThemes,
        activeCustomThemeId,
        setActiveCustomTheme,
        addCustomTheme,
        deleteCustomTheme,
        updateCustomTheme,
    } = useTheme();
    const { confirm } = useConfirm();

    const [editingId, setEditingId] = useState<string | null>(null);

    const builtinThemes = customThemes.filter(t => t.isBuiltin);
    const userThemes = customThemes.filter(t => !t.isBuiltin);
    const editingTheme = editingId ? customThemes.find(t => t.id === editingId) : null;

    const handleCreateTheme = () => {
        const id = generateThemeId();
        const newTheme: CustomTheme = {
            id,
            name: 'My Theme',
            isDark: true,
            palette: getDefaultPalette(true),
        };
        addCustomTheme(newTheme);
        setEditingId(id);
        setActiveCustomTheme(id);
    };

    const handleSelectTheme = (theme: CustomTheme) => {
        if (activeCustomThemeId === theme.id) {
            // Deselect → reset to default
            setActiveCustomTheme(null);
            setEditingId(null);
        } else {
            setActiveCustomTheme(theme.id);
            if (!theme.isBuiltin) {
                setEditingId(theme.id);
            } else {
                setEditingId(null);
            }
        }
    };

    const handleDeleteTheme = async (id: string) => {
        const ok = await confirm({
            title: t('settings.delete_theme'),
            message: t('settings.delete_theme_confirm'),
            confirmText: t('common.delete'),
            variant: 'danger',
        });
        if (!ok) return;
        deleteCustomTheme(id);
        if (editingId === id) setEditingId(null);
    };

    const handlePaletteChange = (key: keyof ThemeColorPalette, value: string) => {
        if (!editingTheme || editingTheme.isBuiltin) return;
        const newPalette = { ...editingTheme.palette, [key]: value };
        updateCustomTheme(editingTheme.id, { palette: newPalette });
    };

    const handleBaseToggle = (isDark: boolean) => {
        if (!editingTheme || editingTheme.isBuiltin) return;
        updateCustomTheme(editingTheme.id, { isDark });
    };

    const handleNameChange = (name: string) => {
        if (!editingTheme || editingTheme.isBuiltin) return;
        updateCustomTheme(editingTheme.id, { name });
    };

    return (
        <motion.section
            key="themes"
            initial={{ opacity: 0, x: -20 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: 20 }}
            transition={{ type: 'spring', damping: 25, stiffness: 220, opacity: { duration: 0.15 } }}
            className="space-y-5 w-full"
        >
            {/* Presets */}
            <div className="space-y-2">
                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                    <Palette className="w-3.5 h-3.5" />
                    {t('settings.presets')}
                </h3>
                <div className="grid grid-cols-4 gap-2">
                    {builtinThemes.map(theme => (
                        <button
                            key={theme.id}
                            onClick={() => handleSelectTheme(theme)}
                            className={`relative rounded-lg p-0.5 transition-all duration-200 ${
                                activeCustomThemeId === theme.id
                                    ? 'ring-2 ring-stash-primary ring-offset-1 ring-offset-stash-surface'
                                    : 'hover:ring-1 hover:ring-stash-subtext/30'
                            }`}
                            title={theme.name}
                        >
                            {/* Color preview swatch */}
                            <div className="rounded-md overflow-hidden h-10 flex">
                                <div className="flex-1" style={{ background: theme.palette.bg }} />
                                <div className="flex-1" style={{ background: theme.palette.surface }} />
                                <div className="flex-1" style={{ background: theme.palette.primary }} />
                            </div>
                            <p className="text-[10px] text-stash-subtext mt-1 truncate text-center">
                                {theme.name}
                            </p>
                            {activeCustomThemeId === theme.id && (
                                <div className="absolute -top-1 -right-1 w-4 h-4 bg-stash-primary rounded-full flex items-center justify-center">
                                    <Check className="w-2.5 h-2.5 text-white" />
                                </div>
                            )}
                        </button>
                    ))}
                </div>
            </div>

            {/* Custom Themes */}
            <div className="space-y-2">
                <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider flex items-center gap-2">
                    <Sparkles className="w-3.5 h-3.5" />
                    {t('settings.custom_themes')}
                </h3>

                {userThemes.length > 0 && (
                    <div className="grid grid-cols-4 gap-2">
                        {userThemes.map(theme => (
                            <button
                                key={theme.id}
                                onClick={() => handleSelectTheme(theme)}
                                className={`relative rounded-lg p-0.5 transition-all duration-200 ${
                                    activeCustomThemeId === theme.id
                                        ? 'ring-2 ring-stash-primary ring-offset-1 ring-offset-stash-surface'
                                        : 'hover:ring-1 hover:ring-stash-subtext/30'
                                }`}
                                title={theme.name}
                            >
                                <div className="rounded-md overflow-hidden h-10 flex">
                                    <div className="flex-1" style={{ background: theme.palette.bg }} />
                                    <div className="flex-1" style={{ background: theme.palette.surface }} />
                                    <div className="flex-1" style={{ background: theme.palette.primary }} />
                                </div>
                                <p className="text-[10px] text-stash-subtext mt-1 truncate text-center">
                                    {theme.name}
                                </p>
                                {activeCustomThemeId === theme.id && (
                                    <div className="absolute -top-1 -right-1 w-4 h-4 bg-stash-primary rounded-full flex items-center justify-center">
                                        <Check className="w-2.5 h-2.5 text-white" />
                                    </div>
                                )}
                            </button>
                        ))}
                    </div>
                )}

                <button
                    onClick={handleCreateTheme}
                    className="w-full flex items-center justify-center gap-2 px-3 py-2.5 rounded-lg border border-dashed border-stash-border text-stash-subtext hover:text-stash-primary hover:border-stash-primary/50 transition-colors text-xs"
                >
                    <Plus className="w-3.5 h-3.5" />
                    {t('settings.create_theme')}
                </button>
            </div>

            {/* Editor (shown when a custom theme is selected) */}
            {editingTheme && !editingTheme.isBuiltin && (
                <div className="space-y-3 p-3 rounded-lg bg-stash-hover/30 border border-stash-border/50">
                    <h3 className="text-xs font-semibold text-stash-subtext uppercase tracking-wider">
                        {t('settings.edit_theme')}
                    </h3>

                    {/* Theme Name */}
                    <div className="flex items-center gap-2">
                        <label className="text-xs text-stash-subtext w-16 shrink-0">{t('settings.theme_name')}</label>
                        <input
                            type="text"
                            value={editingTheme.name}
                            onChange={e => handleNameChange(e.target.value)}
                            className="flex-1 px-2 py-1.5 rounded-md text-xs bg-stash-surface border border-stash-border text-stash-text focus:border-stash-primary outline-none transition"
                            maxLength={32}
                        />
                    </div>

                    {/* Base Mode Toggle */}
                    <div className="flex items-center gap-2">
                        <label className="text-xs text-stash-subtext w-16 shrink-0">{t('settings.base_mode')}</label>
                        <div className="flex gap-1">
                            <button
                                onClick={() => handleBaseToggle(true)}
                                className={`px-3 py-1 rounded-md text-xs font-medium transition ${
                                    editingTheme.isDark
                                        ? 'bg-stash-primary text-white'
                                        : 'bg-stash-hover text-stash-subtext hover:text-stash-text'
                                }`}
                            >
                                Dark
                            </button>
                            <button
                                onClick={() => handleBaseToggle(false)}
                                className={`px-3 py-1 rounded-md text-xs font-medium transition ${
                                    !editingTheme.isDark
                                        ? 'bg-stash-primary text-white'
                                        : 'bg-stash-hover text-stash-subtext hover:text-stash-text'
                                }`}
                            >
                                Light
                            </button>
                        </div>
                    </div>

                    {/* Color Pickers */}
                    <div className="space-y-2">
                        {PALETTE_KEYS.map(({ key, labelKey }) => (
                            <div key={key} className="flex items-center gap-2">
                                <label className="text-xs text-stash-subtext w-16 shrink-0">{t(labelKey)}</label>
                                <div className="flex items-center gap-1.5 flex-1">
                                    <input
                                        type="color"
                                        value={editingTheme.palette[key].startsWith('rgba') ? '#888888' : editingTheme.palette[key]}
                                        onChange={e => handlePaletteChange(key, e.target.value)}
                                        className="w-7 h-7 rounded-md border border-stash-border cursor-pointer p-0.5 bg-transparent"
                                    />
                                    <input
                                        type="text"
                                        value={editingTheme.palette[key]}
                                        onChange={e => handlePaletteChange(key, e.target.value)}
                                        className="flex-1 px-2 py-1 rounded-md text-xs bg-stash-surface border border-stash-border text-stash-text focus:border-stash-primary outline-none transition font-mono"
                                        maxLength={30}
                                    />
                                </div>
                            </div>
                        ))}
                    </div>

                    {/* Delete Button */}
                    <button
                        onClick={() => handleDeleteTheme(editingTheme.id)}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-red-400 hover:bg-red-500/10 transition"
                    >
                        <Trash2 className="w-3.5 h-3.5" />
                        {t('settings.delete_theme')}
                    </button>
                </div>
            )}

            {/* Reset to Default */}
            {activeCustomThemeId && (
                <button
                    onClick={() => {
                        setActiveCustomTheme(null);
                        setEditingId(null);
                    }}
                    className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-xs font-medium text-stash-subtext hover:text-stash-text bg-stash-hover/50 hover:bg-stash-hover transition"
                >
                    <RotateCcw className="w-3.5 h-3.5" />
                    {t('settings.reset_default')}
                </button>
            )}
        </motion.section>
    );
}
