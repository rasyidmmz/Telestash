import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Sidebar } from './Sidebar';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../../context/SettingsContext', () => ({ useSettings: () => ({
  settings: { sidebarCollapsed: false, hideGroups: true, folderPresentation: {
    '10': { kind: 'series' }, '20': { kind: 'movies' },
  } }, updateSetting: vi.fn(), updateFolderPresentation: vi.fn(),
}) }));
vi.mock('@dnd-kit/core', () => ({
  DndContext: ({ children }: { children: React.ReactNode }) => children,
  closestCenter: () => null, KeyboardSensor: class {}, PointerSensor: class {},
  useSensor: () => ({}), useSensors: () => [],
}));
vi.mock('@dnd-kit/sortable', () => ({
  SortableContext: ({ children }: { children: React.ReactNode }) => children,
  useSortable: () => ({ attributes: {}, listeners: {}, setNodeRef: () => {}, transform: null, transition: undefined, isDragging: false }),
  arrayMove: (items: unknown[]) => items, sortableKeyboardCoordinates: () => null,
  verticalListSortingStrategy: () => null, horizontalListSortingStrategy: () => null,
}));

describe('library navigation', () => {
  it('shows manually tagged series and movie folders separately from unassigned folders', () => {
    const asyncAction = async () => {};
    render(<Sidebar folders={[{ id: 10, name: 'Show' }, { id: 20, name: 'Films' }, { id: 30, name: 'Documents' }]}
      groups={[]} activeFolderId={10} setActiveFolderId={() => {}} onDrop={() => {}}
      onDelete={() => {}} onRename={() => {}} onToggleVisibility={() => {}} onExportInvite={() => {}}
      onCreate={asyncAction} isSyncing={false} isConnected onSync={() => {}} onLogout={() => {}} bandwidth={null}
      onAssignFolderToGroup={asyncAction} onReorderFolders={asyncAction} onUpdateGroupOrder={asyncAction}
      onCreateGroup={asyncAction} onUpdateGroup={asyncAction} onDeleteGroup={asyncAction} />);
    expect(within(screen.getByRole('region', { name: 'Series' })).getByText('Show')).toBeTruthy();
    expect(within(screen.getByRole('region', { name: 'Movies' })).getByText('Films')).toBeTruthy();
    expect(within(screen.getByRole('region', { name: 'General' })).getByText('Documents')).toBeTruthy();
  });
});
