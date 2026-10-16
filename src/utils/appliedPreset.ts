import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { create } from 'zustand';
import { normalizeLoadedAdjustments, type Adjustments } from './adjustments';
import type { Preset } from '../components/ui/AppProperties';
import { useLibraryStore } from '../store/useLibraryStore';
import { isPathInCardRoot } from './cardMode';

export interface AppliedPreset {
  id: string;
  name: string;
  amount: number;
  adjustmentsHash: string;
}

export const useAppliedPresetStore = create<{ records: Record<string, AppliedPreset | null> }>(() => ({ records: {} }));
const generations = new Map<string, number>();
const loads = new Map<string, Promise<void>>();
const writes = new Map<string, Promise<void>>();
const hashes = new WeakMap<Adjustments, Promise<string>>();

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([key, child]) => [key, canonical(child)]),
    );
  }
  return value;
}

export function hashPresetAdjustments(adjustments: Adjustments): Promise<string> {
  let pending = hashes.get(adjustments);
  if (!pending) {
    const bytes = new TextEncoder().encode(JSON.stringify(canonical(normalizeLoadedAdjustments(adjustments))));
    pending = crypto.subtle
      .digest('SHA-256', bytes)
      .then(
        (buffer) => 'sha256-v1:' + [...new Uint8Array(buffer)].map((b) => b.toString(16).padStart(2, '0')).join(''),
      );
    hashes.set(adjustments, pending);
  }
  return pending;
}

function setRecord(path: string, record: AppliedPreset | null): void {
  useAppliedPresetStore.setState((state) => ({ records: { ...state.records, [path]: record } }));
}

export function loadAppliedPreset(path: string): Promise<void> {
  const existing = loads.get(path);
  if (existing) return existing;
  const generation = generations.get(path) ?? 0;
  const pending = (writes.get(path) ?? Promise.resolve())
    .then(() => invoke<AppliedPreset | null>('load_applied_preset', { path }))
    .then((record) => {
      if ((generations.get(path) ?? 0) === generation) setRecord(path, record);
    })
    .catch(() => {
      // A damaged/unreadable sidecar must not stop Develop from opening a photo.
      if ((generations.get(path) ?? 0) === generation) setRecord(path, null);
    })
    .finally(() => {
      if (loads.get(path) === pending) loads.delete(path);
    });
  loads.set(path, pending);
  return pending;
}

export function rememberAppliedPreset(
  path: string,
  preset: Pick<Preset, 'id' | 'name'>,
  amount: number,
  adjustments: Adjustments,
): Promise<void> {
  const generation = (generations.get(path) ?? 0) + 1;
  generations.set(path, generation);
  // Serialize writes to this photo; an older in-flight write cannot finish after a newer one.
  const pending = (writes.get(path) ?? Promise.resolve())
    .then(async () => {
      if (generations.get(path) !== generation) return;
      const adjustmentsHash = await hashPresetAdjustments(adjustments);
      if (generations.get(path) !== generation) return;
      const record = { id: preset.id, name: preset.name, amount, adjustmentsHash };
      setRecord(path, record);
      // Coalesce Amount drags without delaying the visible marker or changing edit autosave.
      await new Promise((resolve) => setTimeout(resolve, 250));
      if (generations.get(path) !== generation) return;
      if (!isPathInCardRoot(path, useLibraryStore.getState().cardBrowseRoot)) {
        await invoke('save_applied_preset', { path, record });
      }
    })
    .catch(() => {
      if (generations.get(path) === generation) {
        setRecord(path, null);
        toast.error('Could not save the applied preset indicator.');
      }
    })
    .finally(() => {
      if (writes.get(path) === pending) writes.delete(path);
    });
  writes.set(path, pending);
  return pending;
}
