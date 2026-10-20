// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';
import { debouncedSave, debouncedSetHistory, useEditorActions } from './useEditorActions';
import { executeAppAction } from '../utils/appActions';
import { createResetAdjustmentsWithLensDefaults } from '../utils/rawLensDefaults';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { activeStyleStack, commitPresetApplication, preparePresetApplication } from '../utils/presetApplication';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import type { SelectedImage } from '../components/ui/AppProperties';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue({}) }));
vi.mock('../utils/rawLensDefaults', () => ({ createResetAdjustmentsWithLensDefaults: vi.fn() }));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialEditor = useEditorStore.getState();
const initialLibrary = useLibraryStore.getState();
const initialUi = useUIStore.getState();
const initialSettings = useSettingsStore.getState();
let node: HTMLDivElement, root: Root, actions: ReturnType<typeof useEditorActions>;
function Harness() {
  actions = useEditorActions();
  useKeyboardShortcuts({
    sortedImageList: [],
    handleBackToLibrary: vi.fn(),
    handleDeleteSelected: vi.fn(),
    handleDeleteRejected: vi.fn(),
    handleGoHome: vi.fn(),
    handleImageSelect: vi.fn(),
    handlePasteFiles: vi.fn(),
    handleZoomChange: vi.fn(),
  });
  return null;
}
beforeEach(async () => {
  vi.useFakeTimers();
  vi.mocked(createResetAdjustmentsWithLensDefaults).mockReset();
  vi.mocked(createResetAdjustmentsWithLensDefaults).mockResolvedValue(structuredClone(INITIAL_ADJUSTMENTS));
  useEditorStore.setState(initialEditor, true);
  useEditorStore.getState().resetHistory(structuredClone(INITIAL_ADJUSTMENTS));
  useEditorStore.setState({
    selectedImage: { path: '/history.raw', isReady: true, width: 1000, height: 500 } as SelectedImage,
  });
  useUIStore.setState({ ...initialUi, activeView: 'editor', isSettingsOpen: false });
  useSettingsStore.setState({ ...initialSettings, osPlatform: 'linux' });
  useLibraryStore.setState(initialLibrary, true);
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () => root.render(<Harness />));
});
afterEach(async () => {
  await act(async () => root.unmount());
  node.remove();
  debouncedSetHistory.cancel();
  debouncedSave.cancel();
  vi.useRealTimers();
  useEditorStore.setState(initialEditor, true);
  useLibraryStore.setState(initialLibrary, true);
  useUIStore.setState(initialUi, true);
  useSettingsStore.setState(initialSettings, true);
});
it('records the palette/shortcut Reset label and restores the pending slider on Undo', async () => {
  await act(async () => actions.setAdjustments((previous) => ({ ...previous, exposure: 0.7 })));
  const pending = useEditorStore.getState().adjustments;
  expect(useEditorStore.getState().history).toHaveLength(1);
  await act(async () => {
    expect(executeAppAction('reset_adjustments')).toBe(true);
  });
  const reset = useEditorStore.getState();
  expect(reset.adjustments.exposure).toBe(0);
  expect(reset.historyDetails[reset.historyIndex]?.label).toBe('Reset adjustments');
  expect(reset.history).toHaveLength(3);
  await act(async () => useEditorStore.getState().undo());
  expect(useEditorStore.getState().adjustments).toEqual(pending);
});
it.each(['edit', 'navigation'])('refuses a delayed palette Reset after a newer %s', async (change) => {
  let resolve!: (value: typeof INITIAL_ADJUSTMENTS) => void;
  vi.mocked(createResetAdjustmentsWithLensDefaults).mockImplementation(() => new Promise((done) => (resolve = done)));
  await act(async () => {
    expect(executeAppAction('reset_adjustments')).toBe(true);
  });
  await act(async () => {
    if (change === 'edit') actions.setAdjustments((previous) => ({ ...previous, exposure: 0.8 }));
    else
      useEditorStore.setState({ selectedImage: { ...useEditorStore.getState().selectedImage!, path: '/other.raw' } });
  });
  const current = useEditorStore.getState().adjustments;
  await act(async () => resolve(structuredClone(INITIAL_ADJUSTMENTS)));
  expect(useEditorStore.getState().adjustments).toBe(current);
  expect(useEditorStore.getState().historyDetails.some((entry) => entry?.label === 'Reset adjustments')).toBe(false);
});

it.each(['shortcut', 'panel'])(
  'clears applied Styles on %s Reset and restores them with the exact snapshot on Undo/Redo',
  async (entry) => {
    await act(async () => {
      const plan = preparePresetApplication(
        {
          id: 'history-style',
          name: 'Warm portrait',
          presetType: 'style',
          adjustments: { exposure: 1.2, contrast: 17 },
        },
        100,
      );
      commitPresetApplication(plan, (next) => actions.setAdjustments(() => next, plan.application.label));
      debouncedSetHistory.flush();
    });
    const styled = useEditorStore.getState().adjustments;
    expect(activeStyleStack()?.entries.map((style) => style.presetId)).toEqual(['history-style']);
    await act(async () => {
      if (entry === 'shortcut') expect(executeAppAction('reset_adjustments')).toBe(true);
      else actions.handleResetAdjustments(['/history.raw']);
    });
    expect(useEditorStore.getState().adjustments.exposure).toBe(0);
    expect(useEditorStore.getState().historyDetails.at(-1)?.label).toBe('Reset adjustments');
    expect(activeStyleStack()).toBeNull();
    await act(async () => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments).toBe(styled);
    expect(activeStyleStack()?.entries.map((style) => style.presetId)).toEqual(['history-style']);
    await act(async () => useEditorStore.getState().redo());
    expect(useEditorStore.getState().adjustments.exposure).toBe(0);
    expect(activeStyleStack()).toBeNull();
  },
);
