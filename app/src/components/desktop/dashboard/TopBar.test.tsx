import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TopBar } from './TopBar';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../../errorLogs', () => ({ useErrorLogs: () => [] }));

describe('library view selector', () => {
  it('allows choosing each layout directly and reports the current layout', () => {
    const setViewMode = vi.fn();
    render(<TopBar currentFolderName="Movies" selectedIds={[]} onShowMoveModal={() => {}}
      onBulkDownload={() => {}} onBulkDelete={() => {}} onBulkShare={() => {}}
      onDownloadFolder={() => {}} onClearSelection={() => {}} viewMode="posters"
      setViewMode={setViewMode} searchTerm="" onSearchChange={() => {}} onLogsClick={() => {}} />);
    expect(screen.getByRole('button', { name: 'Poster view' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: 'List view' }));
    expect(setViewMode).toHaveBeenCalledWith('list');
  });
});
