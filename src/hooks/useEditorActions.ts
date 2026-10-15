import { getDpr, transformFromPercent } from '../utils/zoom';
import { useCallback, type SyntheticEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import debounce from 'lodash.debounce';
import { toast } from 'react-toastify';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useProcessStore } from '../store/useProcessStore';
import {
  Adjustments,
  INITIAL_ADJUSTMENTS,
  COPYABLE_ADJUSTMENT_KEYS,
  PasteMode,
  LensAdjustment,
  normalizeLoadedAdjustments,
} from '../utils/adjustments';
import { calculateCenteredCrop } from '../utils/cropUtils';
import { Invokes } from '../components/ui/AppProperties';
import { globalImageCache } from '../utils/ImageLRUCache';
import { isPathInCardRoot } from '../utils/cardMode';
import i18n from 'i18next';

interface LoadedMetadata {
  adjustments: (Adjustments & { is_null?: boolean }) | null;
}

export const debouncedSetHistory = debounce((newAdj: Adjustments) => {
  useEditorStore.getState().pushHistory(newAdj);
}, 500);

let cardEditNoticeShownFor: string | null = null;
const pendingEditorSaves = new Set<Promise<unknown>>();
export interface XmpEditorDraft {
  path: string;
  adjustments: Adjustments;
}
let queuedEditorDraft: XmpEditorDraft | undefined;
let persistedEditorDraft: XmpEditorDraft | undefined;

export function markEditorDraftPersisted(path: string, adjustments: Adjustments): void {
  persistedEditorDraft = { path, adjustments };
}
let importSavePause: Map<string, XmpEditorDraft> | undefined;

const saveDebounce = debounce((path: string, adjustmentsToSave: Adjustments) => {
  queuedEditorDraft = undefined;
  const cardRoot = useLibraryStore.getState().cardBrowseRoot;
  if (isPathInCardRoot(path, cardRoot)) {
    if (cardEditNoticeShownFor !== cardRoot) {
      cardEditNoticeShownFor = cardRoot;
      toast.info(i18n.t('library.cardMode.editNotSaved'));
    }
    return;
  }
  const save = invoke(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: adjustmentsToSave });
  pendingEditorSaves.add(save);
  void save.then(
    () => pendingEditorSaves.delete(save),
    (err) => {
      pendingEditorSaves.delete(save);
      console.error('Auto-save failed:', err);
      toast.error(`Failed to save changes: ${err}`);
    },
  );
  return save;
}, 300);

export const debouncedSave = Object.assign(
  (path: string, adjustments: Adjustments) => {
    const draft = { path, adjustments };
    if (persistedEditorDraft?.path === path) {
      if (persistedEditorDraft.adjustments === adjustments) return;
      persistedEditorDraft = undefined;
    }
    if (importSavePause) {
      importSavePause.set(path, draft);
      return;
    }
    queuedEditorDraft = draft;
    return saveDebounce(path, adjustments);
  },
  {
    cancel: () => {
      queuedEditorDraft = undefined;
      saveDebounce.cancel();
    },
    flush: () => saveDebounce.flush(),
  },
);

export async function waitForPendingEditorSaves(): Promise<void> {
  let failure: PromiseRejectedResult | undefined;
  while (pendingEditorSaves.size) {
    const results = await Promise.allSettled([...pendingEditorSaves]);
    for (const result of results) {
      if (result.status === 'rejected') failure ??= result;
    }
  }
  if (failure) throw failure.reason;
}

function captureEditorDraft(): XmpEditorDraft | undefined {
  const state = useEditorStore.getState();
  return state.selectedImage ? { path: state.selectedImage.path, adjustments: state.adjustments } : undefined;
}

export interface XmpImportSavePause {
  draft?: XmpEditorDraft;
  hasNewerDraft(path: string): boolean;
  finish(consumedPaths: string[]): void;
}

