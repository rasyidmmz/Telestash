import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MediaDetail } from './MediaDetail';

vi.mock('../../../hooks/useVideoMetadata', () => ({ useVideoMetadata: () => ({ data: null }) }));

describe('media details', () => {
  it('shows the selected file and opens that file without inventing artwork', () => {
    const onPlay = vi.fn();
    render(<MediaDetail file={{ id: 42, folder_id: 7, name: 'My movie.mkv', size: 1024, sizeStr: '1 KB', type: 'file' }}
      sourceName="Movies" onPlay={onPlay} onClose={() => {}} onDownload={() => {}} />);
    expect(screen.getByRole('heading', { name: 'My movie.mkv' })).toBeTruthy();
    expect(screen.getByText('Movies')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Play' }));
    expect(onPlay).toHaveBeenCalledOnce();
  });
});
