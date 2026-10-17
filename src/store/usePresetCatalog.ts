import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import debounce from 'lodash.debounce';
import type { UserPreset } from '../hooks/usePresets';

interface CatalogState {
  presets: UserPreset[];
  isLoading: boolean;
  loaded: boolean;
}

export const usePresetCatalog = create<CatalogState>(() => ({ presets: [], isLoading: true, loaded: false }));
let loading: Promise<void> | undefined;
let saving: Promise<unknown> = Promise.resolve();

export function setCatalogPresets(update: UserPreset[] | ((current: UserPreset[]) => UserPreset[])): void {
  const presets = typeof update === 'function' ? update(usePresetCatalog.getState().presets) : update;
  usePresetCatalog.setState({ presets, loaded: true });
}

export const saveCatalogPresets = debounce((presets: UserPreset[]) => {
  saving = saving.catch(() => undefined).then(() => invoke('save_presets', { presets }));
  void saving.catch((error) => console.error('Failed to save presets:', error));
}, 500);

export async function flushCatalogWrites(): Promise<void> {
  saveCatalogPresets.flush();
  await saving;
}

export function loadPresetCatalog(refresh = false): Promise<void> {
  if (loading) return loading;
  if (usePresetCatalog.getState().loaded && !refresh) return Promise.resolve();
  const snapshot = usePresetCatalog.getState().presets;
  usePresetCatalog.setState({ isLoading: true });
  loading = (async () => {
    await flushCatalogWrites();
    const presets = await invoke<UserPreset[]>('load_presets');
    // A slow disk read must never overwrite a rename/delete made in the panel.
    if (usePresetCatalog.getState().presets === snapshot) setCatalogPresets(presets);
  })().finally(() => {
    usePresetCatalog.setState({ isLoading: false });
    loading = undefined;
  });
  return loading;
}
