// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import '../../i18n';
import { mockCommand } from '../../test/tauriMock';
import { Invokes } from './AppProperties';
import CommandPalette from './CommandPalette';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';
import { INITIAL_ADJUSTMENTS } from '../../utils/adjustments';
import type { SelectedImage } from './AppProperties';
import { registerAppActions } from '../../utils/appActions';
import { debouncedSetHistory } from '../../hooks/useEditorActions';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialEditor = useEditorStore.getState();
const initialUI = useUIStore.getState();
let root: Root;
let container: HTMLDivElement;

beforeEach(() => {
  vi.useFakeTimers();
  mockCommand(Invokes.LoadPresets, () => []);
  const adjustments = structuredClone(INITIAL_ADJUSTMENTS);
  useEditorStore.getState().resetHistory(adjustments);
  useEditorStore.getState().setEditor({
    selectedImage: {
      path: '/owned/test.ARW',
      isReady: true,
      width: 6000,
      height: 4000,
      isRaw: true,
      thumbnailUrl: '',
      exif: null,
    } satisfies SelectedImage,
    previewOverride: null,
    showOriginal: false,
  });
  useUIStore.getState().setUI({ activeView: 'editor' });
  container = document.createElement('div');
  document.body.appendChild(container);
  root = createRoot(container);
  act(() => root.render(createElement(CommandPalette)));
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  debouncedSetHistory.cancel();
  useEditorStore.setState(initialEditor, true);
  useUIStore.setState(initialUI, true);
  vi.useRealTimers();
});

function open() {
  act(() =>
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', code: 'KeyK', ctrlKey: true, bubbles: true })),
  );
}
function input(): HTMLInputElement {
  return container.querySelector('input')!;
}
function type(value: string) {
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input(), value);
    input().dispatchEvent(new Event('input', { bubbles: true }));
  });
}
function key(key: string) {
  act(() => input().dispatchEvent(new KeyboardEvent('keydown', { key, code: key, bubbles: true })));
}

describe('keyboard palette editing', () => {
  it('sets typed Exposure and records one undoable action', () => {
    open();
    type('exposure 0.7');
    key('Enter');
    expect(container.querySelector('[role=dialog]')).toBeNull();
    expect(useEditorStore.getState().adjustments.exposure).toBe(0.7);
    expect(useEditorStore.getState().history).toHaveLength(2);
    act(() => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments.exposure).toBe(0);
  });
  it('previews slider keyboard nudges without edits/history and reverts on Escape', () => {
    open();
    type('exposure');
    key('Enter');
    key('ArrowRight');
    key('ArrowRight');
    act(() => vi.advanceTimersByTime(151));
    expect(useEditorStore.getState().previewOverride?.exposure).toBe(0.02);
    expect(useEditorStore.getState().adjustments.exposure).toBe(0);
    expect(useEditorStore.getState().history).toHaveLength(1);
    key('Escape');
    expect(useEditorStore.getState().previewOverride).toBeNull();
    expect(useEditorStore.getState().history).toHaveLength(1);
  });
  it('commits separate immediate commands as separate Undo actions', () => {
    open();
    type('exposure 0.7');
    key('Enter');
    open();
    type('contrast 20');
    key('Enter');
    expect(useEditorStore.getState().history).toHaveLength(3);
    act(() => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments).toMatchObject({ exposure: 0.7, contrast: 0 });
  });
  it('does not replace a concurrent edit with a stale preview', () => {
    open();
    type('exposure 0.7');
    act(() => vi.advanceTimersByTime(151));
    const next = { ...useEditorStore.getState().adjustments, contrast: 30 };
    act(() => useEditorStore.getState().setEditor({ adjustments: next }));
    expect(container.querySelector('[role=dialog]')).toBeNull();
    expect(useEditorStore.getState().adjustments).toBe(next);
    expect(useEditorStore.getState().previewOverride).toBeNull();
  });
  it('clears only its own preview when the photo changes', () => {
    open();
    type('exposure 0.7');
    act(() => vi.advanceTimersByTime(151));
    const external = { ...INITIAL_ADJUSTMENTS, contrast: 40 };
    act(() =>
      useEditorStore.getState().setEditor({
        selectedImage: {
          path: '/owned/other.ARW',
          isReady: true,
          width: 6000,
          height: 4000,
          isRaw: true,
          thumbnailUrl: '',
          exif: null,
        } satisfies SelectedImage,
        previewOverride: external,
      }),
    );
    expect(container.querySelector('[role=dialog]')).toBeNull();
    expect(useEditorStore.getState().previewOverride).toBe(external);
  });
  it('traps Tab in the palette input and restores the prior focus', () => {
    const prior = document.createElement('button');
    document.body.appendChild(prior);
    prior.focus();
    open();
    key('Tab');
    expect(document.activeElement).toBe(input());
    key('Escape');
    expect(document.activeElement).toBe(prior);
    prior.remove();
  });
  it('does not open over an existing modal', () => {
    act(() => useUIStore.getState().setUI({ isImportModalOpen: true }));
    open();
    expect(container.querySelector('[role=dialog]')).toBeNull();
  });
  it('previews a tone-mapper choice and cancels without history', () => {
    open();
    type('agx');
    act(() => vi.advanceTimersByTime(151));
    expect(useEditorStore.getState().previewOverride?.toneMapper).toBe('agx');
    expect(useEditorStore.getState().adjustments.toneMapper).toBe('basic');
    key('Escape');
    expect(useEditorStore.getState().previewOverride).toBeNull();
    expect(useEditorStore.getState().history).toHaveLength(1);
  });
  it('confirming the existing tone mapper does not create an Undo step', () => {
    open();
    type('basic');
    key('Enter');
    expect(useEditorStore.getState().history).toHaveLength(1);
  });
  it('loads and previews a local preset, then commits one undoable edit', async () => {
    mockCommand(Invokes.LoadPresets, () => [
      { preset: { id: 'warm', name: 'Warm portrait', adjustments: { temperature: 12, contrast: 15 } } },
    ]);
    open();
    await act(async () => {
      await Promise.resolve();
    });
    type('Warm portrait');
    act(() => vi.advanceTimersByTime(151));
    expect(useEditorStore.getState().previewOverride).toMatchObject({ temperature: 12, contrast: 15 });
    expect(useEditorStore.getState().adjustments.temperature).toBe(0);
    key('Enter');
    expect(useEditorStore.getState().adjustments).toMatchObject({ temperature: 12, contrast: 15 });
    expect(useEditorStore.getState().history).toHaveLength(2);
    act(() => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments.temperature).toBe(0);
  });
  it('uses the guarded shared action without executing disabled commands', () => {
    let enabled = false;
    const execute = vi.fn();
    const dispose = registerAppActions({ auto_adjustments: { available: () => enabled, execute } });
    open();
    type('auto_adjustments');
    key('Enter');
    expect(execute).not.toHaveBeenCalled();
    expect(container.querySelector('[role=dialog]')).not.toBeNull();
    enabled = true;
    type('auto');
    key('Enter');
    expect(execute).toHaveBeenCalledOnce();
    expect(container.querySelector('[role=dialog]')).toBeNull();
    dispose();
  });
  it('sets a typed crop ratio through the actual crop geometry helper', () => {
    open();
    type('crop 4:3');
    key('Enter');
    expect(useEditorStore.getState().adjustments.aspectRatio).toBeCloseTo(4 / 3);
    expect(useEditorStore.getState().adjustments.crop).not.toBeNull();
    expect(useEditorStore.getState().history).toHaveLength(2);
  });
});
