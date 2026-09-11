import { useEffect, useState } from 'react';
import { X, Plus, Check, Trash2 } from '../../shared/icons.tsx';
import { TelegramFile } from '../../../types';
import { useTags, useTagMutations, FileTag } from '../../../hooks/useFileTags';

interface TagEditorModalProps {
    file: TelegramFile;
    folderId: number | null;
    onClose: () => void;
}

export function TagEditorModal({ file, folderId, onClose }: TagEditorModalProps) {
    const { data: tags = [] } = useTags();
    const { createTag, deleteTag, setFileTags, getFileTagIds } = useTagMutations();
    const [selected, setSelected] = useState<Set<number>>(new Set());
    const [newName, setNewName] = useState('');
    const [busy, setBusy] = useState(false);

    useEffect(() => {
        let cancelled = false;
        getFileTagIds(folderId, file.id)
            .then((ids) => {
                if (!cancelled) setSelected(new Set(ids));
            })
            .catch(console.error);
        return () => {
            cancelled = true;
        };
    }, [folderId, file.id, getFileTagIds]);

    const toggle = (id: number) => {
        setSelected((prev) => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
        });
    };

    const handleSave = async () => {
        setBusy(true);
        try {
            await setFileTags(folderId, file.id, Array.from(selected));
            onClose();
        } catch (err) {
            console.error(err);
        } finally {
            setBusy(false);
        }
    };

    const handleCreate = async () => {
        const name = newName.trim();
        if (!name) return;
        setBusy(true);
        try {
            const tag = await createTag(name);
            setSelected((prev) => new Set(prev).add(tag.id));
            setNewName('');
        } catch (err) {
            console.error(err);
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onClose}>
            <div
                className="w-full max-w-md bg-stash-surface border border-stash-border rounded-xl p-4 shadow-xl"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex items-center justify-between mb-3">
                    <div className="min-w-0">
                        <h3 className="text-sm font-semibold text-stash-text truncate">Tags</h3>
                        <p className="text-xs text-stash-subtext truncate" title={file.name}>{file.name}</p>
                    </div>
                    <button onClick={onClose} className="p-1 rounded hover:bg-stash-hover text-stash-subtext" aria-label="Close">
                        <X className="w-4 h-4" />
                    </button>
                </div>

                <div className="flex flex-wrap gap-2 mb-3 min-h-[2rem]">
                    {tags.length === 0 && (
                        <span className="text-xs text-stash-subtext">No tags yet — create one below.</span>
                    )}
                    {tags.map((tag: FileTag) => (
                        <button
                            key={tag.id}
                            type="button"
                            onClick={() => toggle(tag.id)}
                            className={`inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs border transition-colors ${
                                selected.has(tag.id)
                                    ? 'bg-stash-primary/20 border-stash-primary text-stash-primary'
                                    : 'border-stash-border text-stash-subtext hover:text-stash-text'
                            }`}
                        >
                            {selected.has(tag.id) && <Check className="w-3 h-3" />}
                            {tag.name}
                        </button>
                    ))}
                </div>

                <div className="flex gap-2 mb-3">
                    <input
                        value={newName}
                        onChange={(e) => setNewName(e.target.value)}
                        onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
                        placeholder="New tag name"
                        className="flex-1 bg-white/5 border border-stash-border rounded px-2 py-1.5 text-sm text-stash-text focus:outline-none focus:ring-1 focus:ring-stash-primary"
                    />
                    <button
                        onClick={handleCreate}
                        disabled={busy || !newName.trim()}
                        className="px-3 py-1.5 rounded bg-stash-primary/20 text-stash-primary text-xs font-medium hover:bg-stash-primary/30 disabled:opacity-50 flex items-center gap-1"
                    >
                        <Plus className="w-3 h-3" /> Add
                    </button>
                </div>

                {tags.length > 0 && (
                    <div className="mb-3 max-h-28 overflow-y-auto space-y-1">
                        {tags.map((tag) => (
                            <div key={tag.id} className="flex items-center justify-between text-xs text-stash-subtext">
                                <span className="truncate">{tag.name}</span>
                                <button
                                    onClick={async () => {
                                        if (selected.has(tag.id)) selected.delete(tag.id);
                                        await deleteTag(tag.id);
                                    }}
                                    className="p-1 rounded hover:bg-red-500/10 text-red-400/70 hover:text-red-400"
                                    title={`Delete tag ${tag.name}`}
                                >
                                    <Trash2 className="w-3 h-3" />
                                </button>
                            </div>
                        ))}
                    </div>
                )}

                <div className="flex justify-end gap-2">
                    <button onClick={onClose} className="px-3 py-1.5 rounded text-xs text-stash-subtext hover:text-stash-text">
                        Cancel
                    </button>
                    <button
                        onClick={handleSave}
                        disabled={busy}
                        className="px-3 py-1.5 rounded bg-stash-primary text-black text-xs font-semibold hover:bg-stash-primary/90 disabled:opacity-50"
                    >
                        Save
                    </button>
                </div>
            </div>
        </div>
    );
}
