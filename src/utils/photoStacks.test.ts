import { describe, expect, it, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { ImageFile } from '../components/ui/AppProperties';
import { buildImageGroups } from './imageGrouping';
import { collapseStacks, expandStackPaths, stackKey, toggleStack } from './photoStacks';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';

const image = (path: string, position?: number, id = 'burst'): ImageFile =>
  ({
    path,
    stack: position === undefined ? null : { id, position },
    group_id: '/photos/pair',
    is_raw: path.endsWith('.raw'),
  }) as ImageFile;

describe('photo stacks shared by grid and filmstrip', () => {
  it('uses cover position and emits expanded members together in stored order', () => {
    const member = image('/photos/member.jpg', 1);
    const separate = image('/photos/separate.jpg');
    const cover = image('/photos/cover.raw', 0);
    const images = [member, separate, cover];
    expect(collapseStacks(images, new Set()).map((i) => i.path)).toEqual([cover.path, separate.path]);
    expect(collapseStacks(images, new Set([stackKey(cover)])).map((i) => i.path)).toEqual([
      cover.path,
      member.path,
      separate.path,
    ]);
  });
  it('keeps stacks distinct when a copied folder has the same IDs', () => {
    const a = image('/a/cover.raw', 0);
    const b = image('/a/member.jpg', 1);
    const c = image('/b/cover.raw', 0);
    expect(collapseStacks([a, b, c], new Set())).toEqual([a, c]);
    expect(expandStackPaths([a, b, c], [a.path])).toEqual([a.path, b.path]);
  });
  it('hides a collapsed stack whose cover fails the filter instead of showing another member', () => {
    const cover = { ...image('/photos/cover.jpg', 0), flag: 'pick' } as ImageFile;
    const member = image('/photos/member.jpg', 1);
    const all = [member, cover];
    // Unrated filter after rating the cover: only the unrated member passes.
    const unratedMembersOnly = [member];
    expect(collapseStacks(unratedMembersOnly, new Set(), all)).toEqual([]);
    expect(collapseStacks(all, new Set(), all)).toEqual([cover]);
    // An expanded stack follows the filter as normal.
    expect(collapseStacks(unratedMembersOnly, new Set([stackKey(cover)]), all)).toEqual([member]);
  });
  it('preserves explicit cover and expanded members with RAW+JPEG grouping enabled', () => {
    const raw = image('/photos/pair.raw', 1);
    const jpeg = image('/photos/pair.jpg', 0);
    const groups = buildImageGroups([raw, jpeg], 'raw');
    expect(collapseStacks(groups.displayList, new Set())).toEqual([jpeg]);
    expect(collapseStacks(groups.displayList, new Set([stackKey(jpeg)]))).toEqual([jpeg, raw]);
  });
});

it('does not hide or expand legacy virtual copies carrying a physical stack ID', () => {
  const physical = image('/photos/cover.raw', 0);
  const copy = { ...image('/photos/cover.raw?vc=copy', 0), is_virtual_copy: true };
  expect(collapseStacks([physical, copy], new Set())).toEqual([physical, copy]);
  expect(expandStackPaths([physical, copy], [copy.path])).toEqual([copy.path]);
  expect(expandStackPaths([physical, copy], [physical.path])).toEqual([physical.path]);
});

it('moves the selection to the cover when the active member is hidden by collapsing', () => {
  const cover = image('/photos/cover.jpg', 0);
  const member = image('/photos/member.jpg', 1);
  const key = stackKey(cover);
  useUIStore.getState().setUI({ activeView: 'library' });
  useLibraryStore.getState().setLibrary({
    imageList: [cover, member],
    expandedStacks: new Set([key]),
    libraryActivePath: member.path,
    multiSelectedPaths: [member.path],
  });
  toggleStack(key);
  const state = useLibraryStore.getState();
  expect(state.expandedStacks.has(key)).toBe(false);
  expect(state.libraryActivePath).toBe(cover.path);
  expect(state.multiSelectedPaths).toEqual([cover.path]);
});
