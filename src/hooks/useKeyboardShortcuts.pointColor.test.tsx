// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';
import { useEditorActions, debouncedSetHistory } from './useEditorActions';
import { executeAppAction } from '../utils/appActions';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { INITIAL_ADJUSTMENTS, INITIAL_MASK_ADJUSTMENTS } from '../utils/adjustments';
import { createPointColor } from '../utils/pointColor';
import Color from '../components/adjustments/Color';
import { Panel, Theme, type SelectedImage } from '../components/ui/AppProperties';
import '../i18n';
const callbacks = {
  sortedImageList: [],
  handleBackToLibrary: vi.fn(),
  handleDeleteSelected: vi.fn(),
  handleDeleteRejected: vi.fn(),
  handleGoHome: vi.fn(),
  handleImageSelect: vi.fn(),
  handlePasteFiles: vi.fn(),
  handleZoomChange: vi.fn(),
};
function Harness() {
  useKeyboardShortcuts(callbacks);
  const adjustments = useEditorStore((state) => state.adjustments);
  const { setAdjustments } = useEditorActions();
  return (
    <Color
      adjustments={adjustments}
      setAdjustments={setAdjustments}
      appSettings={{
        lastRootPath: null,
        theme: Theme.Dark,
        adjustmentLayout: {
          hiddenTools: ['whiteBalance', 'hue', 'colorPresence', 'colorGrading', 'colorCalibration'],
        },
      }}
    />
  );
}
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let node: HTMLDivElement, root: Root;
const editor = useEditorStore.getState(),
  ui = useUIStore.getState(),
  settings = useSettingsStore.getState();
beforeEach(async () => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
      unobserve() {}
    },
  );
  useUIStore.setState({ ...ui, activeView: 'editor', activePanel: Panel.Adjustments, isSettingsOpen: false });
  const point = createPointColor({ lightness: 0.7, chroma: 0.1, hue: 30 }, 'shared');
  useEditorStore.getState().resetHistory({
    ...INITIAL_ADJUSTMENTS,
    pointColor: [point],
    masks: [
      {
        id: 'local',
        name: 'Local',
        opacity: 1,
        visible: true,
        invert: false,
        subMasks: [],
        adjustments: { ...INITIAL_MASK_ADJUSTMENTS, pointColor: [point] },
      },
    ],
  });
  useEditorStore.setState({
    selectedImage: { path: '/test.raw', isReady: true, width: 1000, height: 500 } as SelectedImage,
    selectedPointColorId: 'shared',
    isPointColorPickerActive: false,
    pointColorPickerMaskId: null,
  });
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () => root.render(<Harness />));
});
afterEach(async () => {
  await act(async () => root.unmount());
  node.remove();
  vi.unstubAllGlobals();
  debouncedSetHistory.cancel();
  useEditorStore.setState(editor, true);
  useUIStore.setState(ui, true);
  useSettingsStore.setState(settings, true);
});
it('starts the global picker through the shared action and reveals real Point Color controls', async () => {
  await act(async () => {
    expect(executeAppAction('point_color_picker')).toBe(true);
  });
  expect(useEditorStore.getState().isPointColorPickerActive).toBe(true);
  expect(useEditorStore.getState().pointColorPickerMaskId).toBeNull();
  expect(node.querySelector('button[aria-label="Pick a Point Color on the photo"]')?.getAttribute('aria-pressed')).toBe(
    'true',
  );
});
it('deletes only the selected mask swatch with one Undo and rejects missing-mask context', async () => {
  const before = useEditorStore.getState().adjustments;
  await act(async () => useUIStore.setState({ activePanel: Panel.Masks }));
  await act(async () => useEditorStore.setState({ activeMaskContainerId: 'local' }));
  await act(async () => {
    expect(executeAppAction('remove_point_color')).toBe(true);
  });
  expect(useEditorStore.getState().adjustments.masks[0].adjustments.pointColor).toEqual([]);
  expect(useEditorStore.getState().adjustments.pointColor).toEqual(before.pointColor);
  expect(useEditorStore.getState().history).toHaveLength(2);
  await act(async () => useEditorStore.getState().undo());
  expect(useEditorStore.getState().adjustments).toEqual(before);
  await act(async () => useEditorStore.setState({ activeMaskContainerId: null }));
  expect(executeAppAction('point_color_picker')).toBe(false);
  expect(executeAppAction('remove_point_color')).toBe(false);
});
