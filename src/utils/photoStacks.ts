import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import i18next from 'i18next';
import { ImageFile, Invokes, StackMember } from '../components/ui/AppProperties';
import { useLibraryStore } from '../store/useLibraryStore';
import { useEditorStore } from '../store/useEditorStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';
import { expandGroupedPaths } from './imageGrouping';

const folder = (path: string) => path.substring(0, Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')));
export const isStackedPhoto = (image: ImageFile) =>
  !!image.stack && !image.is_virtual_copy && !image.path.includes('?vc=');
export const stackKey = (image: ImageFile) => `${folder(image.path)}\0${image.stack?.id}`;

const byPosition = (a: ImageFile, b: ImageFile) =>
  a.stack!.position - b.stack!.position || a.path.localeCompare(b.path);

interface StackBadgeInfo {
  key: string;
  count: number;
}
const indexes = new WeakMap<ImageFile[], { members: Map<string, ImageFile[]>; badges: Map<string, StackBadgeInfo> }>();
function stackIndex(images: ImageFile[]) {
  const existing = indexes.get(images);
  if (existing) return existing;
  const members = new Map<string, ImageFile[]>();
  for (const image of images) {
    if (!isStackedPhoto(image)) continue;
    const key = stackKey(image);
    const group = members.get(key) ?? [];
    group.push(image);
    members.set(key, group);
  }
  const badges = new Map<string, StackBadgeInfo>();
  for (const [key, group] of members) {
    group.sort(byPosition);
    const info = { key, count: group.length };
    for (const image of group) badges.set(image.path, info);
  }
  const index = { members, badges };
  indexes.set(images, index);
  return index;
}

// Shared by every badge: one index build per immutable image-list update,
// then constant-time selectors rather than a full scan on every row render.
export function stackBadgeInfo(images: ImageFile[], path: string) {
  return stackIndex(images).badges.get(path);
}

// `all` is the unfiltered list. A collapsed stack is represented by its stored
// cover (lowest position among ALL members) and is hidden when the cover does
// not pass the filter, so rating or flagging the cover never makes a hidden
// member appear. Expanded stacks show whichever members pass the filter.
export function collapseStacks(images: ImageFile[], expanded: Set<string>, all: ImageFile[] = images): ImageFile[] {
  const covers = stackIndex(all).members;
  const buckets = new Map<string, ImageFile[]>();
  for (const image of images) {
    if (!isStackedPhoto(image)) continue;
    const key = stackKey(image);
    const members = buckets.get(key) ?? [];
    members.push(image);
    buckets.set(key, members);
  }
  const emitted = new Set<string>();
  return images.flatMap((image) => {
    if (!isStackedPhoto(image)) return [image];
    const key = stackKey(image);
    if (emitted.has(key)) return [];
    emitted.add(key);
    const members = buckets.get(key)!.sort(byPosition);
    if (expanded.has(key)) return members;
    const cover = members.find((m) => m.path === covers.get(key)?.[0]?.path);
    return cover ? [cover] : [];
  });
}

export function expandStackPaths(images: ImageFile[], paths: string[]): string[] {
  const index = stackIndex(images);
  const keys = new Set(paths.map((path) => index.badges.get(path)?.key).filter((key): key is string => !!key));
  return [...new Set([...paths, ...[...keys].flatMap((key) => index.members.get(key)!.map((image) => image.path))])];
}

export function toggleStack(key: string) {
  const library = useLibraryStore.getState();
  const ui = useUIStore.getState();
  let editorCover: string | undefined;
  if (library.expandedStacks.has(key)) {
    const members = stackIndex(library.imageList).members.get(key) ?? [];
    const cover = members[0];
    const paths = new Set(members.map((image) => image.path));
    const selected =
      ui.activeView === 'editor'
        ? [useEditorStore.getState().selectedImage?.path]
        : [library.libraryActivePath, ...library.multiSelectedPaths];
    const touched = selected.some((path) => path && path !== cover?.path && paths.has(path));
    if (cover && touched) {
      library.setLibrary({
        libraryActivePath: cover.path,
        multiSelectedPaths: [cover.path],
        selectionAnchorPath: cover.path,
      });
      if (ui.activeView === 'editor') editorCover = cover.path;
    }
  }
  useLibraryStore.getState().setLibrary((state) => {
    const expandedStacks = new Set(state.expandedStacks);
    if (expandedStacks.has(key)) expandedStacks.delete(key);
    else expandedStacks.add(key);
    return { expandedStacks };
  });
  // Use the normal navigation path: it flushes the old draft and loads the
  // new photo, instead of only changing the filmstrip's active highlight.
  if (editorCover) ui.imageSelectHandler?.(editorCover, true);
}

export interface StackUpdate {
  path: string;
  stack: StackMember | null;
}
export function acceptStackUpdates(updates: StackUpdate[]) {
  const byPath = new Map(updates.map((u) => [u.path, u.stack]));
  useLibraryStore.getState().setLibrary((state) => ({
    imageList: state.imageList.map((i) => (byPath.has(i.path) ? { ...i, stack: byPath.get(i.path) } : i)),
  }));
}

// Serialize stack gestures; publish only after sidecars have been saved.
let stackQueue: Promise<void> = Promise.resolve();
export function editStack(paths: string[], action: 'stack' | 'unstack' | 'cover'): Promise<void> {
  const selected = [...paths];
  const run = async () => {
    const { imageList } = useLibraryStore.getState();
    const grouping = useSettingsStore.getState().appSettings?.grouping ?? 'off';
    const targets = action === 'stack' ? expandGroupedPaths(imageList, selected, grouping) : selected;
    const updates = await invoke<StackUpdate[]>(Invokes.EditPhotoStack, { paths: targets, action });
    acceptStackUpdates(updates);
    if (action !== 'unstack' && selected.length) {
      useLibraryStore.getState().setLibrary({ multiSelectedPaths: [selected[0]], libraryActivePath: selected[0] });
    }
  };
  stackQueue = stackQueue.then(run).catch((error) => {
    toast.error(i18next.t('stacks.failed', { error: String(error) }));
  });
  return stackQueue;
}

export async function stackMergedResult(result: string, sources: string[]) {
  if (!useLibraryStore.getState().stackMergedResults) return;
  try {
    const updates = await invoke<StackUpdate[]>(Invokes.EditPhotoStack, {
      paths: [result, ...sources],
      action: 'stack',
    });
    acceptStackUpdates(updates);
  } catch (error) {
    toast.warning(i18next.t('stacks.mergeSaved', { error: String(error) }));
  }
}
