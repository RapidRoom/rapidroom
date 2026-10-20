import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLibraryStore } from '../store/useLibraryStore';
import { useProcessStore } from '../store/useProcessStore';
import {
  CullingPreviewQueue,
  cullingPreviewWindow,
  cullingPreviewKey,
  type CullingPreviewRequest,
} from '../utils/cullingPreviewQueue';

// Serialize cancellation across remounts too; an old view's late batch must not cancel a new view.
let batches: Promise<number> = Promise.resolve(0);

function createQueue(reportFailure: (path: string) => void): CullingPreviewQueue {
  return new CullingPreviewQueue({
    beginBatch: () => {
      batches = batches.catch(() => 0).then(() => invoke<number>('begin_culling_preview_batch'));
      return batches;
    },
    cached: ({ path, thumbKey }) => {
      const state = useProcessStore.getState();
      const cached = state.previews[path];
      if (!cached || cached.thumbKey !== thumbKey) return false;
      state.setPreview(path, cached.url, thumbKey, cached.dimensions);
      return true;
    },
    loadAdjustments: async (path) => {
      const metadata = await invoke<{ adjustments: Record<string, unknown> | null }>('load_metadata', { path });
      return metadata.adjustments && !metadata.adjustments.is_null ? metadata.adjustments : {};
    },
    render: (path, jsAdjustments, generation) =>
      invoke<Uint8Array>('generate_culling_preview_for_path', { path, jsAdjustments, generation }),
    prepare: async (bytes) => {
      const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }));
      try {
        const image = new Image();
        image.src = url;
        await image.decode();
        if (!image.naturalWidth || !image.naturalHeight) throw new Error('Empty culling preview');
        return { url, width: image.naturalWidth, height: image.naturalHeight };
      } catch (error) {
        URL.revokeObjectURL(url);
        throw error;
      }
    },
    commit: ({ path, thumbKey }, preview) => {
      const state = useProcessStore.getState();
      if (cullingPreviewKey(state.mediumThumbnails[path] || state.thumbnails[path] || '') !== thumbKey) return false;
      state.setPreview(
        path,
        preview.url,
        thumbKey,
        { width: preview.width, height: preview.height },
        useLibraryStore.getState().multiSelectedPaths.slice(-6),
      );
      return true;
    },
    release: (url) => URL.revokeObjectURL(url),
    failed: (path, error) => {
      console.error('Could not prepare culling preview:', path, error);
      reportFailure(path);
    },
  });
}

export function useCullingPreviewPrefetch(
  paths: string[],
  visible: string[],
  active: string | null,
): Record<string, string> {
  const medium = useProcessStore((state) => state.mediumThumbnails);
  const small = useProcessStore((state) => state.thumbnails);
  const signature = JSON.stringify(
    cullingPreviewWindow(paths, visible, active).map((path) => ({
      path,
      thumbKey: cullingPreviewKey(medium[path] || small[path] || ''),
    })),
  );
  const [failures, setFailures] = useState<Record<string, string>>({});
  const queue = useMemo(
    () =>
      createQueue((path) => {
        const state = useProcessStore.getState();
        const key = cullingPreviewKey(state.mediumThumbnails[path] || state.thumbnails[path] || '');
        setFailures((prior) => ({ ...prior, [path]: key }));
      }),
    [],
  );
  useEffect(() => {
    const requests = JSON.parse(signature) as CullingPreviewRequest[];
    setFailures((prior) =>
      Object.fromEntries(
        Object.entries(prior).filter(([path, key]) =>
          requests.some((request) => request.path === path && request.thumbKey === key),
        ),
      ),
    );
    queue.update(requests);
  }, [queue, signature]);
  useEffect(() => () => queue.stop(), [queue]);
  return failures;
}
