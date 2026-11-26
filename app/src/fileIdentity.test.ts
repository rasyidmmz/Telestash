import { describe, expect, it } from 'vitest';
import { fileKey, selectedFiles, groupFilesByFolder } from './fileIdentity';

const files = [
  { id: 7, folder_id: 10, name: 'One.mkv', size: 1, sizeStr: '1 B' },
  { id: 7, folder_id: 20, name: 'Two.mkv', size: 1, sizeStr: '1 B' },
];

describe('file identity', () => {
  it('selects only the file in the selected folder when message IDs collide', () => {
    expect(fileKey(files[0])).not.toBe(fileKey(files[1]));
    expect(selectedFiles(files, ['20:7'])).toEqual([files[1]]);
  });
  it('groups actions by their real source folders, including Saved Messages', () => {
    const groups = groupFilesByFolder([...files, { ...files[0], folder_id: null }]);
    expect([...groups.keys()]).toEqual([10, 20, null]);
  });
});
