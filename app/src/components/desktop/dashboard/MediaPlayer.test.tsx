import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MediaPlayer } from './MediaPlayer';

vi.mock('../../../utils/watchHistory', () => ({ recordWatchEvent: vi.fn() }));
afterEach(() => { cleanup(); vi.mocked(invoke).mockReset(); });

describe('media launch status', () => {
    it('opens a different folder file even when its message ID matches', async () => {
        vi.mocked(invoke).mockImplementation(async command => command === 'cmd_get_stream_info' ? { base_url: 'http://localhost', token: 'test' } : undefined);
        const file = { id: 7, folder_id: 10, name: 'film.mkv', size: 1, sizeStr: '1 B' };
        const { rerender } = render(<MediaPlayer file={file} onClose={() => {}} activeFolderId={10} />);
        await screen.findByText('Opened in player');
        vi.mocked(invoke).mockClear();
        rerender(<MediaPlayer file={{ ...file, folder_id: 20 }} onClose={() => {}} activeFolderId={20} />);
        await waitFor(() => expect(invoke).toHaveBeenCalledWith('cmd_play_in_mpv', expect.objectContaining({ folderId: 20 })));
    });

    it('reports the launch request without claiming to know playback state', async () => {
        let completeLaunch!: () => void;
        const launch = new Promise<void>(resolve => { completeLaunch = resolve; });
        vi.mocked(invoke)
            .mockResolvedValueOnce({ base_url: 'http://localhost', token: 'test' })
            .mockReturnValueOnce(launch);

        const onClose = vi.fn();
        render(<MediaPlayer file={{ id: 1, name: 'film.mkv', size: 1, sizeStr: '1 B' }} onClose={onClose} activeFolderId={null} />);

        expect(await screen.findByText('Opening player...')).toBeTruthy();
        await act(async () => { completeLaunch(); await launch; });
        expect(screen.getByText('Opened in player')).toBeTruthy();
        expect(screen.queryByText(/Playing .* natively/)).toBeNull();
        fireEvent.keyDown(document, { key: 'Escape' });
        expect(onClose).toHaveBeenCalledTimes(1);
    });
});
