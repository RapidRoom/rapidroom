import { useCallback, useRef, type RefObject } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { homeDir } from '@tauri-apps/api/path';
import { toast } from 'react-toastify';
import i18n from 'i18next';
import { useLibraryStore } from '../store/useLibraryStore';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { useProcessStore } from '../store/useProcessStore';
import { useSettingsStore } from '../store/useSettingsStore';
import {
  Invokes,
  LibraryViewMode,
  ImageFile,
  AlbumItem,
  SelectedImage,
  HistogramData,
  WaveformData,
} from '../components/ui/AppProperties';
import type { FolderTree } from '../components/ui/AppProperties';
import { Adjustments, INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from '../utils/adjustments';
import { globalImageCache } from '../utils/ImageLRUCache';
import { debouncedSave, debouncedSetHistory } from './useEditorActions';
import { clearLibrarySelection } from './useLibraryActions';
import { computeSortedLibrary } from './useSortedLibrary';
import type { ImageDimensions } from './useImageRenderSize';

interface CachedEditorState {
  adjustments: Adjustments;
  finalPreviewUrl: string | null;
  histogram: HistogramData | null;
  originalSize: ImageDimensions;
  previewSize: ImageDimensions;
  selectedImage: SelectedImage;
  uncroppedPreviewUrl: string | null;
  waveform: WaveformData | null;
}

interface PreloadedLibraryData {
  currentPath?: string;
  images?: Promise<ImageFile[]>;
  rootPaths?: string[];
  trees?: Promise<FolderTree[]>;
}

interface LoadedImageMetadata {
  adjustments: (Adjustments & { is_null?: boolean }) | null;
}

interface LoadedImageResult {
  height: number;
  used_embedded_preview: boolean;
  width: number;
}

interface PreviousAdjustments {
  adjustments: Adjustments;
  path: string;
}

interface TransformWrapper {
  getZoomIntent?(): number | null;
  resetTransform(duration?: number): void;
}

export interface AppNavigationProps {
  clearThumbnailQueue: () => void;
  refs: {
    transformWrapperRef: RefObject<TransformWrapper | null>;
    preloadedDataRef: RefObject<PreloadedLibraryData | null>;
    cachedEditStateRef: RefObject<CachedEditorState | null>;
    selectedImagePathRef: RefObject<string | null>;
    isBackendReadyRef: RefObject<boolean>;
    latestRenderedJobIdRef: RefObject<number>;
    previewJobIdRef: RefObject<number>;
    currentResRef: RefObject<number>;
    prevAdjustmentsRef: RefObject<PreviousAdjustments | null>;
  };
}

const loadExifForImages = async (
  files: ImageFile[],
  expectedFolderPath: string | null,
  sortKey: string,
  setLibrary: ReturnType<typeof useLibraryStore.getState>['setLibrary'],
) => {
  const exifSortKeys = ['date_taken', 'iso', 'shutter_speed', 'aperture', 'focal_length'];
  const isExifSortActive = exifSortKeys.includes(sortKey);

  if (files.length === 0) {
    setLibrary({ imageList: files });
    return;
  }

  const paths = files.map((f: ImageFile) => f.path);

  if (isExifSortActive) {
    let combinedExifMap: Record<string, Record<string, string>> = {};
    const chunkSize = 100;

    for (let i = 0; i < paths.length; i += chunkSize) {
      const chunk = paths.slice(i, i + chunkSize);
      try {
        const chunkExif = await invoke<Record<string, Record<string, string>>>(Invokes.ReadExifForPaths, {
          paths: chunk,
        });
        combinedExifMap = { ...combinedExifMap, ...chunkExif };
      } catch (err) {
        console.error('Failed to read EXIF chunk:', err);
      }
    }

    const finalImageList = files.map((image) => ({
      ...image,
      exif: combinedExifMap[image.path] || image.exif || null,
    }));
    setLibrary({ imageList: finalImageList });
  } else {
    setLibrary({ imageList: files });

    setTimeout(() => {
      const fetchExifInChunks = async () => {
        const chunkSize = 50;
        for (let i = 0; i < paths.length; i += chunkSize) {
          if (useLibraryStore.getState().currentFolderPath !== expectedFolderPath) break;

          const chunk = paths.slice(i, i + chunkSize);
          try {
            const chunkExif = await invoke<Record<string, Record<string, string>>>(Invokes.ReadExifForPaths, {
              paths: chunk,
            });
            setLibrary((state) => ({
              imageList: state.imageList.map((image: ImageFile) => ({
                ...image,
                exif: chunkExif[image.path] || image.exif || null,
              })),
            }));
            await new Promise((resolve) => setTimeout(resolve, 50));
          } catch (err) {
            console.error('Failed to read EXIF chunk:', err);
          }
        }
      };
      fetchExifInChunks();
    }, 500);
  }
};

export function useAppNavigation({ clearThumbnailQueue, refs }: AppNavigationProps) {
  const {
    transformWrapperRef,
    preloadedDataRef,
    cachedEditStateRef,
    selectedImagePathRef,
    isBackendReadyRef,
    latestRenderedJobIdRef,
    previewJobIdRef,
    currentResRef,
    prevAdjustmentsRef,
  } = refs;

  const handleGoHome = useCallback(() => {
    void invoke(Invokes.SetCardBrowseRoot, { path: null });
    useLibraryStore.getState().setLibrary({
      rootPaths: [],
      cardBrowseRoot: null,
      currentFolderPath: null,
      activeAlbumId: null,
      imageList: [],
      imageRatings: {},
      folderTrees: [],
      multiSelectedPaths: [],
      libraryActivePath: null,
      expandedFolders: new Set(),
    });
    useUIStore.getState().setUI({ isLibraryExportPanelVisible: false });
  }, []);

  const handleBackToLibrary = useCallback(() => {
    const { selectedImage } = useEditorStore.getState();
    const { setLibrary } = useLibraryStore.getState();
    const { setUI } = useUIStore.getState();

    if (!useEditorStore.getState().softProof && selectedImage?.path && cachedEditStateRef.current) {
      globalImageCache.set(selectedImage.path, cachedEditStateRef.current);
    }
    if (transformWrapperRef.current) {
      transformWrapperRef.current.resetTransform(0);
    }
    useEditorStore.getState().setEditor({
      zoom: 1,
      showOriginal: false,
      previewOverride: null,
    });

    debouncedSave.flush();
    debouncedSetHistory.cancel();

    const lastActivePath = selectedImage?.path ?? null;
    const isStillVisible =
      lastActivePath !== null &&
      computeSortedLibrary(useLibraryStore.getState(), useSettingsStore.getState()).some(
        (img) => img.path === lastActivePath,
      );

    if (isStillVisible) {
      setLibrary({ libraryActivePath: lastActivePath });
    } else {
      clearLibrarySelection();
    }
    setUI({ activeView: 'library', slideDirection: 1 });
  }, [refs]);

  const imageSelectionGenerationRef = useRef(0);
  const committedImageGenerationRef = useRef(0);

  const handleImageSelect = useCallback(
    async (path: string, openInEditor: boolean = true) => {
      const { selectedImage, isSliderDragging, resetHistory, setEditor } = useEditorStore.getState();
      const { setLibrary, multiSelectedPaths } = useLibraryStore.getState();
      const { setUI } = useUIStore.getState();

      if (openInEditor) {
        setUI({ activeView: 'editor' });
      }

      const generation = ++imageSelectionGenerationRef.current;
      // Keyboard navigation follows the requested photo even while its cache lookup awaits.
      const newMultiSelectedPaths = multiSelectedPaths.includes(path) ? multiSelectedPaths : [path];

      setLibrary({
        multiSelectedPaths: newMultiSelectedPaths,
        libraryActivePath: path,
        selectionAnchorPath: path,
      });

      if (selectedImage?.path === path) return;

      useEditorStore.getState().patchesSentToBackend.clear();
      debouncedSave.flush();
      debouncedSetHistory.cancel();

      if (!useEditorStore.getState().softProof && selectedImage?.path && cachedEditStateRef.current) {
        globalImageCache.set(selectedImage.path, cachedEditStateRef.current);
      }

      const cachedThumb = useProcessStore.getState().thumbnails[path];
      const cachedMedium = useProcessStore.getState().mediumThumbnails[path] || cachedThumb;

      const cached = globalImageCache.get(path);
      const isFrontendCached = cached !== undefined && Boolean(cached.selectedImage?.isReady);
      const isCachedInBackend = isFrontendCached
        ? await invoke<boolean>('is_image_cached', { path }).catch(() => false)
        : false;

      // A cached selection can await the backend while later arrow presses overtake it.
      if (generation !== imageSelectionGenerationRef.current) return;
      const pendingZoom = useEditorStore.getState().zoomRequest;
      const currentPath = useEditorStore.getState().selectedImage?.path;
      const percent =
        pendingZoom && pendingZoom.imagePath === currentPath
          ? pendingZoom.percent
          : (transformWrapperRef.current?.getZoomIntent?.() ?? null);
      const zoomRequest = { imagePath: path, percent, resetPosition: true };
      committedImageGenerationRef.current = generation;
      const isCurrentSelection = () =>
        committedImageGenerationRef.current === generation && selectedImagePathRef.current === path;

      const hasDifferentResolution =
        cached &&
        (useEditorStore.getState().originalSize.width !== cached.originalSize.width ||
          useEditorStore.getState().originalSize.height !== cached.originalSize.height);

      if (!isCachedInBackend || hasDifferentResolution) {
        setEditor({ hasRenderedFirstFrame: false });
      }

      selectedImagePathRef.current = path;

      setEditor({
        showOriginal: false,
        activeMaskId: null,
        activeMaskContainerId: null,
        activeAiPatchContainerId: null,
        activeAiSubMaskId: null,
        isWbPickerActive: false,
        mixerPickerProperty: null,
        isRelightPickerActive: false,
        activeRelightLightId: null,
        previewOverride: null,
      });

      setUI({
        isLibraryExportPanelVisible: false,
        compactEditorPanelHeightOverride: null,
      });

      if (isFrontendCached) {
        setEditor({
          zoomRequest,
          selectedImage: {
            ...cached.selectedImage,
            thumbnailUrl: cachedMedium || cached.selectedImage.thumbnailUrl,
          },
          originalSize: cached.originalSize,
          previewSize: cached.previewSize,
          histogram: cached.histogram,
          waveform: cached.waveform,
          finalPreviewUrl: useEditorStore.getState().softProof ? null : cached.finalPreviewUrl,
          uncroppedAdjustedPreviewUrl: cached.uncroppedPreviewUrl,
        });

        setEditor({ adjustments: cached.adjustments });
        resetHistory(cached.adjustments);
        prevAdjustmentsRef.current = { path, adjustments: cached.adjustments };

        setLibrary({ isViewLoading: false });

        latestRenderedJobIdRef.current = previewJobIdRef.current;
        isBackendReadyRef.current = false;
        currentResRef.current = Infinity;

        invoke<LoadedImageResult>(Invokes.LoadImage, { path })
          .then((_result) => {
            if (!isCurrentSelection()) return;
            isBackendReadyRef.current = true;
            currentResRef.current = 0;
            setEditor((state) => {
              if (state.selectedImage?.path !== path) return state;
              return {
                originalSize: { width: _result.width, height: _result.height },
                selectedImage: {
                  ...state.selectedImage,
                  usedEmbeddedPreview: _result.used_embedded_preview,
                },
              };
            });
          })
          .catch((err: unknown) => {
            if (!isCurrentSelection() || String(err).includes('cancelled')) return;
            console.error('Background load_image failed on cache hit:', err);
            isBackendReadyRef.current = true;
            currentResRef.current = 0;
          });

        invoke<LoadedImageMetadata>(Invokes.LoadMetadata, { path })
          .then((metadata) => {
            if (!isCurrentSelection()) return;
            let freshAdjustments: Adjustments;
            if (metadata.adjustments && !metadata.adjustments.is_null) {
              freshAdjustments = normalizeLoadedAdjustments(metadata.adjustments);
            } else {
              freshAdjustments = { ...INITIAL_ADJUSTMENTS };
            }
            if (freshAdjustments.aspectRatio == null && cached.adjustments.aspectRatio != null) {
              freshAdjustments.aspectRatio = cached.adjustments.aspectRatio;
            }
            if (!isSliderDragging && JSON.stringify(cached.adjustments) !== JSON.stringify(freshAdjustments)) {
              setEditor({ adjustments: freshAdjustments });
              resetHistory(freshAdjustments);
              prevAdjustmentsRef.current = { path, adjustments: freshAdjustments };
              globalImageCache.set(path, { ...cached, adjustments: freshAdjustments });
            }
          })
          .catch((err) => console.error('Failed background metadata sync on cache hit:', err));

        return;
      }

      isBackendReadyRef.current = true;

      const imageFile = useLibraryStore.getState().imageList.find((img) => img.path === path);
      setEditor({
        zoomRequest,
        selectedImage: {
          exif: null,
          group_id: imageFile?.group_id ?? null,
          height: 0,
          isRaw: false,
          isReady: false,
          metadata: null,
          path,
          thumbnailUrl: cachedMedium,
          width: 0,
        },
        originalSize: { width: 0, height: 0 },
        previewSize: { width: 0, height: 0 },
        histogram: null,
        waveform: null,
        uncroppedAdjustedPreviewUrl: null,
      });

      setLibrary({ isViewLoading: true });

      setEditor((state) => {
        const prev = state.finalPreviewUrl;
        if (prev?.startsWith('blob:') && !globalImageCache.isProtected(prev)) {
          setTimeout(() => {
            if (!globalImageCache.isProtected(prev)) {
              URL.revokeObjectURL(prev);
            }
          }, 250);
        }
        return { finalPreviewUrl: null };
      });

      setEditor((state) => {
        if (state.interactivePatch?.url) URL.revokeObjectURL(state.interactivePatch.url);
        return { interactivePatch: null };
      });
    },
    [refs],
  );

  const handleSelectSubfolder = useCallback(
    async (
      path: string | null,
      isNewRoot = false,
      preloadedImages?: ImageFile[],
      expandParents = true,
      preserveEditor = false,
      skipHistory = false,
    ) => {
      const { appSettings, handleSettingsChange } = useSettingsStore.getState();
      const { pinnedFolders } = appSettings || { pinnedFolders: [] };
      const { setLibrary, sortCriteria } = useLibraryStore.getState();
      const { setUI } = useUIStore.getState();
      const { setProcess } = useProcessStore.getState();
      const { selectedImage, resetHistory, setEditor } = useEditorStore.getState();
      const libraryViewMode = appSettings?.libraryViewMode;

      if (!skipHistory && path) {
        useLibraryStore.getState().pushNavHistory({ type: 'folder', path });
      }

      if (!preserveEditor) {
        await invoke('cancel_thumbnail_generation');
        clearThumbnailQueue();
        setLibrary({ isViewLoading: true, activeAlbumId: null, libraryScrollTop: 0 });
        setProcess({ thumbnails: {} });
        globalImageCache.clear();
        setUI({ activeView: 'library' });
      } else {
        setLibrary({ isViewLoading: true });
      }

      try {
        const { rootPaths, expandedFolders: currentExpandedFolders } = useLibraryStore.getState();
        let newExpandedFolders = new Set(currentExpandedFolders);

        if (isNewRoot && path) {
          newExpandedFolders = new Set([path]);
          if (appSettings) {
            handleSettingsChange({ ...appSettings, lastRootPath: path });
          }
        } else if (path && expandParents) {
          const allRoots = [...(rootPaths || []), ...(pinnedFolders || [])].filter(Boolean) as string[];
          const relevantRoot = allRoots.find((r) => path.startsWith(r));

          if (relevantRoot) {
            const separator = path.includes('/') ? '/' : '\\';
            const parentSeparatorIndex = path.lastIndexOf(separator);

            if (parentSeparatorIndex > -1 && path.length > relevantRoot.length) {
              let current = path.substring(0, parentSeparatorIndex);
              while (current && current.length >= relevantRoot.length) {
                newExpandedFolders.add(current);
                const nextParentIndex = current.lastIndexOf(separator);
                if (nextParentIndex === -1 || current === relevantRoot) break;
                current = current.substring(0, nextParentIndex);
              }
            }
            newExpandedFolders.add(relevantRoot);
          }
        }

        setLibrary({
          currentFolderPath: path,
          expandedFolders: newExpandedFolders,
          ...(preserveEditor ? {} : { imageList: [], multiSelectedPaths: [], libraryActivePath: null }),
        });

        if (!preserveEditor && selectedImage) {
          debouncedSave.flush();
          debouncedSetHistory.cancel();
          setEditor({ selectedImage: null, finalPreviewUrl: null, uncroppedAdjustedPreviewUrl: null, histogram: null });
          setEditor({ adjustments: INITIAL_ADJUSTMENTS });
          resetHistory(INITIAL_ADJUSTMENTS);
          useEditorStore.getState().patchesSentToBackend.clear();
        }

        const command =
          libraryViewMode === LibraryViewMode.Recursive ? Invokes.ListImagesRecursive : Invokes.ListImagesInDir;

        let files: ImageFile[];
        if (preloadedImages) {
          files = preloadedImages;
        } else {
          files = await invoke(command, { path });
        }

        const initialRatings: Record<string, number> = {};
        files.forEach((f) => {
          if (f.rating !== undefined) {
            initialRatings[f.path] = f.rating;
          }
        });
        setLibrary({ imageRatings: initialRatings });

        await loadExifForImages(files, path, sortCriteria.key, setLibrary);

        if (!preserveEditor && !useLibraryStore.getState().cardBrowseRoot) {
          invoke(Invokes.StartBackgroundIndexing, { folderPath: path }).catch((err) => {
            console.error('Failed to start background indexing:', err);
          });
        }
      } catch (err) {
        console.error('Failed to load folder contents:', err);
        toast.error('Failed to load images from the selected folder.');
      } finally {
        useLibraryStore.getState().setLibrary({ isViewLoading: false });
      }
    },
    [clearThumbnailQueue, refs],
  );

  const handleSelectAlbum = useCallback(
    async (albumId: string, albumName: string, imagePaths: string[], preserveEditor = false, skipHistory = false) => {
      const { setLibrary, sortCriteria } = useLibraryStore.getState();
      const { setUI } = useUIStore.getState();

      if (!skipHistory) {
        useLibraryStore.getState().pushNavHistory({ type: 'album', path: albumId, albumName, images: imagePaths });
      }

      if (!preserveEditor) {
        await invoke('cancel_thumbnail_generation');
        clearThumbnailQueue();
        setLibrary({ libraryScrollTop: 0 });
        globalImageCache.clear();
        setUI({ activeView: 'library' });
      }

      const albumFolderPath = `Album: ${albumName}`;

      setLibrary({
        isViewLoading: true,
        currentFolderPath: albumFolderPath,
        activeAlbumId: albumId,
      });

      try {
        const files: ImageFile[] = await invoke(Invokes.GetAlbumImages, { paths: imagePaths });

        if (imagePaths.length > 0 && files.length === 0) {
          toast.warning(i18n.t('contextMenus.toasts.albumFilesUnreachable'));
        }

        const initialRatings: Record<string, number> = {};
        files.forEach((f) => {
          if (f.rating !== undefined) initialRatings[f.path] = f.rating;
        });

        setLibrary({
          imageRatings: initialRatings,
          ...(preserveEditor ? {} : { multiSelectedPaths: [], libraryActivePath: null }),
        });

        await loadExifForImages(files, albumFolderPath, sortCriteria.key, setLibrary);
      } catch (err) {
        console.error('Failed to load album images:', err);
        toast.error(`Failed to load album: ${err}`);
      } finally {
        setLibrary({ isViewLoading: false });
      }
    },
    [clearThumbnailQueue],
  );

  const handleNavBack = useCallback(async () => {
    const { navHistory, navIndex, setLibrary } = useLibraryStore.getState();

    if (navIndex > 0) {
      const targetIndex = navIndex - 1;
      const target = navHistory[targetIndex];
      setLibrary({ navIndex: targetIndex });

      if (target.type === 'folder') {
        handleSelectSubfolder(target.path, false, undefined, true, false, true);
      } else if (target.type === 'album') {
        handleSelectAlbum(target.path, target.albumName!, target.images!, false, true);
      }
    }
  }, [handleSelectSubfolder, handleSelectAlbum]);

  const handleNavForward = useCallback(async () => {
    const { navHistory, navIndex, setLibrary } = useLibraryStore.getState();

    if (navIndex < navHistory.length - 1) {
      const targetIndex = navIndex + 1;
      const target = navHistory[targetIndex];
      setLibrary({ navIndex: targetIndex });

      if (target.type === 'folder') {
        handleSelectSubfolder(target.path, false, undefined, true, false, true);
      } else if (target.type === 'album') {
        handleSelectAlbum(target.path, target.albumName!, target.images!, false, true);
      }
    }
  }, [handleSelectSubfolder, handleSelectAlbum]);

  // Card mode replaced the library roots with the card, so put the saved roots back.
  const leaveCardMode = async () => {
    const { appSettings } = useSettingsStore.getState();
    const rootFolders: string[] = appSettings?.rootFolders ?? [];
    await invoke(Invokes.SetCardBrowseRoot, { path: null });
    let folderTrees: FolderTree[] = [];
    if (rootFolders.length > 0) {
      folderTrees = await invoke<FolderTree[]>(Invokes.GetPinnedFolderTrees, {
        paths: rootFolders,
        expandedFolders: rootFolders,
        showImageCounts: appSettings?.enableFolderImageCounts || appSettings?.folderTreeSort?.key === 'imageCount',
      }).catch(() => []);
    }
    useLibraryStore.getState().setLibrary({
      cardBrowseRoot: null,
      rootPaths: rootFolders,
      folderTrees,
      expandedFolders: new Set(rootFolders),
    });
  };

  const handleOpenFolder = useCallback(async () => {
    const { osPlatform, appSettings, handleSettingsChange } = useSettingsStore.getState();
    const { setLibrary } = useLibraryStore.getState();
    const isAndroid = osPlatform === 'android';

    try {
      let selectedPath = '';
      if (isAndroid) {
        selectedPath = await invoke<string>(Invokes.GetOrCreateInternalLibraryRoot);
      } else {
        const selected = await open({ directory: true, multiple: false, defaultPath: await homeDir() });
        if (typeof selected === 'string') {
          selectedPath = selected;
        }
      }

      if (selectedPath) {
        if (useLibraryStore.getState().cardBrowseRoot) {
          await leaveCardMode();
        }
        const { rootPaths, folderTrees } = useLibraryStore.getState();
        if (!rootPaths.includes(selectedPath)) {
          const newRootPaths = [...rootPaths, selectedPath];
          setLibrary({ rootPaths: newRootPaths });

          if (appSettings) {
            handleSettingsChange({ ...appSettings, rootFolders: newRootPaths });
          }

          setLibrary({ isTreeLoading: true });
          try {
            const newTree = await invoke<FolderTree>(Invokes.GetFolderTree, {
              path: selectedPath,
              expandedFolders: [selectedPath],
              showImageCounts:
                appSettings?.enableFolderImageCounts || appSettings?.folderTreeSort?.key === 'imageCount',
            });
            setLibrary({ folderTrees: [...folderTrees, newTree] });
          } catch (e) {
            toast.error(`Failed to load folder tree: ${e}`);
          } finally {
            setLibrary({ isTreeLoading: false });
          }
        }
        await handleSelectSubfolder(selectedPath, true);
      }
    } catch (err) {
      console.error(isAndroid ? 'Failed to open Android library root:' : 'Failed to open directory dialog:', err);
      toast.error(isAndroid ? 'Failed to open library.' : 'Failed to open folder selection dialog.');
    }
  }, [handleSelectSubfolder]);

  const handleBrowseCard = async () => {
    const { osPlatform, appSettings } = useSettingsStore.getState();
    if (osPlatform === 'android') return;

    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== 'string' || !selected) return;

      await invoke(Invokes.SetCardBrowseRoot, { path: selected });
      const newTree = await invoke<FolderTree>(Invokes.GetFolderTree, {
        path: selected,
        expandedFolders: [selected],
        showImageCounts: appSettings?.enableFolderImageCounts || appSettings?.folderTreeSort?.key === 'imageCount',
      });

      useLibraryStore.getState().setLibrary({
        rootPaths: [selected],
        cardBrowseRoot: selected,
        folderTrees: [newTree],
        expandedFolders: new Set([selected]),
      });
      await handleSelectSubfolder(selected, false);
      toast.success(i18n.t('library.cardMode.opened'));
    } catch (err) {
      await invoke(Invokes.SetCardBrowseRoot, { path: null }).catch(() => undefined);
      useLibraryStore.getState().setLibrary({ cardBrowseRoot: null });
      toast.error(i18n.t('library.cardMode.browseFailed', { error: String(err) }));
    }
  };

  const handleContinueSession = () => {
    const restore = async () => {
      const { appSettings } = useSettingsStore.getState();
      const { setLibrary } = useLibraryStore.getState();

      const rootFolders = appSettings?.rootFolders?.length
        ? appSettings.rootFolders
        : appSettings?.lastRootPath
          ? [appSettings.lastRootPath]
          : [];

      if (rootFolders.length === 0) return;

      await invoke(Invokes.SetCardBrowseRoot, { path: null });
      setLibrary({ cardBrowseRoot: null });

      const folderState = appSettings?.lastFolderState;
      const pathToSelect = folderState?.currentFolderPath || rootFolders[0];

      setLibrary({ rootPaths: rootFolders });

      if (folderState?.expandedFolders) {
        const newExpandedFolders = new Set<string>(folderState.expandedFolders);
        setLibrary({ expandedFolders: newExpandedFolders });
      } else {
        setLibrary({ expandedFolders: new Set(rootFolders) });
      }

      setLibrary({ isTreeLoading: true });
      try {
        let treesData: FolderTree[];
        const preloadedData = preloadedDataRef.current;
        if (preloadedData?.rootPaths?.join() === rootFolders.join() && preloadedData.trees) {
          treesData = await preloadedData.trees;
          preloadedData.trees = undefined;
        } else {
          const expandedArr = folderState?.expandedFolders
            ? Array.from(new Set(folderState.expandedFolders))
            : rootFolders;
          treesData = await invoke<FolderTree[]>(Invokes.GetPinnedFolderTrees, {
            paths: rootFolders,
            expandedFolders: expandedArr,
            showImageCounts: appSettings?.enableFolderImageCounts || appSettings?.folderTreeSort?.key === 'imageCount',
          });
        }
        setLibrary({ folderTrees: treesData });
      } catch (err) {
        console.error('Failed to restore folder trees:', err);
      } finally {
        setLibrary({ isTreeLoading: false });
      }

      let preloadedImages: ImageFile[] | undefined = undefined;
      const preloadedData = preloadedDataRef.current;
      if (preloadedData && preloadedData.currentPath === pathToSelect && preloadedData.images) {
        try {
          preloadedImages = await preloadedData.images;
          preloadedData.images = undefined;
        } catch (e) {
          console.error('Failed to retrieve preloaded images', e);
        }
      }

      if (pathToSelect && pathToSelect.startsWith('Album: ')) {
        const activeAlbumId = folderState?.activeAlbumId;
        if (activeAlbumId) {
          try {
            const albumTree = await invoke<AlbumItem[]>(Invokes.GetAlbums);
            setLibrary({ albumTree });

            const findObj = (nodes: AlbumItem[]): AlbumItem | null => {
              for (const n of nodes) {
                if (n.id === activeAlbumId) return n;
                if (n.type === 'group') {
                  const f = findObj(n.children);
                  if (f) return f;
                }
              }
              return null;
            };

            const album = findObj(albumTree);
            if (album?.type === 'album') {
              await handleSelectAlbum(album.id, album.name, album.images);
            } else {
              await handleSelectSubfolder(rootFolders[0], false, undefined, false);
            }
          } catch (e) {
            console.error('Failed to restore album session:', e);
            await handleSelectSubfolder(rootFolders[0], false, undefined, false);
          }
        } else {
          await handleSelectSubfolder(rootFolders[0], false, undefined, false);
        }
      } else {
        await handleSelectSubfolder(pathToSelect, false, preloadedImages, false);
      }
    };

    restore().catch((err) => {
      console.error('Failed to restore session:', err);
      toast.error('Failed to restore session. A folder may have been moved or deleted.');
      handleGoHome();
      useLibraryStore.getState().setLibrary({ isTreeLoading: false });
    });
  };

  return {
    handleGoHome,
    handleBackToLibrary,
    handleImageSelect,
    handleSelectSubfolder,
    handleSelectAlbum,
    handleOpenFolder,
    handleNavBack,
    handleNavForward,
    handleBrowseCard,
    handleContinueSession,
  };
}
