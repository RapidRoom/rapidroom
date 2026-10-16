import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { useLibraryStore } from './useLibraryStore';
import { useEditorStore } from './useEditorStore';
import { toast } from 'react-toastify';

export interface DenoiseMarkStatus {
  marked: boolean;
  denoised: boolean;
}
export interface DenoiseQueueSummary {
  completed: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
  errors: string[];
  outputs: string[];
}
interface QueueState {
  statuses: Record<string, DenoiseMarkStatus>;
  setupOpen: boolean;
  starting: boolean;
  jobId: number | null;
  progress: { current: number; total: number; path: string } | null;
  message: string;
  summary: DenoiseQueueSummary | null;
}
export const useDenoiseQueueStore = create<QueueState>(() => ({
  statuses: {},
  setupOpen: false,
  starting: false,
  jobId: null,
  progress: null,
  message: '',
  summary: null,
}));

export async function refreshDenoiseMarks(paths: string[]) {
  const statuses = await invoke<Record<string, DenoiseMarkStatus>>('denoise_mark_status', { paths });
  useDenoiseQueueStore.setState((s) => ({ statuses: { ...s.statuses, ...statuses } }));
  useLibraryStore.getState().setLibrary((s) => ({
    imageList: s.imageList.map((image) =>
      statuses[image.path]
        ? {
            ...image,
            denoise_marked: statuses[image.path].marked,
            denoised: statuses[image.path].denoised,
          }
        : image,
    ),
  }));
}

let markWrites: Promise<void> = Promise.resolve();

export function setDenoiseMark(marked?: boolean, paths?: string[]) {
  const { multiSelectedPaths } = useLibraryStore.getState();
  const selected = useEditorStore.getState().selectedImage;
  const targets = paths ?? (multiSelectedPaths.length ? multiSelectedPaths : selected ? [selected.path] : []);
  if (!targets.length) return Promise.resolve();
  markWrites = markWrites.then(async () => {
    try {
      const statuses = await invoke<Record<string, DenoiseMarkStatus>>('denoise_mark_status', { paths: targets });
      const next = marked ?? !targets.every((path) => statuses[path]?.marked);
      await invoke('set_denoise_mark', { paths: targets, marked: next });
      await refreshDenoiseMarks(targets);
    } catch (error) {
      toast.error(String(error));
    }
  });
  return markWrites;
}
