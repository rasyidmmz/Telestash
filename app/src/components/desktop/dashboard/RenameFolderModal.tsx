import { useState, useRef, useEffect } from 'react';
import { Pencil, X } from '@phosphor-icons/react';
import { useTranslation } from 'react-i18next';

interface RenameFolderModalProps {
    folderId: number;
    currentName: string;
    onRename: (folderId: number, oldName: string, newName: string) => Promise<void>;
    onClose: () => void;
}

export function RenameFolderModal({ folderId, currentName, onRename, onClose }: RenameFolderModalProps) {
    const [name, setName] = useState(currentName);
    const [isSubmitting, setIsSubmitting] = useState(false);
    const inputRef = useRef<HTMLInputElement>(null);
    const { t } = useTranslation();

    useEffect(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
    }, []);

    const handleSubmit = async () => {
        if (isSubmitting) return;
        const trimmed = name.trim();
        if (!trimmed || trimmed === currentName) {
            onClose();
            return;
        }
        setIsSubmitting(true);
        try {
            await onRename(folderId, currentName, trimmed);
            onClose();
        } catch {
            // error handled by parent
            setIsSubmitting(false);
        }
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter') {
            e.preventDefault();
            handleSubmit();
        } else if (e.key === 'Escape') {
            onClose();
        }
    };

    return (
        <div
            className="fixed inset-0 z-[250] flex items-center justify-center bg-black/50 backdrop-blur-sm"
            onClick={onClose}
        >
            <div
                className="bg-stash-surface border border-stash-border rounded-xl w-[360px] shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-150"
                onClick={e => e.stopPropagation()}
            >
                {/* Header */}
                <div className="p-4 border-b border-stash-border flex items-center justify-between">
                    <h3 className="text-stash-text font-medium flex items-center gap-2">
                        <Pencil className="w-4 h-4 text-blue-400" />
                        {t('files.rename_folder')}
                    </h3>
                    <button
                        onClick={onClose}
                        className="text-stash-subtext hover:text-stash-text transition-colors"
                        disabled={isSubmitting}
                    >
                        <X className="w-4 h-4" />
                    </button>
                </div>

                {/* Body */}
                <div className="p-4 space-y-3">
                    <div className="text-sm text-stash-subtext">
                        {t('files.enter_new_name', { name: currentName })}
                    </div>
                    <input
                        ref={inputRef}
                        type="text"
                        value={name}
                        onChange={e => setName(e.target.value)}
                        onKeyDown={handleKeyDown}
                        maxLength={100}
                        className="w-full bg-stash-bg border border-stash-border rounded-lg px-3 py-2 text-sm text-stash-text placeholder:text-stash-subtext/50 focus:outline-none focus:ring-2 focus:ring-stash-primary/50 focus:border-stash-primary/50 transition-all"
                        placeholder={t('files.folder_name')}
                        disabled={isSubmitting}
                    />
                </div>

                {/* Footer */}
                <div className="p-4 border-t border-stash-border flex justify-end gap-2 bg-stash-hover/10">
                    <button
                        onClick={onClose}
                        className="px-4 py-2 text-sm font-medium text-stash-subtext hover:text-stash-text bg-stash-hover/50 hover:bg-stash-hover rounded-lg transition-colors"
                        disabled={isSubmitting}
                    >
                        {t('common.cancel')}
                    </button>
                    <button
                        onClick={handleSubmit}
                        disabled={isSubmitting || !name.trim() || name.trim() === currentName}
                        className="px-4 py-2 text-sm font-medium text-white bg-stash-primary hover:bg-stash-primary/90 disabled:opacity-40 disabled:cursor-not-allowed rounded-lg transition-colors"
                    >
                        {isSubmitting ? t('files.renaming') : t('files.rename')}
                    </button>
                </div>
            </div>
        </div>
    );
}
