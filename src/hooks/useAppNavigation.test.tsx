// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it } from 'vitest';
import { Invokes, SelectedImage, Panel, type AppSettings } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { createLightroomWorkspace } from '../utils/lightroomWorkspace';
import { mockCommand } from '../test/tauriMock';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { globalImageCache } from '../utils/ImageLRUCache';
import { AppNavigationProps, useAppNavigation } from './useAppNavigation';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialLibraryState = useLibraryStore.getState();
const initialUiState = useUIStore.getState();
const initialSettingsState = useSettingsStore.getState();

afterEach(() => {
  globalImageCache.clear();
  useEditorStore.setState(initialEditorState, true);
  useLibraryStore.setState(initialLibraryState, true);
  useUIStore.setState(initialUiState, true);
  useSettingsStore.setState(initialSettingsState, true);
});

function selectedImage(path: string, usedEmbeddedPreview: boolean): SelectedImage {
  return {
    exif: {},
    height: 480,
    isRaw: true,
    isReady: true,
    path,
    thumbnailUrl: '',
    usedEmbeddedPreview,
    width: 640,
  };
}

function cacheImage(path: string, usedEmbeddedPreview: boolean) {
  globalImageCache.set(path, {
    adjustments: INITIAL_ADJUSTMENTS,
    histogram: null,
    waveform: null,
    finalPreviewUrl: null,
    uncroppedPreviewUrl: null,
    selectedImage: selectedImage(path, usedEmbeddedPreview),
    originalSize: { width: 640, height: 480 },
    previewSize: { width: 0, height: 0 },
  });
}

function createRefs(): AppNavigationProps['refs'] {
  return {
    transformWrapperRef: { current: null },
    preloadedDataRef: { current: null },
    cachedEditStateRef: { current: null },
    selectedImagePathRef: { current: null },
    isBackendReadyRef: { current: false },
    latestRenderedJobIdRef: { current: 0 },
    previewJobIdRef: { current: 0 },
    currentResRef: { current: 0 },
    prevAdjustmentsRef: { current: null },
  };
}

async function waitUntil(predicate: () => boolean) {
  for (let i = 0; i < 20; i += 1) {
    if (predicate()) return;
    await act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
  }
  expect(predicate()).toBe(true);
}

async function mountNavigation(refs: AppNavigationProps['refs']) {
  let navigation!: ReturnType<typeof useAppNavigation>;
  function Harness() {
    navigation = useAppNavigation({ clearThumbnailQueue: () => {}, refs });
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => root.render(createElement(Harness)));
  return { navigation, unmount: () => act(async () => root.unmount()) };
}

describe('useAppNavigation cached image refresh', () => {
  it('updates the preview warning from a fresh load result on a cache hit', async () => {
    const path = '/photos/cached.NEF';
    cacheImage(path, false);
    mockCommand('is_image_cached', () => true);
    mockCommand(Invokes.LoadImage, () => ({
      width: 640,
      height: 480,
      used_embedded_preview: true,
    }));
    mockCommand(Invokes.LoadMetadata, () => ({ adjustments: { is_null: true } }));

    const refs = createRefs();
    const { navigation, unmount } = await mountNavigation(refs);
    try {
      await act(async () => navigation.handleImageSelect(path));
      await waitUntil(() => useEditorStore.getState().selectedImage?.usedEmbeddedPreview === true);
      expect(useEditorStore.getState().selectedImage?.path).toBe(path);
    } finally {
      await unmount();
    }
  });

  it('ignores a late cached load result after navigation to another photo', async () => {
    const pathA = '/photos/cached.NEF';
    const pathB = '/photos/next.NEF';
    cacheImage(pathA, false);
    mockCommand('is_image_cached', ({ path } = {}) => path === pathA);
    mockCommand(Invokes.LoadMetadata, () => ({ adjustments: { is_null: true } }));

    let resolveLoadImage!: (result: unknown) => void;
    const pendingLoad = new Promise((resolve) => {
      resolveLoadImage = resolve;
    });
    mockCommand(Invokes.LoadImage, () => pendingLoad);

    const refs = createRefs();
    const { navigation, unmount } = await mountNavigation(refs);
    try {
      await act(async () => navigation.handleImageSelect(pathA));
      expect(useEditorStore.getState().selectedImage?.path).toBe(pathA);

      await act(async () => navigation.handleImageSelect(pathB));
      expect(useEditorStore.getState().selectedImage?.path).toBe(pathB);
      expect(useEditorStore.getState().selectedImage?.isReady).toBe(false);

      await act(async () => {
        resolveLoadImage({ width: 640, height: 480, used_embedded_preview: true });
        await pendingLoad;
        await new Promise((resolve) => setTimeout(resolve, 0));
      });
      await waitUntil(() => refs.selectedImagePathRef.current === pathB);

      expect(useEditorStore.getState().selectedImage?.path).toBe(pathB);
      expect(useEditorStore.getState().selectedImage?.usedEmbeddedPreview).toBeUndefined();
    } finally {
      await unmount();
    }
  });
});

