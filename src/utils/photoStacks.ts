import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import i18next from 'i18next';
import { ImageFile, Invokes, StackMember } from '../components/ui/AppProperties';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { expandGroupedPaths } from './imageGrouping';

const folder = (path: string) => path.substring(0, Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')));
export const stackKey = (image: ImageFile) => `${folder(image.path)}\0${image.stack?.id}`;

export function collapseStacks(images: ImageFile[], expanded: Set<string>): ImageFile[] {
  const buckets = new Map<string, ImageFile[]>();
  for (const image of images) {
    if (!image.stack) continue;
    const key = stackKey(image);
    const members = buckets.get(key) ?? [];
    members.push(image);
    buckets.set(key, members);
  }
  const emitted = new Set<string>();
  return images.flatMap((image) => {
    if (!image.stack) return [image];
    const key = stackKey(image);
    if (emitted.has(key)) return [];
    emitted.add(key);
    const members = buckets
      .get(key)!
      .sort((a, b) => a.stack!.position - b.stack!.position || a.path.localeCompare(b.path));
    return expanded.has(key) ? members : [members[0]];
  });
}

export function expandStackPaths(images: ImageFile[], paths: string[]): string[] {
  const keys = new Set(images.filter((i) => paths.includes(i.path) && i.stack).map(stackKey));
  return [...new Set([...paths, ...images.filter((i) => i.stack && keys.has(stackKey(i))).map((i) => i.path)])];
}

export function toggleStack(key: string) {
  useLibraryStore.getState().setLibrary((state) => {
    const expandedStacks = new Set(state.expandedStacks);
    if (expandedStacks.has(key)) expandedStacks.delete(key);
    else expandedStacks.add(key);
    return { expandedStacks };
  });
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
