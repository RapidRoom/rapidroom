import { advanceWhenSaved } from '../hooks/useLibraryActions';
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
  setupOpen: boolean;
  starting: boolean;
  jobId: number | null;
  progress: { current: number; total: number; path: string } | null;
  message: string;
  summary: DenoiseQueueSummary | null;
}
export const useDenoiseQueueStore = create<QueueState>(() => ({
  setupOpen: false,
  starting: false,
  jobId: null,
  progress: null,
  message: '',
  summary: null,
}));

let statusGeneration = 0;
const latestStatusRequest = new Map<string, number>();

export async function refreshDenoiseMarks(paths: string[]) {
  const generation = ++statusGeneration;
  for (const path of paths) latestStatusRequest.set(path, generation);
  const statuses = await invoke<Record<string, DenoiseMarkStatus>>('denoise_mark_status', { paths });
  const latest = Object.fromEntries(
    Object.entries(statuses).filter(([path]) => latestStatusRequest.get(path) === generation),
  );
  useLibraryStore.getState().setLibrary((state) => ({ denoiseStatuses: { ...state.denoiseStatuses, ...latest } }));
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
      const { onSaved } = advanceWhenSaved(targets);
      await invoke('set_denoise_mark', { paths: targets, marked: next });
      await refreshDenoiseMarks(targets);
      onSaved?.();
    } catch (error) {
      toast.error(String(error));
    }
  });
  return markWrites;
}
