import { rememberAppliedStyleStack } from '../utils/appliedPreset';
import { createResetAdjustmentsWithLensDefaults } from '../utils/rawLensDefaults';
import { captureAdjustments } from '../utils/adjustmentCapture';
import { normalizeLensPolicyVersion } from '../utils/lensPolicyVersion';
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
  getActiveTools,
  mergeActiveTools,
  normalizeLoadedAdjustments,
  setToolActive,
} from '../utils/adjustments';
import { calculateCenteredCrop, getOrientedDimensions } from '../utils/cropUtils';
import { Invokes } from '../components/ui/AppProperties';
import { globalImageCache } from '../utils/ImageLRUCache';
import { isPathInCardRoot } from '../utils/cardMode';
import i18n from 'i18next';
import {
  queuePhotoWrite,
  pausePhotoWrites,
  waitForPhotoWrites,
  pendingPhotoWriteDrafts,
} from '../utils/photoWriteQueue';

interface LoadedMetadata {
  adjustments: (Adjustments & { is_null?: boolean }) | null;
}

let copyRequest = 0;

export const debouncedSetHistory = debounce((newAdj: Adjustments, label?: string) => {
  if (label) useEditorStore.getState().pushHistory(newAdj, label);
  else useEditorStore.getState().pushHistory(newAdj);
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
  const save = queuePhotoWrite(path, () =>
    invoke(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: adjustmentsToSave }),
  );
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
  for (const draft of pendingPhotoWriteDrafts()) captured.set(draft.path, draft);
  const before = captureEditorDraft();
  if (before) captured.set(before.path, before);
  // Pause new requests synchronously, before waiting for existing writes. Never
  // flush a queued autosave into the incoming XMP packet.
  importSavePause = buffered;
  const resumePhotoWrites = pausePhotoWrites();
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
    resumePhotoWrites(
      (path, snapshot) =>
        !consumedPaths.includes(path) ||
        !snapshot ||
        (captured.has(path)
          ? snapshot !== captured.get(path)!.adjustments
          : buffered.get(path)?.adjustments === snapshot),
    );
    const entries = [...writes.values()];
    entries.forEach((draft, index) => {
      debouncedSave(draft.path, draft.adjustments);
      // The ordinary debounce has one slot; retain buffered writes to other photos.
      if (index < entries.length - 1) debouncedSave.flush();
    });
  };
  try {
    await waitForPendingEditorSaves();
    await waitForPhotoWrites();
    debouncedSetHistory.flush();
    for (const draft of pendingPhotoWriteDrafts()) captured.set(draft.path, draft);
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
    (value: Partial<Adjustments> | ((prev: Adjustments) => Adjustments), label?: string) => {
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
        debouncedSetHistory(newAdjustments, label);
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
    const { selectedImage, adjustments: before } = useEditorStore.getState();
    if (!selectedImage?.isReady) return;
    debouncedSetHistory.flush();
    try {
      const autoAdjustments: Adjustments = await invoke(Invokes.CalculateAutoAdjustments, {
        jsAdjustments: before,
      });
      const current = useEditorStore.getState();
      if (current.selectedImage !== selectedImage || !current.selectedImage.isReady || current.adjustments !== before)
        return;
      setAdjustments(
        (prev: Adjustments) => ({
          ...prev,
          ...autoAdjustments,
          activeTools: mergeActiveTools(prev, autoAdjustments),
        }),
        'Auto adjustments',
      );
    } catch (err) {
      toast.error(`Failed to apply auto adjustments: ${err}`);
    }
  }, [setAdjustments]);

  const toggleShowOriginal = useCallback(() => {
    setEditor((state) => {
      const isShowing = !state.showOriginal;

      if (isShowing) {
        const override: Adjustments = { ...INITIAL_ADJUSTMENTS };
        const preservedKeys: Array<keyof Adjustments> = [
          'baselineExposure',
          'cameraExposureCompensation',
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

        preservedKeys.forEach((key) => {
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
          activeTools: setToolActive(getActiveTools(prev), 'lut', true),
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
      const { selectedImage } = useEditorStore.getState();
      const pathsToReset = paths || multiSelectedPaths;
      if (!pathsToReset.length) return;
      pathsToReset.forEach((path) => globalImageCache.delete(path));
      debouncedSetHistory.flush();
      const before = useEditorStore.getState().adjustments;
      const active = selectedImage?.isReady && pathsToReset.includes(selectedImage.path) ? selectedImage : null;
      const otherPaths = pathsToReset.filter((path) => path !== active?.path);
      const operations: Promise<void>[] = [];
      if (active) {
        operations.push(
          createResetAdjustmentsWithLensDefaults(active).then((reset) => {
            const current = useEditorStore.getState();
            if (current.selectedImage !== active || !current.selectedImage.isReady || current.adjustments !== before)
              return;
            void rememberAppliedStyleStack(active.path, null, reset);
            setAdjustments(() => reset, 'Reset adjustments');
            debouncedSetHistory.flush();
            if (useLibraryStore.getState().libraryActivePath === active.path)
              setLibrary({ libraryActiveAdjustments: reset });
          }),
        );
      }
      if (otherPaths.length) {
        operations.push(
          invoke(Invokes.ResetAdjustmentsForPaths, { paths: otherPaths }).then(async () => {
            if (libraryActivePath && otherPaths.includes(libraryActivePath)) {
              const reset = await createResetAdjustmentsWithLensDefaults({
                path: libraryActivePath,
                width: 0,
                height: 0,
              });
              if (useLibraryStore.getState().libraryActivePath === libraryActivePath)
                setLibrary({ libraryActiveAdjustments: reset });
            }
          }),
        );
      }
      void Promise.all(operations).catch((error) => toast.error(`Failed to reset adjustments: ${error}`));
    },
    [setAdjustments],
  );

  const handleAutoLensCorrection = useCallback(
    (paths?: string[]) => {
      const { multiSelectedPaths, libraryActivePath, setLibrary } = useLibraryStore.getState();
      const { selectedImage } = useEditorStore.getState();
      const pathsToUpdate = paths?.length
        ? paths
        : multiSelectedPaths.length
          ? multiSelectedPaths
          : selectedImage
            ? [selectedImage.path]
            : [];
      if (!pathsToUpdate.length) return;
      pathsToUpdate.forEach((path) => globalImageCache.delete(path));
      debouncedSetHistory.flush();
      const before = useEditorStore.getState().adjustments;
      const active = selectedImage?.isReady && pathsToUpdate.includes(selectedImage.path) ? selectedImage : null;
      const otherPaths = pathsToUpdate.filter((path) => path !== active?.path);
      const operations: Promise<void>[] = [];
      if (active) {
        // The bulk writer reads saved recipes. Resolve the editor's lens patch
        // without writing that path, so neither a draft nor a later autosave
        // can be replaced by an older on-disk recipe.
        operations.push(
          invoke<Partial<Adjustments>>(Invokes.GetDefaultRawLensAdjustments, {
            path: active.path,
            forceAuto: true,
          }).then((lens) => {
            const current = useEditorStore.getState();
            if (current.selectedImage !== active || !current.selectedImage.isReady || current.adjustments !== before)
              return;
            const next: Adjustments = {
              ...before,
              lensCorrectionMode: 'auto',
              cameraLensCorrectionVersion: 1,
              lensDistortionEnabled: true,
              lensTcaEnabled: true,
              lensVignetteEnabled: true,
              lensMaker: lens.lensMaker ?? null,
              lensModel: lens.lensModel ?? null,
              lensDistortionParams: lens.lensDistortionParams ?? null,
            };
            setAdjustments(next, 'Auto lens correction');
            debouncedSetHistory.flush();
            if (useLibraryStore.getState().libraryActivePath === active.path)
              setLibrary({ libraryActiveAdjustments: next });
          }),
        );
      }
      if (otherPaths.length) {
        operations.push(
          invoke('apply_auto_lens_correction_to_paths', { paths: otherPaths }).then(async () => {
            if (libraryActivePath && otherPaths.includes(libraryActivePath)) {
              const meta = await invoke<LoadedMetadata>(Invokes.LoadMetadata, { path: libraryActivePath });
              if (
                useLibraryStore.getState().libraryActivePath === libraryActivePath &&
                meta.adjustments &&
                !meta.adjustments.is_null
              )
                setLibrary({ libraryActiveAdjustments: normalizeLoadedAdjustments(meta.adjustments) });
            }
          }),
        );
      }
      void Promise.all(operations).catch((error) => toast.error(`Failed to apply auto lens correction: ${error}`));
    },
    [setAdjustments],
  );

  const handleCopyAdjustments = useCallback(async (pathOrEvent?: string | SyntheticEvent) => {
    const request = ++copyRequest;
    const pathOverride = typeof pathOrEvent === 'string' ? pathOrEvent : undefined;
    const { selectedImage, adjustments } = useEditorStore.getState();
    const library = useLibraryStore.getState();
    const { libraryActivePath, multiSelectedPaths } = library;
    const settings = useSettingsStore.getState().appSettings;
    let sourceAdjustments: Adjustments | null = null;

    const pathToCopyFrom =
      pathOverride || (selectedImage ? selectedImage.path : libraryActivePath || multiSelectedPaths[0]);
    if (!pathToCopyFrom) return;
    const editorSource = selectedImage?.path === pathToCopyFrom;
    const editorSnapshot = editorSource ? JSON.stringify(adjustments) : null;
    const isCurrent = () => {
      const current = useEditorStore.getState();
      const activeLibrary = useLibraryStore.getState();
      return (
        request === copyRequest &&
        current.selectedImage === selectedImage &&
        useSettingsStore.getState().appSettings === settings &&
        (editorSource
          ? current.adjustments === adjustments && JSON.stringify(current.adjustments) === editorSnapshot
          : activeLibrary.libraryActivePath === libraryActivePath &&
            activeLibrary.multiSelectedPaths === multiSelectedPaths &&
            activeLibrary.currentFolderPath === library.currentFolderPath &&
            activeLibrary.libraryActiveAdjustments === library.libraryActiveAdjustments)
      );
    };

    if (selectedImage && pathToCopyFrom === selectedImage.path) {
      sourceAdjustments = structuredClone(adjustments);
    } else if (pathToCopyFrom) {
      try {
        const meta = await invoke<LoadedMetadata>(Invokes.LoadMetadata, { path: pathToCopyFrom });
        if (meta?.adjustments && !meta.adjustments.is_null) {
          sourceAdjustments = normalizeLoadedAdjustments(meta.adjustments);
        }
      } catch (err) {
        if (isCurrent()) toast.error(`Failed to load metadata for copying: ${err}`);
        return;
      }
    }

    if (!isCurrent()) return;
    const source = sourceAdjustments ? structuredClone(sourceAdjustments) : null;
    const file = library.imageList.find((item) => item.path === pathToCopyFrom);
    let defaults: Adjustments;
    try {
      defaults = await createResetAdjustmentsWithLensDefaults(
        editorSource
          ? selectedImage
          : {
              path: pathToCopyFrom,
              width: 0,
              height: 0,
              isRaw: file?.is_raw,
            },
      );
    } catch (err) {
      if (isCurrent()) toast.error(`Failed to load metadata for copying: ${err}`);
      return;
    }
    if (!isCurrent()) return;
    // Unopened photos with no saved recipe use the same resolved fresh defaults
    // as their comparison baseline; saved objects keep their own interpretation.
    const adjustmentsToCopy = captureAdjustments(source ?? defaults, COPYABLE_ADJUSTMENT_KEYS);
    useEditorStore.getState().setEditor({
      copiedAdjustments: adjustmentsToCopy,
      copiedAdjustmentDefaults: { adjustments: adjustmentsToCopy, defaults: structuredClone(defaults) },
    });
    useProcessStore.getState().setProcess({ isCopied: true });
  }, []);

  const handlePasteAdjustments = useCallback(
    (paths?: string[]) => {
      const { copiedAdjustments, copiedAdjustmentDefaults, selectedImage, adjustments } = useEditorStore.getState();
      const { multiSelectedPaths } = useLibraryStore.getState();
      const { appSettings } = useSettingsStore.getState();
      const { setProcess } = useProcessStore.getState();

      if (!copiedAdjustments || !appSettings) return;

      const mode = appSettings.copyPasteSettings?.mode ?? PasteMode.Merge;
      const includedAdjustments = appSettings.copyPasteSettings?.includedAdjustments ?? COPYABLE_ADJUSTMENT_KEYS;
      // Old/programmatically supplied clipboards have no source provenance;
      // retain their previous baseline instead of borrowing the target photo's.
      const defaults =
        copiedAdjustmentDefaults?.adjustments === copiedAdjustments
          ? copiedAdjustmentDefaults.defaults
          : INITIAL_ADJUSTMENTS;
      const adjustmentsToApply = captureAdjustments(
        copiedAdjustments,
        includedAdjustments,
        mode === PasteMode.Merge ? defaults : undefined,
      );

      if (adjustmentsToApply.exposure !== undefined) {
        adjustmentsToApply.baselineExposure = copiedAdjustments.baselineExposure ?? false;
        adjustmentsToApply.cameraExposureCompensation = copiedAdjustments.cameraExposureCompensation ?? false;
      }

      if (Object.prototype.hasOwnProperty.call(adjustmentsToApply, LensAdjustment.LensMaker)) {
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
        setAdjustments(
          normalizeLensPolicyVersion(
            { ...adjustments, ...adjustmentsToApply },
            adjustmentsToApply,
            adjustments,
          ) as Adjustments,
          'Paste adjustments',
        );
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

  const handleCropOrientationToggle = useCallback(() => {
    const { selectedImage } = useEditorStore.getState();
    if (!selectedImage?.width || !selectedImage.height) return;
    setAdjustments((prev) => {
      if (!prev.aspectRatio || !Number.isFinite(prev.aspectRatio) || prev.aspectRatio <= 0 || prev.aspectRatio === 1) {
        return prev;
      }
      const aspectRatio = 1 / prev.aspectRatio;
      const steps = prev.orientationSteps || 0;
      const { width, height } = getOrientedDimensions(selectedImage.width, selectedImage.height, steps);
      const crop = prev.crop;
      const center = crop
        ? {
            x: (crop.x + crop.width / 2) * (crop.unit === '%' ? width / 100 : 1),
            y: (crop.y + crop.height / 2) * (crop.unit === '%' ? height / 100 : 1),
          }
        : undefined;
      return {
        ...prev,
        aspectRatio,
        crop: calculateCenteredCrop(
          selectedImage.width,
          selectedImage.height,
          steps,
          aspectRatio,
          prev.rotation || 0,
          center,
        ),
      };
    });
  }, [setAdjustments]);

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
    handleCropOrientationToggle,
    toggleShowOriginal,
  };
}
