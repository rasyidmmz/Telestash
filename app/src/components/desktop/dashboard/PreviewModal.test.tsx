import { fireEvent, render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { describe, expect, it, vi } from 'vitest';
import { PreviewModal } from './PreviewModal';

describe('preview dialog', () => {
    it('shows available navigation and closes once on Escape', async () => {
        vi.mocked(invoke).mockResolvedValue('http://localhost/image');
        const onClose = vi.fn();
        const onNext = vi.fn();
        render(<PreviewModal file={{ id: 2, name: 'photo.png', size: 1, sizeStr: '1 B' }} onClose={onClose} onNext={onNext} activeFolderId={null} />);

        expect(await screen.findByAltText('Preview of photo.png')).toBeTruthy();
        expect(screen.queryByRole('button', { name: 'Previous file' })).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: 'Next file' }));
        expect(onNext).toHaveBeenCalledTimes(1);
        fireEvent.keyDown(document, { key: 'Escape' });
        expect(onClose).toHaveBeenCalledTimes(1);
    });
});
