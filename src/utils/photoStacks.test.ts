import { describe, expect, it, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { ImageFile } from '../components/ui/AppProperties';
import { buildImageGroups } from './imageGrouping';
import { collapseStacks, expandStackPaths, stackKey } from './photoStacks';

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
  it('shows a matching member when filtering excludes the stored cover', () => {
    const member = image('/photos/member.jpg', 1);
    expect(collapseStacks([member], new Set())).toEqual([member]);
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
