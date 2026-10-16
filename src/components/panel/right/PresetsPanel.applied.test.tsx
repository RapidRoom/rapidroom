// @vitest-environment jsdom
import React, { act } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { beforeEach } from 'vitest';
import { hashPresetAdjustments, rememberAppliedPreset, useAppliedPresetStore } from '../../../utils/appliedPreset';
import AppliedPresetBadge from '../../ui/AppliedPresetBadge';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, it, expect, vi } from 'vitest';
import type { Preset } from '../../ui/AppProperties';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import { clearPresetHoverPreview } from '../../../utils/presetHoverPreview';
import { debouncedSetHistory } from '../../../hooks/useEditorActions';
import PresetsPanel from './PresetsPanel';
const { webcrypto } = await vi.importActual<{ webcrypto: Crypto }>('node:crypto');
const { TextEncoder } = await vi.importActual<{ TextEncoder: typeof globalThis.TextEncoder }>('node:util');
const fixtures = vi.hoisted(() => ({
  presets: [{ preset: { id: 'warm', name: 'Warm', adjustments: { exposure: 2 } } as Preset }],
  isLoading: false,
}));
vi.mock('../../../hooks/usePresets', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../hooks/usePresets')>()),
  usePresets: () => fixtures,
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../../context/ContextMenuContext', () => ({ useContextMenu: () => ({ showContextMenu: vi.fn() }) }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (k: string) => k }) }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root | undefined;
let container: HTMLDivElement | undefined;
afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  container?.remove();
  fixtures.presets[0].preset.cameraModelRestriction = undefined;
  clearPresetHoverPreview();
  debouncedSetHistory.cancel();
});

const persisted = new Map<string, unknown>();
let photoNumber = 0;
beforeEach(() => {
  Object.defineProperty(globalThis, 'crypto', { value: webcrypto, configurable: true });
  Object.defineProperty(globalThis, 'TextEncoder', { value: TextEncoder, configurable: true });
  persisted.clear();
  useAppliedPresetStore.setState({ records: {} });
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const { path, record } = args as { path: string; record: unknown };
    if (command === 'save_applied_preset') {
      persisted.set(path, JSON.parse(JSON.stringify(record)));
      return undefined as never;
    }
    if (command === 'load_applied_preset') return (persisted.get(path) ?? null) as never;
    return undefined as never;
  });
});
async function mount() {
  container ??= document.createElement('div');
  if (!container.parentNode) document.body.append(container);
  root ??= createRoot(container);
  await act(async () =>
    root!.render(
      <>
        <PresetsPanel onNavigateToCommunity={() => {}} />
        <AppliedPresetBadge />
      </>,
    ),
  );
}
function row() {
  return [...container!.querySelectorAll('[role=button]')].find((e) => e.textContent?.startsWith('Warm'))!;
}
async function until(check: () => boolean) {
  for (let i = 0; i < 100 && !check(); i++)
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
  expect(check()).toBe(true);
}
function photo(path: string, exposure = 0) {
  useEditorStore.getState().resetHistory({ ...INITIAL_ADJUSTMENTS, exposure });
  useEditorStore.setState({
    selectedImage: { path, isReady: true, exif: {} } as NonNullable<
      ReturnType<typeof useEditorStore.getState>['selectedImage']
    >,
    previewOverride: null,
    isSliderDragging: false,
    showOriginal: false,
  });
  useUIStore.setState({ activeView: 'editor', presetCreationRequested: false });
}
async function apply() {
  await act(async () => row().dispatchEvent(new MouseEvent('click', { bubbles: true })));
  await until(() => !!container!.querySelector('[data-applied-preset="warm"]'));
}
it('marks the actual committed preset, hides after manual edits and restores via Undo/Redo', async () => {
  photo('/applied-' + ++photoNumber + '.raw');
  await mount();
  await apply();
  expect(useEditorStore.getState().adjustments.exposure).toBe(2);
  expect(row().querySelector('[aria-label="editor.presets.applied"]')).not.toBeNull();
  const applied = useEditorStore.getState().adjustments;
  await act(async () => {
    const next = { ...applied, exposure: 3 };
    useEditorStore.getState().pushHistory(next);
    useEditorStore.setState({ adjustments: next });
  });
  expect(container!.querySelector('[data-applied-preset]')).toBeNull();
  await act(async () => useEditorStore.getState().undo());
  await until(() => !!container!.querySelector('[data-applied-preset="warm"]'));
  await act(async () => useEditorStore.getState().redo());
  expect(container!.querySelector('[data-applied-preset]')).toBeNull();
});
it('reloads the small sidecar record after a fresh UI store and panel mount', async () => {
  const path = '/restart-' + ++photoNumber + '.raw';
  photo(path);
  await mount();
  await apply();
  await until(() => persisted.has(path));
  const loaded = JSON.parse(JSON.stringify(useEditorStore.getState().adjustments));
  await act(async () => root!.unmount());
  root = undefined;
  useAppliedPresetStore.setState({ records: {} });
  photo(path);
  useEditorStore.setState({ adjustments: loaded });
  await mount();
  await until(() => !!container!.querySelector('[data-applied-preset="warm"]'));
  expect(Object.keys(persisted.get(path) as object).sort()).toEqual(['adjustmentsHash', 'amount', 'id', 'name']);
  expect(JSON.stringify(persisted.get(path)).length).toBeLessThan(200);
  expect(useEditorStore.getState().adjustments).not.toHaveProperty('appliedPreset');
});
it('switches two photos without leaking the previous photo marker or hover state', async () => {
  const a = '/photo-a-' + ++photoNumber + '.raw';
  const b = '/photo-b-' + photoNumber + '.raw';
  photo(a);
  await mount();
  await apply();
  await until(() => persisted.has(a));
  const adjustmentsA = useEditorStore.getState().adjustments;
  const adjustmentsB = { ...INITIAL_ADJUSTMENTS, exposure: -1 };
  persisted.set(b, {
    id: 'cool',
    name: 'Cool',
    amount: 50,
    adjustmentsHash: await hashPresetAdjustments(adjustmentsB),
  });
  await act(async () => {
    photo(b);
    useEditorStore.setState({ adjustments: adjustmentsB });
  });
  expect(container!.querySelector('[data-applied-preset="warm"]')).toBeNull();
  await until(() => !!container!.querySelector('[data-applied-preset="cool"]'));
  expect(container!.querySelector('[data-applied-preset="cool"]')!.textContent).toContain('50%');
  await act(async () => {
    photo(a);
    useEditorStore.setState({ adjustments: adjustmentsA });
  });
  await until(() => !!container!.querySelector('[data-applied-preset="warm"]'));
  await act(async () => useEditorStore.setState({ previewOverride: { ...adjustmentsA, exposure: 99 } }));
  expect(container!.querySelector('[data-applied-preset="warm"]')).not.toBeNull();
});
it('does not mark a hover-only preset preview', async () => {
  const path = '/hover-' + ++photoNumber + '.raw';
  photo(path);
  await mount();
  await act(async () => row().dispatchEvent(new MouseEvent('mouseover', { bubbles: true })));
  expect(useEditorStore.getState().previewOverride?.exposure).toBe(2);
  expect(container!.querySelector('[data-applied-preset]')).toBeNull();
  expect(persisted.has(path)).toBe(false);
});