export async function prepareEditorDraftForXmpImport(targetPath?: string): Promise<XmpImportSavePause> {
  if (importSavePause) throw new Error('An XMP import is already in progress');
  const buffered = new Map<string, XmpEditorDraft>();
  const captured = new Map<string, XmpEditorDraft>();
  // Active snapshots supply import data; only pending edits need resuming.
  const queuedBeforePause = queuedEditorDraft;
  if (queuedBeforePause) captured.set(queuedBeforePause.path, queuedBeforePause);
  const before = captureEditorDraft();
  if (before) captured.set(before.path, before);
  // Pause new requests synchronously, before waiting for existing writes. Never
  // flush a queued autosave into the incoming XMP packet.
  importSavePause = buffered;
  debouncedSave.cancel();
  let finished = false;
  const finish = (consumedPaths: string[]) => {
    if (finished) return;
    finished = true;
    const current = captureEditorDraft();
    const writes = new Map<string, XmpEditorDraft>();
    if (queuedBeforePause && !consumedPaths.includes(queuedBeforePause.path)) {
      writes.set(queuedBeforePause.path, current?.path === queuedBeforePause.path ? current : queuedBeforePause);
    }
    for (const [path, draft] of buffered) {
      if (consumedPaths.includes(path) && draft.adjustments === captured.get(path)?.adjustments) continue;
      writes.set(path, current?.path === path ? current : draft);
    }
    importSavePause = undefined;
    const entries = [...writes.values()];
    entries.forEach((draft, index) => {
      debouncedSave(draft.path, draft.adjustments);
      // The ordinary debounce has one slot; retain buffered writes to other photos.
      if (index < entries.length - 1) debouncedSave.flush();
    });
  };
  try {
    await waitForPendingEditorSaves();
    debouncedSetHistory.flush();
    for (const [path, bufferedDraft] of buffered) captured.set(path, bufferedDraft);
    const active = captureEditorDraft();
    if (active) captured.set(active.path, active);
    const draft = targetPath ? captured.get(targetPath) : active;
    const hasNewerDraft = (path: string) => {
      const active = captureEditorDraft();
      const snapshot = captured.get(path);
      if (active?.path === path && snapshot && active.adjustments !== snapshot.adjustments) return true;
      return buffered.has(path) && buffered.get(path)?.adjustments !== snapshot?.adjustments;
    };
    return { draft, hasNewerDraft, finish };
  } catch (error) {
    finish([]);
    throw error;
  }
}

