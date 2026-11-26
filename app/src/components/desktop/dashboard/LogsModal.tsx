import { AnimatePresence, motion } from 'framer-motion';
import { AlertTriangle, Copy, Download, Terminal, Trash2, X } from '../../shared/icons.tsx';
import { toast } from 'sonner';
import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { clearErrorLogs, ErrorLogEntry, getErrorLogsText, useErrorLogs } from '../../../errorLogs';
import { useModalDialog } from '../../../hooks/useModalDialog';
import { humanizeError } from '../../../utils/errorHumanizer';
import i18n from '../../../i18n';

interface LogsModalProps {
    isOpen: boolean;
    onClose: () => void;
}

export function LogsModal({ isOpen, onClose }: LogsModalProps) {
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const dialogRef = useModalDialog(isOpen, onClose, closeButtonRef);
    const frontendLogs = useErrorLogs();
    const [backendLogs, setBackendLogs] = useState<ErrorLogEntry[]>([]);
    const logs = useMemo(
        () => [...backendLogs, ...frontendLogs].sort((a, b) => Date.parse(b.time) - Date.parse(a.time)),
        [backendLogs, frontendLogs],
    );

    useEffect(() => {
        if (!isOpen) return;
        invoke<Array<Omit<ErrorLogEntry, 'id'>>>('cmd_get_transfer_logs')
            .then(items => {
                setBackendLogs(items.map((item, index) => ({
                    id: `backend-${item.time}-${index}`,
                    ...item,
                })));
            })
            .catch(e => {
                toast.error(humanizeError(e, i18n.t('errors.action_load_logs')));
            });
    }, [isOpen]);

    const formattedTerminalText = useMemo(() => {
        return logs.map(formatLogLine).join('\n\n');
    }, [logs]);

    const copyLogs = async () => {
        const text = formattedTerminalText || getErrorLogsText() || 'No logs recorded';
        try {
            await navigator.clipboard.writeText(text);
            toast.success('Diagnostic logs copied.');
        } catch (e) {
            toast.error(humanizeError(e, 'copy diagnostic logs'));
        }
    };

    const downloadLogs = () => {
        const text = formattedTerminalText || getErrorLogsText() || 'No logs recorded';
        const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = `telestash-diagnostics-${new Date().toISOString().replace(/[:.]/g, '-')}.txt`;
        anchor.click();
        URL.revokeObjectURL(url);
        toast.success('Diagnostic logs saved.');
    };

    const clearLogs = async () => {
        clearErrorLogs();
        setBackendLogs([]);
        try {
            await invoke('cmd_clear_transfer_logs');
        } catch (e) {
            toast.error(humanizeError(e, i18n.t('errors.action_clear_logs')));
        }
    };

    return (
        <AnimatePresence>
            {isOpen && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    className="fixed inset-0 bg-black/70 backdrop-blur-md z-[200] flex items-center justify-center p-4"
                    onClick={onClose}
                >
                    <motion.div
                        ref={dialogRef}
                        initial={{ scale: 0.95, opacity: 0 }}
                        animate={{ scale: 1, opacity: 1 }}
                        exit={{ scale: 0.95, opacity: 0 }}
                        transition={{ type: 'spring', damping: 25, stiffness: 300 }}
                        onClick={e => e.stopPropagation()}
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="logs-modal-title"
                        tabIndex={-1}
                        className="w-full max-w-4xl max-h-[85vh] bg-stash-surface border border-stash-border rounded-2xl shadow-2xl overflow-hidden flex flex-col"
                    >
                        {/* Terminal Header */}
                        <div className="px-5 py-3.5 border-b border-stash-border flex items-center justify-between bg-stash-hover select-none">
                            <div className="flex items-center gap-3">
                                <div className="flex items-center gap-2 text-stash-subtext">
                                    <Terminal className="w-4 h-4 text-stash-primary" />
                                    <h2 id="logs-modal-title" className="text-base font-semibold tracking-tight text-stash-text">Diagnostic logs</h2>
                                </div>
                                <span className="text-[10px] px-2 py-0.5 rounded-full bg-stash-hover text-stash-subtext font-normal">
                                    {logs.length} line{logs.length === 1 ? '' : 's'}
                                </span>
                            </div>
                            <button
                                ref={closeButtonRef}
                                onClick={onClose}
                                className="p-1.5 rounded-lg text-stash-subtext hover:text-stash-text hover:bg-stash-hover transition"
                                title="Close"
                                aria-label="Close logs"
                            >
                                <X className="w-4 h-4" />
                            </button>
                        </div>

                        {/* Terminal Stream View */}
                        <div className="p-4 flex-1 overflow-y-auto bg-stash-bg font-mono text-[12px] leading-relaxed text-stash-text whitespace-pre select-text">
                            {logs.length === 0 ? (
                                <div className="h-64 flex flex-col items-center justify-center text-center text-stash-subtext">
                                    <AlertTriangle className="w-7 h-7 mb-2 text-stash-subtext" />
                                    <p className="text-xs font-mono">System log stream empty</p>
                                </div>
                            ) : (
                                <div className="space-y-4">
                                    {logs.map((log) => (
                                        <div key={log.id} className="leading-snug">
                                            <div className="flex items-start flex-wrap gap-x-2 text-stash-text">
                                                <span className="text-stash-subtext">{formatTimestamp(log.time)}</span>
                                                <span className="text-stash-primary font-semibold">[{log.source}{log.category ? `/${log.category}` : ''}]</span>
                                                {log.level === 'info' ? (
                                                    <span className="text-sky-400 font-bold">[INFO]</span>
                                                ) : (
                                                    <span className="text-red-400 font-bold">[ERROR]</span>
                                                )}
                                                <span className="text-stash-text font-medium break-all">{log.message}</span>
                                            </div>
                                            {log.details && (
                                                <div className="mt-1 pl-4 border-l-2 border-stash-border text-stash-subtext text-[11px] whitespace-pre-wrap break-all">
                                                    {formatDetailsBlock(log.details)}
                                                </div>
                                            )}
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>

                        {/* Terminal Footer */}
                        <div className="px-5 py-3 border-t border-stash-border flex items-center justify-between bg-stash-hover">
                            <button
                                onClick={clearLogs}
                                disabled={logs.length === 0}
                                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs text-stash-subtext hover:text-red-400 hover:bg-red-500/10 transition font-medium disabled:opacity-30 disabled:pointer-events-none"
                            >
                                <Trash2 className="w-3.5 h-3.5" />
                                Clear Console
                            </button>
                            <div className="flex items-center gap-2">
                                <button
                                    onClick={copyLogs}
                                    disabled={logs.length === 0}
                                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border border-stash-border text-stash-text hover:bg-stash-hover transition disabled:opacity-30 disabled:pointer-events-none"
                                >
                                    <Copy className="w-3.5 h-3.5" />
                                    Copy
                                </button>
                                <button
                                    onClick={downloadLogs}
                                    disabled={logs.length === 0}
                                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium auth-primary transition disabled:opacity-30 disabled:pointer-events-none shadow-sm"
                                >
                                    <Download className="w-3.5 h-3.5" />
                                    Save .txt
                                </button>
                            </div>
                        </div>
                    </motion.div>
                </motion.div>
            )}
        </AnimatePresence>
    );
}

function formatTimestamp(isoTime: string): string {
    try {
        const d = new Date(isoTime);
        const pad = (n: number, width = 2) => String(n).padStart(width, '0');
        const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
        const time = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
        return `[${date} ${time}]`;
    } catch {
        return `[${isoTime}]`;
    }
}

function formatLogLine(log: ErrorLogEntry): string {
    const timeStr = formatTimestamp(log.time);
    const sourceStr = `[${log.source}${log.category ? `/${log.category}` : ''}]`;
    const line = `${timeStr} ${sourceStr} ${log.level === 'info' ? '[INFO]' : '[ERROR]'} ${log.message}`;
    if (!log.details) return line;

    const detailsIndented = log.details
        .split('\n')
        .map(l => `    │ ${l}`)
        .join('\n');
    return `${line}\n${detailsIndented}`;
}

function formatDetailsBlock(details: string): string {
    return details
        .split('\n')
        .map(line => `│ ${line}`)
        .join('\n');
}
