// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Invokes, SelectedImage, type ImageFile, type AppSettings } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { mockCommand, invoke } from '../test/tauriMock';
import { useSettingsStore } from '../store/useSettingsStore';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { globalImageCache } from '../utils/ImageLRUCache';
import { AppNavigationProps, useAppNavigation } from './useAppNavigation';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialLibraryState = useLibraryStore.getState();
const initialUiState = useUIStore.getState();
const initialSettingsState = useSettingsStore.getState();
vi.mock('@tauri-apps/api/path', () => ({
  resolve: async (path: string) => (path.startsWith('/') ? path : '/cwd/' + path),
  dirname: async (path: string) => path.slice(0, path.lastIndexOf('/')),
  homeDir: async () => '/home/test',
}));

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

describe('external file folder context (CLI/Open With and MCP)', () => {
  const file = (path: string) => ({ path, is_raw: true, rating: 2 }) as ImageFile;
  function setup() {
    useSettingsStore.setState({
      appSettings: { rootFolders: ['/configured'], libraryViewMode: 'flat' } as AppSettings,
    });
    mockCommand(Invokes.GetFolderTree, ({ path } = {}) => ({ path, name: 'Folder', children: [] }));
    mockCommand(Invokes.ReadExifForPaths, () => ({}));
  }
  it('fills a fresh filmstrip from the opened file folder before selecting the photo', async () => {
    setup();
    const files = [file('/photos/a.ARW'), file('/photos/b.ARW')];
    mockCommand(Invokes.ListImagesInDir, () => files);
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      await act(async () => navigation.handleOpenImage('/photos/b.ARW'));
      expect(useLibraryStore.getState()).toMatchObject({
        imageList: files,
        currentFolderPath: '/photos',
        rootPaths: ['/photos'],
        libraryActivePath: '/photos/b.ARW',
      });
      expect(useEditorStore.getState().selectedImage?.path).toBe('/photos/b.ARW');
      expect(useUIStore.getState().activeView).toBe('editor');
      expect(useSettingsStore.getState().appSettings?.rootFolders).toEqual(['/configured']);
    } finally {
      await unmount();
    }
  });
  it('resolves a relative CLI path before loading its sibling filmstrip', async () => {
    setup();
    mockCommand(Invokes.ListImagesInDir, () => [file('/cwd/relative.ARW')]);
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      await act(async () => navigation.handleOpenImage('relative.ARW'));
      expect(useLibraryStore.getState().currentFolderPath).toBe('/cwd');
      expect(useEditorStore.getState().selectedImage?.path).toBe('/cwd/relative.ARW');
    } finally {
      await unmount();
    }
  });
  it('keeps an existing album or recursive context when the requested original or VC is already listed', async () => {
    setup();
    const files = [file('/photos/a.ARW'), file('/other/b.ARW')];
    useLibraryStore.setState({ imageList: files, currentFolderPath: 'album:test', activeAlbumId: 'test' });
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      await act(async () => navigation.handleOpenImage('/photos/a.ARW?vc=copy'));
      expect(useLibraryStore.getState()).toMatchObject({
        imageList: files,
        currentFolderPath: 'album:test',
        activeAlbumId: 'test',
      });
      expect(invoke.mock.calls.some(([name]) => name === Invokes.ListImagesInDir)).toBe(false);
    } finally {
      await unmount();
    }
  });
  it('discards an older folder response when another external file opens first', async () => {
    setup();
    let resolveA!: (value: ImageFile[]) => void;
    const pendingA = new Promise<ImageFile[]>((resolve) => {
      resolveA = resolve;
    });
    const filesB = [file('/B/b.ARW')];
    mockCommand(Invokes.ListImagesInDir, ({ path } = {}) => (path === '/A' ? pendingA : filesB));
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      let first!: Promise<void>;
      await act(async () => {
        first = navigation.handleOpenImage('/A/a.ARW');
        await Promise.resolve();
      });
      await act(async () => navigation.handleOpenImage('/B/b.ARW'));
      await act(async () => {
        resolveA([file('/A/a.ARW')]);
        await first;
      });
      expect(useLibraryStore.getState().imageList).toEqual(filesB);
      expect(useLibraryStore.getState().currentFolderPath).toBe('/B');
      expect(useEditorStore.getState().selectedImage?.path).toBe('/B/b.ARW');
    } finally {
      await unmount();
    }
  });
  it('keeps a later manual selection when an external folder read completes', async () => {
    setup();
    let resolve!: (value: ImageFile[]) => void;
    mockCommand(
      Invokes.ListImagesInDir,
      () =>
        new Promise<ImageFile[]>((done) => {
          resolve = done;
        }),
    );
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      let pending!: Promise<void>;
      await act(async () => {
        pending = navigation.handleOpenImage('/A/a.ARW');
        await Promise.resolve();
      });
      await act(async () => navigation.handleImageSelect('/B/b.ARW'));
      await act(async () => {
        resolve([file('/A/a.ARW')]);
        await pending;
      });
      expect(useEditorStore.getState().selectedImage?.path).toBe('/B/b.ARW');
      expect(useLibraryStore.getState().imageList).toEqual([]);
    } finally {
      await unmount();
    }
  });
  it('does not leave or weaken read-only card mode for an outside file', async () => {
    setup();
    useLibraryStore.setState({ cardBrowseRoot: '/card', rootPaths: ['/card'] });
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      await expect(navigation.handleOpenImage('/elsewhere/a.ARW')).rejects.toThrow('read-only card');
      expect(useLibraryStore.getState().cardBrowseRoot).toBe('/card');
      expect(invoke.mock.calls.some(([name]) => name === Invokes.ListImagesInDir)).toBe(false);
    } finally {
      await unmount();
    }
  });
  it('does not let an older same-folder EXIF read remove a newer externally opened photo from the filmstrip', async () => {
    setup();
    useLibraryStore.setState({ sortCriteria: { ...useLibraryStore.getState().sortCriteria, key: 'iso' } });
    const a = file('/A/a.ARW'),
      b = file('/A/b.ARW');
    let listings = 0,
      exifCalls = 0;
    let release!: (v: unknown) => void;
    const pending = new Promise((r) => {
      release = r;
    });
    mockCommand(Invokes.ListImagesInDir, () => (++listings === 1 ? [a] : [a, b]));
    mockCommand(Invokes.ReadExifForPaths, () => (++exifCalls === 1 ? pending : {}));
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      let first!: Promise<void>;
      await act(async () => {
        first = navigation.handleOpenImage(a.path);
      });
      await waitUntil(() => exifCalls === 1);
      await act(async () => navigation.handleOpenImage(b.path));
      expect(useEditorStore.getState().selectedImage?.path).toBe(b.path);
      const latest = useLibraryStore.getState().imageList;
      expect(latest.map((f) => f.path)).toContain(b.path);
      await act(async () => {
        release({});
        await first;
      });
      expect(useLibraryStore.getState().imageList).toBe(latest);
    } finally {
      release({});
      await unmount();
    }
  });
  it('does not replace the filmstrip after a newer manual image choice during external EXIF loading', async () => {
    setup();
    const a = file('/A/a.ARW'),
      b = file('/B/b.ARW');
    const previous = [b];
    useLibraryStore.setState({
      imageList: previous,
      currentFolderPath: '/B',
      sortCriteria: { ...useLibraryStore.getState().sortCriteria, key: 'iso' },
    });
    let release!: (v: unknown) => void,
      started = false;
    const pending = new Promise((r) => {
      release = r;
    });
    mockCommand(Invokes.ListImagesInDir, () => [a]);
    mockCommand(Invokes.ReadExifForPaths, () => {
      started = true;
      return pending;
    });
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      let first!: Promise<void>;
      await act(async () => {
        first = navigation.handleOpenImage(a.path);
      });
      await waitUntil(() => started);
      await act(async () => navigation.handleImageSelect(b.path));
      await act(async () => {
        release({});
        await first;
      });
      expect(useEditorStore.getState().selectedImage?.path).toBe(b.path);
      expect(useLibraryStore.getState().imageList).toBe(previous);
    } finally {
      release({});
      await unmount();
    }
  });

  it('checks card boundaries even when an old listed image is outside the card', async () => {
    setup();
    const previous = [file('/outside/a.ARW')];
    useLibraryStore.setState({ cardBrowseRoot: '/card', rootPaths: ['/card'], imageList: previous });
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      await expect(navigation.handleOpenImage('/outside/a.ARW')).rejects.toThrow('read-only card');
      expect(useLibraryStore.getState().cardBrowseRoot).toBe('/card');
      expect(useEditorStore.getState().selectedImage).toBeNull();
    } finally {
      await unmount();
    }
  });
  it('still fills background EXIF after handing a non-EXIF-sorted folder to the editor', async () => {
    setup();
    const photo = file('/A/a.ARW');
    mockCommand(Invokes.ListImagesInDir, () => [photo]);
    mockCommand(Invokes.ReadExifForPaths, () => ({ [photo.path]: { ISO: '800' } }));
    const { navigation, unmount } = await mountNavigation(createRefs());
    try {
      await act(async () => navigation.handleOpenImage(photo.path));
      expect(useEditorStore.getState().selectedImage?.path).toBe(photo.path);
      await act(async () => new Promise((resolve) => setTimeout(resolve, 650)));
      expect(useLibraryStore.getState().imageList[0].exif).toEqual({ ISO: '800' });
      expect(useLibraryStore.getState().currentFolderPath).toBe('/A');
    } finally {
      await unmount();
    }
  });
});
