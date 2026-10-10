// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';
import { useEditorStore } from '../store/useEditorStore';
import { Theme, type AppSettings, type SelectedImage } from '../components/ui/AppProperties';
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
  return null;
}
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root;
let node: HTMLDivElement;
const editor = useEditorStore.getState(),
  ui = useUIStore.getState(),
  settings = useSettingsStore.getState();
beforeEach(async () => {
  useUIStore.setState({ ...ui, activeView: 'editor', beforeAfterVisible: false, isSettingsOpen: false });
  useEditorStore.setState({ ...editor, selectedImage: { path: '/image.raw', isReady: true } as SelectedImage });
  useSettingsStore.setState({
    ...settings,
    osPlatform: 'linux',
    appSettings: { lastRootPath: null, theme: Theme.Dark, lightroomKeymap: true } as AppSettings,
  });
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () => root.render(<Harness />));
});
afterEach(async () => {
  await act(async () => root.unmount());
  node.remove();
  document.querySelector('[role=dialog]')?.remove();
  useEditorStore.setState(editor, true);
  useUIStore.setState(ui, true);
  useSettingsStore.setState(settings, true);
  vi.clearAllMocks();
});
const key = async (code: string) =>
  act(async () => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { code, key: code.slice(3).toLowerCase(), bubbles: true, cancelable: true }),
    );
  });
it('uses the optional map immediately and toggles Before/After without editing history', async () => {
  const history = useEditorStore.getState().history,
    adjustments = useEditorStore.getState().adjustments;
  await key('KeyY');
  expect(useUIStore.getState().beforeAfterVisible).toBe(true);
  expect(useEditorStore.getState().history).toBe(history);
  expect(useEditorStore.getState().adjustments).toBe(adjustments);
  await key('KeyG');
  expect(callbacks.handleBackToLibrary).toHaveBeenCalledOnce();
  await act(async () =>
    useSettingsStore.setState({
      appSettings: { lastRootPath: null, theme: Theme.Dark, lightroomKeymap: false } as AppSettings,
    }),
  );
  await key('KeyY');
  expect(useUIStore.getState().beforeAfterVisible).toBe(true);
});
it('honors custom disabling and refuses module/compare shortcuts in native dialogs or fields', async () => {
  await act(async () =>
    useSettingsStore.setState({
      appSettings: {
        lastRootPath: null,
        theme: Theme.Dark,
        lightroomKeymap: true,
        keybinds: { toggle_before_after: [] },
      } as AppSettings,
    }),
  );
  await key('KeyY');
  expect(useUIStore.getState().beforeAfterVisible).toBe(false);
  const modal = document.createElement('div');
  modal.setAttribute('role', 'dialog');
  document.body.append(modal);
  await key('KeyG');
  expect(callbacks.handleBackToLibrary).not.toHaveBeenCalled();
  modal.remove();
  const field = document.createElement('select');
  node.append(field);
  field.focus();
  await key('KeyG');
  expect(callbacks.handleBackToLibrary).not.toHaveBeenCalled();
});

it('switches from Before/After to existing Reference View without hiding its chooser', async () => {
  await key('KeyY');
  await act(async () =>
    window.dispatchEvent(
      new KeyboardEvent('keydown', { code: 'KeyR', key: 'R', shiftKey: true, bubbles: true, cancelable: true }),
    ),
  );
  expect(useUIStore.getState().beforeAfterVisible).toBe(false);
  expect(useEditorStore.getState().referenceView.mode).toBe('side-by-side');
  expect(useEditorStore.getState().referenceView.isChooserOpen).toBe(true);
});

it('reviewer: custom Before/After wins over a conflicting optional default', async () => {
  await act(async () =>
    useSettingsStore.setState({
      appSettings: {
        lastRootPath: null,
        theme: Theme.Dark,
        lightroomKeymap: true,
        keybinds: { toggle_before_after: ['KeyR'] },
      } as AppSettings,
    }),
  );
  await key('KeyR');
  expect(useUIStore.getState().beforeAfterVisible).toBe(true);
});