it('records Amount from the real slider without compounding and restores its saved amount', async () => {
  const path = '/amount-' + ++photoNumber + '.raw';
  photo(path);
  await mount();
  await apply();
  const slider = container!.querySelector('input[type="range"]') as HTMLInputElement;
  expect(slider).toBeTruthy();
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(slider, '50');
    slider.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await until(() => useAppliedPresetStore.getState().records[path]?.amount === 50);
  expect(useEditorStore.getState().adjustments.exposure).toBe(1);
  await until(() => (persisted.get(path) as { amount: number })?.amount === 50);
  expect(row().textContent).toContain('50%');
});
it('serializes an old in-flight save ahead of a newer preset and keeps the latest record', async () => {
  const path = '/ordered-' + ++photoNumber + '.raw';
  let release!: () => void;
  const blocked = new Promise<void>((resolve) => {
    release = resolve;
  });
  let oldStarted = false;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const { path: photoPath, record } = args as { path: string; record: { name: string } };
    if (command === 'save_applied_preset') {
      if (record.name === 'First') {
        oldStarted = true;
        await blocked;
      }
      persisted.set(photoPath, record);
    }
    return undefined as never;
  });
  const first = rememberAppliedPreset(path, { id: 'first', name: 'First' }, 100, {
    ...INITIAL_ADJUSTMENTS,
    exposure: 1,
  });
  await until(() => oldStarted);
  const second = rememberAppliedPreset(path, { id: 'second', name: 'Second' }, 50, {
    ...INITIAL_ADJUSTMENTS,
    exposure: 2,
  });
  release();
  await Promise.all([first, second]);
  expect((persisted.get(path) as { name: string }).name).toBe('Second');
  expect(useAppliedPresetStore.getState().records[path]?.amount).toBe(50);
});
it('hashes normalized values independently of key order and detects a real edit', async () => {
  const a = { ...INITIAL_ADJUSTMENTS, exposure: 2 };
  const b = Object.fromEntries(Object.entries(a).reverse()) as typeof a;
  expect(await hashPresetAdjustments(a)).toBe(await hashPresetAdjustments(b));
  expect(await hashPresetAdjustments(a)).not.toBe(await hashPresetAdjustments({ ...b, exposure: 3 }));
});
