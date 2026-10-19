import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { create } from 'zustand';
import { normalizeLoadedAdjustments, type Adjustments } from './adjustments';
import type { Preset } from '../components/ui/AppProperties';
import { useLibraryStore } from '../store/useLibraryStore';
import { isPathInCardRoot } from './cardMode';
import { useEditorStore } from '../store/useEditorStore';
import { queuePhotoWrite } from './photoWriteQueue';
import { keepManualStyleEdits, migrateAppliedStyle, type StyleStack } from './styleStack';

export interface AppliedPreset {
  id: string;
  name: string;
  amount: number;
  adjustmentsHash: string;
  stack?: StyleStack;
}

export const useAppliedPresetStore = create<{ records: Record<string, AppliedPreset | null> }>(() => ({ records: {} }));
const snapshots = new Map<string, WeakMap<Adjustments, StyleStack | null>>();
export function styleSnapshot(path: string, adjustments: Adjustments): StyleStack | null | undefined {
  return snapshots.get(path)?.get(adjustments);
}
export function rememberStyleSnapshot(path: string, adjustments: Adjustments, stack: StyleStack | null): void {
  let map = snapshots.get(path);
  if (!map) snapshots.set(path, (map = new WeakMap()));
  map.set(adjustments, stack);
}
export function appliedPresetLoading(path: string): boolean {
  return loads.has(path);
}
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
    const controls = { ...normalizeLoadedAdjustments(adjustments) } as Adjustments;
    // Autosave derives EXIF profile names in Auto mode and regenerates selected
    // profile coefficients. Hash user controls, not those per-photo cache values.
    if (controls.lensCorrectionMode === 'auto') {
      controls.lensMaker = null;
      controls.lensModel = null;
      controls.lensDistortionParams = null;
    } else if (
      typeof controls.lensMaker === 'string' &&
      controls.lensMaker.trim() &&
      typeof controls.lensModel === 'string' &&
      controls.lensModel.trim()
    ) {
      controls.lensDistortionParams = null;
    }
    const bytes = new TextEncoder().encode(JSON.stringify(canonical(controls)));
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
  const current = useEditorStore.getState();
  const records = useAppliedPresetStore.getState().records;
  if (
    current.selectedImage?.path === path &&
    Object.hasOwn(records, path) &&
    styleSnapshot(path, current.adjustments) !== undefined
  )
    return Promise.resolve();
  const existing = loads.get(path);
  if (existing) return existing;
  const generation = generations.get(path) ?? 0;
  // Pending writes already have the newest in-memory record. Reading through
  // their debounce would block MCP and can read an older sidecar on navigation.
  const pending = (
    writes.has(path) && Object.hasOwn(records, path)
      ? Promise.resolve(records[path])
      : invoke<AppliedPreset | null>('load_applied_preset', { path })
  )
    .then(async (record) => {
      if ((generations.get(path) ?? 0) !== generation) return;
      const editor = useEditorStore.getState();
      const adjustments = editor.adjustments;
      if (editor.selectedImage?.path === path && editor.selectedImage.isReady) {
        const stack = !record
          ? null
          : record.stack
            ? keepManualStyleEdits(
                {
                  ...record.stack,
                  base: normalizeLoadedAdjustments(record.stack.base),
                  result: normalizeLoadedAdjustments(record.stack.result),
                },
                adjustments,
              )
            : (await hashPresetAdjustments(adjustments)) === record.adjustmentsHash
              ? migrateAppliedStyle(record, adjustments)
              : null;
        if (
          (generations.get(path) ?? 0) !== generation ||
          useEditorStore.getState().adjustments !== adjustments ||
          useEditorStore.getState().selectedImage?.path !== path
        )
          return;
        rememberStyleSnapshot(path, adjustments, stack);
        if (stack && record) record = { ...record, stack };
      }
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

function persistAppliedPreset(path: string, record: AppliedPreset | null, adjustments: Adjustments): Promise<void> {
  const generation = (generations.get(path) ?? 0) + 1;
  generations.set(path, generation);
  setRecord(path, record);
  const pending = (writes.get(path) ?? Promise.resolve())
    .then(async () => {
      if (generations.get(path) !== generation) return;
      const saved = record ? { ...record, adjustmentsHash: await hashPresetAdjustments(adjustments) } : null;
      if (generations.get(path) !== generation) return;
      setRecord(path, saved);
      await new Promise((resolve) => setTimeout(resolve, 250));
      if (generations.get(path) !== generation) return;
      if (!isPathInCardRoot(path, useLibraryStore.getState().cardBrowseRoot)) {
        await queuePhotoWrite(
          path,
          async () => {
            if (generations.get(path) !== generation) return;
            await invoke('save_applied_preset', {
              path,
              record: saved,
              ...(saved?.stack || !saved ? { adjustments } : {}),
            });
          },
          adjustments,
          async () => {
            if (generations.get(path) !== generation) return;
            // An imported packet consumed these controls. Retain the captured
            // Style recipe/Amount while leaving the native import's pixels alone;
            // loading it reconciles imported differences as manual edits.
            await invoke('save_applied_preset', { path, record: saved });
          },
        );
      }
    })
    .catch(() => {
      if (generations.get(path) === generation) toast.error('Could not save the applied Styles.');
    })
    .finally(() => {
      if (writes.get(path) === pending) writes.delete(path);
    });
  writes.set(path, pending);
  return pending;
}

export function rememberAppliedPreset(
  path: string,
  preset: Pick<Preset, 'id' | 'name'>,
  amount: number,
  adjustments: Adjustments,
  stack?: StyleStack,
): Promise<void> {
  return persistAppliedPreset(
    path,
    { id: preset.id, name: preset.name, amount, adjustmentsHash: '', ...(stack ? { stack } : {}) },
    adjustments,
  );
}

export function rememberAppliedStyleStack(
  path: string,
  stack: StyleStack | null,
  adjustments: Adjustments,
): Promise<void> {
  rememberStyleSnapshot(path, adjustments, stack);
  if (!stack) return persistAppliedPreset(path, null, adjustments);
  const last = stack.entries.at(-1);
  return rememberAppliedPreset(
    path,
    { id: last?.presetId ?? 'rapidroom-empty-style-stack', name: last?.name ?? 'Styles' },
    last?.amount ?? 0,
    adjustments,
    stack,
  );
}
