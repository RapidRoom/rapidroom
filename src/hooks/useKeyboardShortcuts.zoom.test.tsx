// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';
import { executeAppAction } from '../utils/appActions';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { LibraryViewMode, Theme, type ImageFile, type SelectedImage } from '../components/ui/AppProperties';
const zoom = vi.fn((percent: number) => useEditorStore.setState({ zoom: percent / 0.2 }));
const callbacks = {
  sortedImageList: [] as ImageFile[],
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
  library = useLibraryStore.getState(),
  settings = useSettingsStore.getState();
beforeEach(async () => {
  vi.stubGlobal('devicePixelRatio', 1);
  useUIStore.setState({ ...ui, activeView: 'editor', isSettingsOpen: false });
  useSettingsStore.setState({ ...settings, osPlatform: 'linux' });
  useLibraryStore.setState(library, true);
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
  useLibraryStore.setState(library, true);
  useSettingsStore.setState(settings, true);
  callbacks.sortedImageList = [];
  vi.mocked(callbacks.handleImageSelect).mockClear();
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

it('keeps arrow navigation within the active folder in Recursive view', async () => {
  const paths = [
    '/photos/FOLDER123/IMAGE1.jpg',
    '/photos/FOLDER123/IMAGE2.jpg',
    '/photos/FOLDER456/IMAGE3.jpg',
    '/photos/FOLDER456/IMAGE4.jpg',
    '/photos/FOLDER123/IMAGE47.jpg',
  ];
  callbacks.sortedImageList = paths.map((path) => ({ path }) as ImageFile);
  await act(async () => {
    root.render(<Harness />);
    useUIStore.setState({ ...ui, activeView: 'library', isSettingsOpen: false });
    useSettingsStore.setState({
      ...settings,
      osPlatform: 'linux',
      appSettings: {
        ...(settings.appSettings ?? { lastRootPath: null, theme: Theme.Dark }),
        libraryViewMode: LibraryViewMode.Recursive,
      },
    });
    useLibraryStore.setState({
      ...library,
      libraryActivePath: '/photos/FOLDER123/IMAGE2.jpg',
      multiSelectedPaths: ['/photos/FOLDER123/IMAGE2.jpg'],
    });
  });
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'ArrowRight', bubbles: true }));
  });

  expect(useLibraryStore.getState().libraryActivePath).toBe('/photos/FOLDER123/IMAGE47.jpg');
  expect(callbacks.handleImageSelect).toHaveBeenCalledWith('/photos/FOLDER123/IMAGE47.jpg', false);
});

it('preserves global arrow navigation outside Recursive view', async () => {
  callbacks.sortedImageList = [
    '/photos/FOLDER123/IMAGE1.jpg',
    '/photos/FOLDER123/IMAGE2.jpg',
    '/photos/FOLDER456/IMAGE3.jpg',
  ].map((path) => ({ path }) as ImageFile);
  await act(async () => {
    root.render(<Harness />);
    useUIStore.setState({ ...ui, activeView: 'library', isSettingsOpen: false });
    useSettingsStore.setState({
      ...settings,
      osPlatform: 'linux',
      appSettings: {
        ...(settings.appSettings ?? { lastRootPath: null, theme: Theme.Dark }),
        libraryViewMode: LibraryViewMode.Flat,
      },
    });
    useLibraryStore.setState({
      ...library,
      libraryActivePath: '/photos/FOLDER123/IMAGE2.jpg',
      multiSelectedPaths: ['/photos/FOLDER123/IMAGE2.jpg'],
    });
  });
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'ArrowRight', bubbles: true }));
  });

  expect(useLibraryStore.getState().libraryActivePath).toBe('/photos/FOLDER456/IMAGE3.jpg');
  expect(callbacks.handleImageSelect).toHaveBeenCalledWith('/photos/FOLDER456/IMAGE3.jpg', false);
});

it('advances rapid editor arrows from the pending photo while the displayed photo is still loading', async () => {
  const paths = ['/A.ARW', '/B.ARW', '/C.ARW', '/D.ARW'];
  callbacks.sortedImageList = paths.map((path) => ({ path }) as ImageFile);
  callbacks.handleImageSelect.mockImplementation((path: string) => {
    // The real navigation hook publishes this intent before its asynchronous cache check.
    useLibraryStore.getState().setLibrary({ libraryActivePath: path });
  });
  try {
    await act(async () => {
      useEditorStore
        .getState()
        .setEditor({ selectedImage: { ...useEditorStore.getState().selectedImage!, path: paths[0] } });
      useLibraryStore.getState().setLibrary({ libraryActivePath: paths[1] });
      root.render(<Harness />);
    });
    await act(async () => {
      for (const code of ['ArrowRight', 'ArrowRight', 'ArrowLeft']) {
        window.dispatchEvent(new KeyboardEvent('keydown', { code, key: code, bubbles: true }));
      }
    });
    expect(callbacks.handleImageSelect.mock.calls.map(([path]) => path)).toEqual([paths[2], paths[3], paths[2]]);
    expect(useEditorStore.getState().selectedImage?.path).toBe(paths[0]);
  } finally {
    callbacks.handleImageSelect.mockReset();
  }
});

it('opens Instagram through the rebindable action while respecting text and modal focus', async () => {
  const key = () =>
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'i',
        code: 'KeyI',
        ctrlKey: true,
        altKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  const input = document.createElement('input');
  node.append(input);
  input.focus();
  await act(async () => {
    key();
  });
  expect(useUIStore.getState().instagramExportRequest).toBeNull();
  input.blur();
  await act(async () => {
    useUIStore.setState({ confirmModalState: { ...ui.confirmModalState, isOpen: true } });
    key();
  });
  expect(useUIStore.getState().instagramExportRequest).toBeNull();
  await act(async () => {
    useUIStore.setState({ confirmModalState: { ...ui.confirmModalState, isOpen: false } });
    key();
  });
  expect(useUIStore.getState().instagramExportRequest).not.toBeNull();
  await act(async () => {
    useUIStore.setState({ instagramExportRequest: null });
    useSettingsStore.setState({
      appSettings: {
        ...(settings.appSettings ?? { lastRootPath: null, theme: Theme.Dark }),
        keybinds: { export_instagram: ['ctrl', 'shift', 'KeyJ'] },
      },
    });
  });
  await act(async () => {
    key();
  });
  expect(useUIStore.getState().instagramExportRequest).toBeNull();
  await act(async () => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'J',
        code: 'KeyJ',
        ctrlKey: true,
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  expect(useUIStore.getState().instagramExportRequest).not.toBeNull();
});
