// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { useAppNavigation, type AppNavigationProps } from './useAppNavigation';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { mockCommand } from '../test/tauriMock';
import { Invokes, type SelectedImage } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { globalImageCache } from '../utils/ImageLRUCache';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialEditor = useEditorStore.getState();
const initialLibrary = useLibraryStore.getState();
const initialUI = useUIStore.getState();
let unmount: (() => Promise<void>) | undefined;
afterEach(async () => {
  await unmount?.();
  unmount = undefined;
  globalImageCache.clear();
  useEditorStore.setState(initialEditor, true);
  useLibraryStore.setState(initialLibrary, true);
  useUIStore.setState(initialUI, true);
});
function image(path: string): SelectedImage {
  return { path, width: 4608, height: 3072, isReady: true, isRaw: true, exif: null, thumbnailUrl: '' };
}
function cache(path: string) {
  globalImageCache.set(path, {
    selectedImage: image(path),
    adjustments: INITIAL_ADJUSTMENTS,
    originalSize: { width: 4608, height: 3072 },
    previewSize: { width: 1024, height: 683 },
    finalPreviewUrl: null,
    uncroppedPreviewUrl: null,
    histogram: null,
    waveform: null,
  });
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
async function harness() {
  useEditorStore.getState().setEditor({ selectedImage: image('/A.ARW'), zoom: 4, zoomRequest: null });
  mockCommand(Invokes.LoadImage, () => new Promise(() => {}));
  mockCommand(Invokes.LoadMetadata, () => ({ adjustments: { is_null: true } }));
  const getZoomIntent = vi.fn((): number | null => 2);
  const refs: AppNavigationProps['refs'] = {
    transformWrapperRef: { current: { resetTransform: vi.fn(), getZoomIntent } },
    preloadedDataRef: { current: null },
    cachedEditStateRef: { current: null },
    selectedImagePathRef: { current: '/A.ARW' },
    isBackendReadyRef: { current: true },
    latestRenderedJobIdRef: { current: 0 },
    previewJobIdRef: { current: 0 },
    currentResRef: { current: 0 },
    prevAdjustmentsRef: { current: null },
  };
  let navigation!: ReturnType<typeof useAppNavigation>;
  function Harness() {
    navigation = useAppNavigation({ clearThumbnailQueue: vi.fn(), refs });
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => root.render(createElement(Harness)));
  unmount = async () => {
    await act(async () => root.unmount());
  };
  return { navigation, getZoomIntent };
}

it('keeps the last arrow-selected photo when earlier cached checks finish out of order', async () => {
  cache('/B.ARW');
  cache('/C.ARW');
  const b = deferred<boolean>(),
    c = deferred<boolean>();
  mockCommand('is_image_cached', ({ path } = {}) => (path === '/B.ARW' ? b.promise : c.promise));
  const { navigation } = await harness();
  const first = navigation.handleImageSelect('/B.ARW');
  const last = navigation.handleImageSelect('/C.ARW');
  await act(async () => {
    c.resolve(true);
    await last;
  });
  expect(useEditorStore.getState().selectedImage?.path).toBe('/C.ARW');
  await act(async () => {
    b.resolve(true);
    await first;
  });
  expect(useEditorStore.getState().selectedImage?.path).toBe('/C.ARW');
  expect(useEditorStore.getState().zoomRequest).toMatchObject({ imagePath: '/C.ARW', percent: 2 });
});

it('cancels a pending arrow away when the latest selection returns to the visible photo', async () => {
  cache('/B.ARW');
  const b = deferred<boolean>();
  mockCommand('is_image_cached', () => b.promise);
  const { navigation } = await harness();
  const pending = navigation.handleImageSelect('/B.ARW');
  await navigation.handleImageSelect('/A.ARW');
  await act(async () => {
    b.resolve(true);
    await pending;
  });
  expect(useEditorStore.getState().selectedImage?.path).toBe('/A.ARW');
});

it('carries the requested physical percentage through photos whose fitted geometry is not ready', async () => {
  const { navigation, getZoomIntent } = await harness();
  await act(async () => navigation.handleImageSelect('/B.ARW'));
  expect(useEditorStore.getState().selectedImage?.isReady).toBe(false);
  getZoomIntent.mockReturnValue(0.68); // Intermediate geometry must not replace the pending200% intent.
  await act(async () => navigation.handleImageSelect('/C.ARW'));
  expect(useEditorStore.getState().zoomRequest).toMatchObject({ imagePath: '/C.ARW', percent: 2 });
});
