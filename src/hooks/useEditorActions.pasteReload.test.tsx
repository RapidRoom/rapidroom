// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { toast } from 'react-toastify';
import { mockCommand } from '../test/tauriMock';
import { Invokes, Theme, type SelectedImage } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, PasteMode, type Adjustments } from '../utils/adjustments';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { debouncedSave, debouncedSetHistory, useEditorActions } from './useEditorActions';

// The post-paste reload must never put pre-paste values back into the editor (#421 review).

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const editorInitial = useEditorStore.getState();
const libraryInitial = useLibraryStore.getState();
const settingsInitial = useSettingsStore.getState();
const image: SelectedImage = {
  path: '/open.ARW',
  width: 6000,
  height: 4000,
  isRaw: true,
  exif: {},
  isReady: true,
  thumbnailUrl: '',
};
let root: Root | undefined;
let actions: ReturnType<typeof useEditorActions>;
let onDisk: Partial<Adjustments>;
let resolveApply: () => void;

beforeEach(async () => {
  vi.useFakeTimers();
  useEditorStore.setState(editorInitial, true);
  useLibraryStore.setState(libraryInitial, true);
  useSettingsStore.setState(settingsInitial, true);
  useSettingsStore.setState({
    appSettings: {
      theme: Theme.Dark,
      lastRootPath: null,
      copyPasteSettings: {
        mode: PasteMode.Replace,
        includedAdjustments: ['exposure', 'contrast'],
        knownAdjustments: [],
        autoSync: false,
      },
    },
  } as never);
  onDisk = { ...INITIAL_ADJUSTMENTS };
  mockCommand(Invokes.ApplyAdjustmentsToPaths, () => new Promise<void>((resolve) => (resolveApply = resolve)));
  mockCommand(Invokes.LoadMetadata, () => ({ adjustments: structuredClone(onDisk) }));
  vi.spyOn(toast, 'error').mockImplementation(() => 'error');
  useEditorStore.setState({
    selectedImage: image,
    adjustments: { ...INITIAL_ADJUSTMENTS },
    copiedAdjustments: { ...INITIAL_ADJUSTMENTS, exposure: 1.5, contrast: 20 },
  });
  function Harness() {
    actions = useEditorActions();
    return null;
  }
  root = createRoot(document.createElement('div'));
  await act(async () => root!.render(createElement(Harness)));
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  debouncedSetHistory.cancel();
  debouncedSave.cancel();
  useEditorStore.setState(editorInitial, true);
  useLibraryStore.setState(libraryInitial, true);
  useSettingsStore.setState(settingsInitial, true);
  vi.restoreAllMocks();
  vi.useRealTimers();
});

async function pasteAndFinish() {
  await act(async () => actions.handlePasteAdjustments(['/open.ARW']));
  expect(useEditorStore.getState().adjustments.exposure).toBe(1.5);
  await act(async () => {
    resolveApply();
    await vi.runAllTimersAsync();
  });
}

it('a stale (pre-paste) reload leaves the pasted values in the editor', async () => {
  await pasteAndFinish();
  const { adjustments } = useEditorStore.getState();
  expect(adjustments.exposure).toBe(1.5);
  expect(adjustments.contrast).toBe(20);
});

it('takes a backend-resolved value for a pasted key the user has not touched', async () => {
  onDisk = { ...INITIAL_ADJUSTMENTS, exposure: 1.5, contrast: 20, lensMaker: 'Sony' };
  await pasteAndFinish();
  expect(useEditorStore.getState().adjustments.lensMaker).toBe('Sony');
});

it('keeps an edit the user made after the paste', async () => {
  await act(async () => actions.handlePasteAdjustments(['/open.ARW']));
  await act(async () => actions.setAdjustments((prev: Adjustments) => ({ ...prev, contrast: 35 })));
  onDisk = { ...INITIAL_ADJUSTMENTS, exposure: 1.5, contrast: 10 };
  await act(async () => {
    resolveApply();
    await vi.runAllTimersAsync();
  });
  expect(useEditorStore.getState().adjustments.contrast).toBe(35);
});
