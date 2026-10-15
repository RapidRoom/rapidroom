import { describe, expect, it } from 'vitest';
import { getVisibleImagePaths } from './LibraryGrid';

describe('getVisibleImagePaths', () => {
  it('flattens visible image rows in folder order and omits collapsed groups', () => {
    expect(
      getVisibleImagePaths([
        { type: 'header' },
        { type: 'images', images: [{ path: '/photos/current/IMG_1821.jpg' }, { path: '/photos/current/MG_8355.jpg' }] },
        { type: 'header' },
        { type: 'footer' },
      ]),
    ).toEqual(['/photos/current/IMG_1821.jpg', '/photos/current/MG_8355.jpg']);
  });
});