export function useEditorActions() {
  const setEditor = useEditorStore((s) => s.setEditor);

  const setAdjustments = useCallback(
    (value: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)) => {
      setEditor((state) => {
        const prev = state.adjustments;
        const newAdjustments = typeof value === 'function' ? value(prev) : { ...prev, ...value };
        if (newAdjustments === prev) return state;
        if (importSavePause && state.selectedImage) {
          importSavePause.set(state.selectedImage.path, {
            path: state.selectedImage.path,
            adjustments: newAdjustments,
          });
        }
        debouncedSetHistory(newAdjustments);
        return {
          adjustments: newAdjustments,
          ...(state.showOriginal ? { showOriginal: false, previewOverride: null } : {}),
        };
      });
    },
    [setEditor],
  );

  const handleRotate = useCallback(
    (degrees: number) => {
      const { selectedImage, adjustments } = useEditorStore.getState();
      const increment = degrees > 0 ? 1 : 3;
      const newAspectRatio =
        adjustments.aspectRatio && adjustments.aspectRatio !== 0 ? 1 / adjustments.aspectRatio : null;
      const newOrientationSteps = ((adjustments.orientationSteps || 0) + increment) % 4;
      const newCrop =
        selectedImage?.width && selectedImage?.height
          ? calculateCenteredCrop(selectedImage.width, selectedImage.height, newOrientationSteps, newAspectRatio)
          : null;

      setAdjustments((prev) => ({
        ...prev,
        aspectRatio: newAspectRatio,
        orientationSteps: newOrientationSteps,
        rotation: 0,
        crop: newCrop,
      }));
    },
    [setAdjustments],
  );

  const handleAutoAdjustments = useCallback(async () => {
    const selectedImage = useEditorStore.getState().selectedImage;
    if (!selectedImage?.isReady) return;
    try {
      const autoAdjustments: Adjustments = await invoke(Invokes.CalculateAutoAdjustments);
      setAdjustments((prev: Adjustments) => ({
        ...prev,
        ...autoAdjustments,
        sectionVisibility: { ...prev.sectionVisibility, ...autoAdjustments.sectionVisibility },
      }));
    } catch (err) {
      toast.error(`Failed to apply auto adjustments: ${err}`);
    }
  }, [setAdjustments]);

  const toggleShowOriginal = useCallback(() => {
    setEditor((state) => {
      const isShowing = !state.showOriginal;

      if (isShowing) {
        const override: Adjustments = { ...INITIAL_ADJUSTMENTS };
        const geometryKeys: Array<keyof Adjustments> = [
          'crop',
          'rotation',
          'flipHorizontal',
          'flipVertical',
          'orientationSteps',
          'aspectRatio',
          'transformDistortion',
          'transformVertical',
          'transformHorizontal',
          'transformRotate',
          'transformAspect',
          'transformScale',
          'transformXOffset',
          'transformYOffset',
          'lensDistortionAmount',
          'lensVignetteAmount',
          'lensTcaAmount',
          'lensDistortionParams',
          'lensMaker',
          'lensModel',
          'lensDistortionEnabled',
          'lensTcaEnabled',
          'lensVignetteEnabled',
        ];

        geometryKeys.forEach((key) => {
          override[key] = state.adjustments[key];
        });

        return { showOriginal: true, previewOverride: override };
      } else {
        return { showOriginal: false, previewOverride: null };
      }
    });
  }, [setEditor]);

  const handleLutSelect = useCallback(
    async (path: string, isBuiltIn: boolean = false) => {
      const isAndroid = useSettingsStore.getState().osPlatform === 'android';
      try {
        const result: { size: number } = await invoke('load_and_parse_lut', { path });
        const name =
          isAndroid && path.startsWith('content://')
            ? await invoke<string>('resolve_android_content_uri_name', { uriStr: path })
            : path.split(/[\\/]/).pop() || 'LUT';
        setAdjustments((prev: Adjustments) => ({
          ...prev,
          lutPath: path,
          lutName: name,
          lutSize: result.size,
          lutIntensity: 100,
          lutIsSceneReferred: isBuiltIn,
          sectionVisibility: {
            ...(prev.sectionVisibility || INITIAL_ADJUSTMENTS.sectionVisibility),
            effects: true,
          },
        }));
      } catch (err) {
        toast.error(`Failed to load LUT: ${err}`);
      }
    },
    [setAdjustments],
  );

  const setLutPreviewOverride = useCallback(
    (path: string | null, isBuiltIn: boolean = false) => {
      setEditor((state) => {
        if (!path) return { previewOverride: null };
        const name = path.split(/[\\/]/).pop() || 'LUT';
        return {
          previewOverride: {
            ...state.adjustments,
            lutPath: path,
            lutName: name,
            lutIntensity: state.adjustments.lutIntensity,
            lutIsSceneReferred: isBuiltIn,
          },
        };
      });
    },
    [setEditor],
  );

  const handleResetAdjustments = useCallback(
    (paths?: string[]) => {
      const { multiSelectedPaths, libraryActivePath, setLibrary } = useLibraryStore.getState();
      const { selectedImage, resetHistory } = useEditorStore.getState();
      const pathsToReset = paths || multiSelectedPaths;
      if (pathsToReset.length === 0) return;

      pathsToReset.forEach((p) => globalImageCache.delete(p));
      debouncedSetHistory.cancel();

      invoke(Invokes.ResetAdjustmentsForPaths, { paths: pathsToReset })
        .then(() => {
          if (libraryActivePath && pathsToReset.includes(libraryActivePath))
            setLibrary({ libraryActiveAdjustments: { ...INITIAL_ADJUSTMENTS } });
          if (selectedImage && pathsToReset.includes(selectedImage.path)) {
            const aspect =
              selectedImage.width && selectedImage.height ? selectedImage.width / selectedImage.height : null;
            const resetData = { ...INITIAL_ADJUSTMENTS, aspectRatio: aspect, aiPatches: [] };
            resetHistory(resetData);
            setEditor({ adjustments: resetData });
          }
        })
        .catch((err) => toast.error(`Failed to reset adjustments: ${err}`));
    },
    [setEditor],
  );

  const handleAutoLensCorrection = useCallback(
    (paths?: string[]) => {
      const { multiSelectedPaths, libraryActivePath, setLibrary } = useLibraryStore.getState();
      const { selectedImage, resetHistory } = useEditorStore.getState();

      const pathsToUpdate =
        paths && paths.length > 0
          ? paths
          : multiSelectedPaths.length > 0
            ? multiSelectedPaths
            : selectedImage
              ? [selectedImage.path]
              : [];

      if (pathsToUpdate.length === 0) return;

      pathsToUpdate.forEach((p) => globalImageCache.delete(p));

      invoke('apply_auto_lens_correction_to_paths', { paths: pathsToUpdate })
        .then(async () => {
          if (selectedImage && pathsToUpdate.includes(selectedImage.path)) {
            const meta = await invoke<LoadedMetadata>(Invokes.LoadMetadata, { path: selectedImage.path });
            if (meta.adjustments && !meta.adjustments.is_null) {
              const normalized = normalizeLoadedAdjustments(meta.adjustments);
              setEditor({ adjustments: normalized });
              resetHistory(normalized);
            }
          }
          if (libraryActivePath && pathsToUpdate.includes(libraryActivePath)) {
            const meta = await invoke<LoadedMetadata>(Invokes.LoadMetadata, { path: libraryActivePath });
            if (meta.adjustments && !meta.adjustments.is_null) {
              setLibrary({ libraryActiveAdjustments: normalizeLoadedAdjustments(meta.adjustments) });
            }
          }
        })
        .catch((err) => toast.error(`Failed to apply auto lens correction: ${err}`));
    },
    [setEditor],
  );

  const handleCopyAdjustments = useCallback(async (pathOrEvent?: string | SyntheticEvent) => {
    const pathOverride = typeof pathOrEvent === 'string' ? pathOrEvent : undefined;
    const { selectedImage, adjustments } = useEditorStore.getState();
    const { libraryActivePath, multiSelectedPaths } = useLibraryStore.getState();
    let sourceAdjustments: Adjustments | null = null;

    const pathToCopyFrom =
      pathOverride || (selectedImage ? selectedImage.path : libraryActivePath || multiSelectedPaths[0]);

    if (selectedImage && pathToCopyFrom === selectedImage.path) {
      sourceAdjustments = adjustments;
    } else if (pathToCopyFrom) {
      try {
        const meta = await invoke<LoadedMetadata>(Invokes.LoadMetadata, { path: pathToCopyFrom });
        if (meta?.adjustments && !meta.adjustments.is_null) {
          sourceAdjustments = normalizeLoadedAdjustments(meta.adjustments);
        } else {
          sourceAdjustments = INITIAL_ADJUSTMENTS;
        }
      } catch (err) {
        toast.error(`Failed to load metadata for copying: ${err}`);
        return;
      }
    }

    if (!sourceAdjustments) return;

    const adjustmentsToCopy: Partial<Adjustments> = {};

    for (const key of COPYABLE_ADJUSTMENT_KEYS) {
      if (Object.prototype.hasOwnProperty.call(sourceAdjustments, key)) {
        adjustmentsToCopy[key] = structuredClone(sourceAdjustments[key]);
      }
    }
    useEditorStore.getState().setEditor({ copiedAdjustments: adjustmentsToCopy });
    useProcessStore.getState().setProcess({ isCopied: true });
  }, []);

  const handlePasteAdjustments = useCallback(
    (paths?: string[]) => {
      const { copiedAdjustments, selectedImage, adjustments } = useEditorStore.getState();
      const { multiSelectedPaths } = useLibraryStore.getState();
      const { appSettings } = useSettingsStore.getState();
      const { setProcess } = useProcessStore.getState();

      if (!copiedAdjustments || !appSettings) return;

      const mode = appSettings.copyPasteSettings?.mode ?? PasteMode.Merge;
      const includedAdjustments = appSettings.copyPasteSettings?.includedAdjustments ?? COPYABLE_ADJUSTMENT_KEYS;
      const adjustmentsToApply: Partial<Adjustments> = {};

      for (const key of includedAdjustments) {
        if (Object.prototype.hasOwnProperty.call(copiedAdjustments, key)) {
          const value = copiedAdjustments[key as keyof Adjustments];
          if (mode === PasteMode.Merge) {
            const defaultValue = INITIAL_ADJUSTMENTS[key as keyof Adjustments];
            if (JSON.stringify(value) !== JSON.stringify(defaultValue))
              adjustmentsToApply[key as keyof Adjustments] = value;
          } else {
            adjustmentsToApply[key as keyof Adjustments] = value;
          }
        }
      }

      if (includedAdjustments.includes(LensAdjustment.LensMaker)) {
        if (!adjustmentsToApply.lensMaker) {
          adjustmentsToApply.lensDistortionParams = null;
        }
      }

      if (Object.keys(adjustmentsToApply).length === 0) {
        setProcess({ isPasted: true });
        return;
      }

      const pathsToUpdate =
        paths || (multiSelectedPaths.length > 0 ? multiSelectedPaths : selectedImage ? [selectedImage.path] : []);
      if (pathsToUpdate.length === 0) return;

      pathsToUpdate.forEach((p) => globalImageCache.delete(p));

      if (selectedImage && pathsToUpdate.includes(selectedImage.path)) {
        setAdjustments({ ...adjustments, ...adjustmentsToApply });
      }

      invoke(Invokes.ApplyAdjustmentsToPaths, { paths: pathsToUpdate, adjustments: adjustmentsToApply })
        .then(() => {
          if (selectedImage && pathsToUpdate.includes(selectedImage.path)) {
            invoke<LoadedMetadata>(Invokes.LoadMetadata, { path: selectedImage.path }).then((meta) => {
              const loadedAdjustments = meta.adjustments;
              if (loadedAdjustments && !loadedAdjustments.is_null) {
                setAdjustments((prev) => ({
                  ...prev,
                  lensMaker: loadedAdjustments.lensMaker,
                  lensModel: loadedAdjustments.lensModel,
                  lensDistortionParams: loadedAdjustments.lensDistortionParams,
                }));
              }
            });
          }
        })
        .catch((err) => toast.error(`Failed to paste adjustments: ${err}`));

      setProcess({ isPasted: true });
    },
    [setAdjustments],
  );

  const handleZoomChange = useCallback((zoomValue: number, fitToWindow: boolean = false) => {
    const { selectedImage, baseRenderSize, setEditor } = useEditorStore.getState();
    if (!selectedImage) return;
    const zoom =
      !fitToWindow && baseRenderSize.renderScale > 0
        ? transformFromPercent(zoomValue, baseRenderSize.renderScale, getDpr())
        : 1;
    setEditor({
      zoom,
      zoomRequest: { imagePath: selectedImage.path, percent: fitToWindow ? null : zoomValue },
    });
  }, []);

  return {
    setAdjustments,
    handleRotate,
    handleAutoAdjustments,
    handleLutSelect,
    setLutPreviewOverride,
    handleResetAdjustments,
    handleAutoLensCorrection,
    handleCopyAdjustments,
    handlePasteAdjustments,
    handleZoomChange,
    toggleShowOriginal,
  };
}
