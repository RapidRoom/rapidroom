// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';
import { executeAppAction } from '../utils/appActions';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import type { SelectedImage } from '../components/ui/AppProperties';
const zoom = vi.fn((percent: number) => useEditorStore.setState({ zoom: percent / 0.2 }));
const callbacks = {
  sortedImageList: [],
  handleBackToLibrary: vi.fn(),
  handleDeleteSelected: vi.fn(),
  handleDeleteRejected: vi.fn(),
  handleGoHome: vi.fn(),
  handleImageSelect: vi.fn(),
  handlePasteFiles: vi.fn(),
  handleZoomChange: zoom,
};
function Harness() {
  useKeyboardShortcuts(callbacks);
  return null;
}
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let node: HTMLDivElement, root: Root;
const editor = useEditorStore.getState(),
  ui = useUIStore.getState(),
  settings = useSettingsStore.getState();
beforeEach(async () => {
  vi.stubGlobal('devicePixelRatio', 1);
  useUIStore.setState({ ...ui, activeView: 'editor', isSettingsOpen: false });
  useSettingsStore.setState({ ...settings, osPlatform: 'linux' });
  useEditorStore.setState({
    ...editor,
    selectedImage: { path: '/test.raw', isReady: true, width: 1000, height: 500 } as SelectedImage,
    zoom: 5,
    baseRenderSize: {
      width: 200,
      height: 100,
      renderScale: 0.2,
      containerWidth: 200,
      containerHeight: 100,
      offsetX: 0,
      offsetY: 0,
    },
    originalSize: { width: 1000, height: 500 },
    displaySize: { width: 1000, height: 500 },
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
  zoom.mockClear();
  useEditorStore.setState(editor, true);
  useUIStore.setState(ui, true);
  useSettingsStore.setState(settings, true);
});
it('shares fixed zoom stops and the cap with palette actions despite a stale display readout', async () => {
  await act(async () => {
    for (let n = 0; n < 5; n++) expect(executeAppAction('zoom_in_step')).toBe(true);
  });
  expect(zoom.mock.calls.map(([value]) => value)).toEqual([1.5, 2, 3, 4, 4]);
  expect(useEditorStore.getState().displaySize).toEqual({ width: 1000, height: 500 });
  await act(async () => {
    executeAppAction('zoom_out_step');
  });
  expect(zoom).toHaveBeenLastCalledWith(3);
});
it('keeps the shared zoom action unavailable outside an active editor photo', async () => {
  await act(async () => useUIStore.setState({ activeView: 'library' }));
  expect(executeAppAction('zoom_in_step')).toBe(false);
  expect(zoom).not.toHaveBeenCalled();
});
