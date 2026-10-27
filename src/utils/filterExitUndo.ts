import { createElement } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import FilterExitToast from '../components/ui/FilterExitToast';
import { ImageFlag, Invokes } from '../components/ui/AppProperties';
import { useLibraryStore } from '../store/useLibraryStore';
import { useEditorStore } from '../store/useEditorStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';
import { computeSortedLibrary } from '../hooks/useSortedLibrary';
import { afterPendingLibraryMetadataWrites, enqueueLibraryMetadataWrite } from './libraryMetadataWrites';

interface MetadataSnapshot {
  rating: number;
  flag: ImageFlag | null;
  color: string | null;
}
type Snapshot = Map<string, MetadataSnapshot>;
type Field = keyof MetadataSnapshot;
const TOAST_ID = 'filter-exit';

export function snapshotLibraryMetadata(paths: string[]): Snapshot {
  const { imageRatings, imageList } = useLibraryStore.getState();
  const images = new Map(imageList.map((image) => [image.path, image]));
  return new Map(
    paths.map((path) => {
      const image = images.get(path);
      return [
        path,
        {
          rating: imageRatings[path] ?? image?.rating ?? 0,
          flag: image?.flag ?? null,
          color: image?.tags?.find((tag) => tag.startsWith('color:'))?.slice(6) ?? null,
        },
      ];
    }),
  );
}

function restoreFields(snapshot: Snapshot, fields: Field[], ratingWrite = false) {
  useLibraryStore.getState().setLibrary((state) => {
    const imageRatings = { ...state.imageRatings };
    if (fields.includes('rating'))
      snapshot.forEach((value, path) => {
        imageRatings[path] = value.rating;
      });
    return {
      imageRatings,
      imageList: state.imageList.map((image) => {
        const value = snapshot.get(image.path);
        if (!value) return image;
        const tags = (image.tags ?? []).filter((tag) => !tag.startsWith('color:'));
        if (value.color) tags.push(`color:${value.color}`);
        return {
          ...image,
          flag: fields.includes('flag')
            ? value.flag
            : ratingWrite && value.rating > 0 && image.flag === ImageFlag.Reject
              ? null
              : image.flag,
          tags: fields.includes('color') ? tags : image.tags,
        };
      }),
    };
  });
}

function restoreMetadata(snapshot: Snapshot) {
  const current = snapshotLibraryMetadata([...snapshot.keys()]);
  for (const field of ['rating', 'flag', 'color'] as const) {
    const groups = new Map<MetadataSnapshot[typeof field], string[]>();
    snapshot.forEach((value, path) => {
      // A positive rating clears Reject on disk even when its old value was unchanged.
      const resendReject =
        field === 'flag' &&
        value.flag === ImageFlag.Reject &&
        value.rating > 0 &&
        value.rating !== current.get(path)?.rating;
      if (value[field] === current.get(path)?.[field] && !resendReject) return;
      groups.set(value[field], [...(groups.get(value[field]) ?? []), path]);
    });
    groups.forEach((paths, value) => {
      const desired = new Map(paths.map((path) => [path, snapshot.get(path)!]));
      enqueueLibraryMetadataWrite({
        apply: () => {
          const before = snapshotLibraryMetadata(paths);
          restoreFields(desired, [field], field === 'rating');
          return () => restoreFields(before, field === 'rating' ? ['rating', 'flag'] : [field]);
        },
        save: () =>
          field === 'rating'
            ? invoke(Invokes.SetRatingForPaths, { paths, rating: value })
            : field === 'flag'
              ? invoke(Invokes.SetFlagForPaths, { paths, flag: value })
              : invoke(Invokes.SetColorLabelForPaths, { paths, color: value }),
        onError: (error) => {
          console.error(error);
          toast.error(`Failed to undo: ${error}`);
        },
      });
    });
  }
}

export function showFilterExitUndo(activePath: string, count: number, snapshot: Snapshot, selection: string[]) {
  const content = createElement(FilterExitToast, {
    fileName: activePath.split('?vc=')[0].split(/[\\/]/).pop() ?? '',
    count,
    onUndo: () => {
      toast.dismiss(TOAST_ID);
      const view = useUIStore.getState().activeView;
      const active =
        view === 'editor'
          ? useEditorStore.getState().selectedImage?.path
          : useLibraryStore.getState().libraryActivePath;
      afterPendingLibraryMetadataWrites(() => {
        restoreMetadata(snapshot);
        afterPendingLibraryMetadataWrites(() => {
          const current =
            view === 'editor'
              ? useEditorStore.getState().selectedImage?.path
              : useLibraryStore.getState().libraryActivePath;
          if (
            useUIStore.getState().activeView !== view ||
            useUIStore.getState().cullingModalState.isOpen ||
            current !== active
          )
            return;
          const visible = computeSortedLibrary(useLibraryStore.getState(), useSettingsStore.getState());
          const paths = new Set(visible.map((image) => image.path));
          if (!paths.has(activePath)) return;
          useLibraryStore.getState().setLibrary({
            multiSelectedPaths: selection.includes(activePath)
              ? selection.filter((path) => paths.has(path))
              : [activePath],
            libraryActivePath: activePath,
            selectionAnchorPath: activePath,
          });
          useUIStore.getState().imageSelectHandler?.(activePath, view === 'editor');
        });
      });
    },
  });
  if (toast.isActive(TOAST_ID)) toast.update(TOAST_ID, { render: content, autoClose: 5000 });
  else toast(content, { toastId: TOAST_ID });
}