function setWorkspaceView(activeView: 'library' | 'editor') {
  const layout = createLightroomWorkspace(initialUiState);
  useUIStore.setState({
    ...layout,
    activeView,
    panelLayout: { ...layout.panelLayout, bottom: layout.panelLayout.bottom ?? [] },
    activePanels: { ...layout.activePanels, bottom: layout.activePanels.bottom ?? null },
    panelSwitcherPlacement: {
      ...layout.panelSwitcherPlacement,
      bottom: layout.panelSwitcherPlacement.bottom ?? 'top',
    },
  });
}

describe('workspace module panel routing', () => {
  it.each(['rapidroom', 'lightroom'] as const)(
    'selects the Library module panel only in %s mode',
    async (workspace) => {
      const image = selectedImage('/photos/current.NEF', false);
      const before = { ...INITIAL_ADJUSTMENTS, exposure: 0.4 };
      useEditorStore.setState({ selectedImage: image, adjustments: before });
      useSettingsStore.setState({
        appSettings: { ...initialSettingsState.appSettings, workspacePreset: workspace } as AppSettings,
      });
      setWorkspaceView('editor');
      useUIStore.getState().setPanel(Panel.Metadata);
      const { navigation, unmount } = await mountNavigation(createRefs());
      try {
        await act(async () => navigation.handleBackToLibrary());
        expect(useUIStore.getState().activeView).toBe('library');
        expect(useUIStore.getState().activePanels.rightTop).toBe(
          workspace === 'lightroom' ? Panel.Adjustments : Panel.Metadata,
        );
        expect(useEditorStore.getState().adjustments).toEqual(before);
        expect(useLibraryStore.getState().libraryActivePath).toBe(image.path);
      } finally {
        await unmount();
      }
    },
  );
  it.each(['rapidroom', 'lightroom'] as const)(
    'selects the Develop module panel only in %s mode, including the same photo',
    async (workspace) => {
      const image = selectedImage('/photos/current.NEF', false);
      const before = { ...INITIAL_ADJUSTMENTS, exposure: 0.4 };
      useEditorStore.setState({ selectedImage: image, adjustments: before });
      useSettingsStore.setState({
        appSettings: { ...initialSettingsState.appSettings, workspacePreset: workspace } as AppSettings,
      });
      setWorkspaceView('library');
      useUIStore.getState().setPanel(Panel.Metadata);
      const { navigation, unmount } = await mountNavigation(createRefs());
      try {
        await act(async () => navigation.handleImageSelect(image.path, true));
        expect(useUIStore.getState().activeView).toBe('editor');
        expect(useUIStore.getState().activePanels.rightTop).toBe(
          workspace === 'lightroom' ? Panel.Adjustments : Panel.Metadata,
        );
        expect(useEditorStore.getState().adjustments).toEqual(before);
      } finally {
        await unmount();
      }
    },
  );
});
