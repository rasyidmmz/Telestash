import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SidebarItem } from './SidebarItem';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('@dnd-kit/sortable', () => ({
  useSortable: () => ({ attributes: {}, listeners: {}, setNodeRef: () => {}, transform: null, transition: undefined, isDragging: false }),
}));

describe('folder presentation menu', () => {
  it('assigns a folder type without navigating or changing the folder', () => {
    const onSetFolderKind = vi.fn();
    const onClick = vi.fn();
    render(<SidebarItem icon={() => null} label="A series" active={false} folderId={12}
      onClick={onClick} onDrop={() => {}} onDelete={() => {}}
      folderKind="general" onSetFolderKind={onSetFolderKind} />);

    fireEvent.contextMenu(screen.getByText('A series'));
    fireEvent.click(screen.getByRole('button', { name: 'Series' }));
    expect(onSetFolderKind).toHaveBeenCalledWith('series');
    expect(onClick).not.toHaveBeenCalled();
  });
});
