import { showInstagramExportComplete } from '../utils/instagramExportToast';
import { showTransferSummary, type TransferSummary } from '../utils/fileTransfers';
import { useEffect, useRef } from 'react';
import { listen as tauriListen, type Event, type UnlistenFn } from '@tauri-apps/api/event';
import { convertFileSrc } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Status } from '../components/ui/ExportImportProperties';
import type {
  CullingSuggestions,
  ImageFile,
  ImageFlag,
  StackMember,
  HistogramData,
  Progress,
  WaveformData,
} from '../components/ui/AppProperties';
import { useProcessStore } from '../store/useProcessStore';
import { useEditorStore } from '../store/useEditorStore';
import {
  isActiveDenoiseEvent,
  useUIStore,
  type DenoiseCompleteEvent,
  type DenoiseErrorEvent,
  type DenoiseProgressEvent,
} from '../store/useUIStore';
import { useLibraryStore } from '../store/useLibraryStore';
import type { ExternalEditSession } from '../store/useProcessStore';

interface TauriEventPayloads {
  'ai-denoise-error': { path: string; error: string };
  'ai-denoise-progress': { path: string; progress: number };
  'ai-denoise-ready': string;
  'ai-model-download-finish': string;
  'ai-model-download-start': string;
  'analytics-update': {
    histogram: HistogramData | null;
    path: string;
    waveform: WaveformData | null;
  };
  'batch-export-progress': Progress & { path?: string };
  'culling-complete': CullingSuggestions;
  'culling-error': string;
  'culling-progress': { current: number; stage: string; total: number };
  'culling-start': number;
  'denoise-complete': DenoiseCompleteEvent;
  'denoise-error': DenoiseErrorEvent;
  'denoise-progress': DenoiseProgressEvent;
  'external-edit-session': ExternalEditSession;
  'export-cancelled': null;
  'export-cancelling': null;
  'export-complete': { instagramOutputFolder?: string | null } | null;
  'export-error': string;
  'focus-stack-complete': { base64: string; depthMap: string };
  'focus-stack-error': string;
  'focus-stack-progress': string;
  'hdr-complete': { base64: string };
  'hdr-error': string;
  'hdr-progress': string;
  'image-metadata-loaded': {
    flag: ImageFlag | null;
    is_edited: boolean;
    path: string;
    rating: number;
    tags: string[] | null;
    stack?: StackMember | null;
  };
  'import-complete': TransferSummary;
  'import-error': string;
  'import-progress': { current: number; path: string; total: number };
  'import-start': { total: number };
  'indexing-finished': null;
  'indexing-progress': Progress;
  'indexing-started': null;
  'open-with-file': string;
  'panorama-complete': { base64: string };
  'panorama-error': string;
  'panorama-progress': string;
  'preview-update-uncropped': string;
  'thumbnail-generated': {
    data?: string;
    is_edited?: boolean;
    path: string;
    previewPath?: string;
    rating?: number;
    thumbnailPath?: string;
  };
  'thumbnail-generation-complete': boolean;
  'thumbnail-progress': Progress;
  'wgpu-frame-ready': { path: string };
}

function listen<T extends keyof TauriEventPayloads>(
  eventName: T,
  handler: (event: Event<TauriEventPayloads[T]>) => void,
): Promise<UnlistenFn> {
  return tauriListen<TauriEventPayloads[T]>(eventName, handler);
}

interface TauriListenerProps {
  refreshAllFolderTrees: () => void;
  handleSelectSubfolder: (
    path: string,
    isNewRoot?: boolean,
    preloadedImages?: ImageFile[],
    expandParents?: boolean,
  ) => void;
  refreshImageList: () => void;
  markGenerated: (path: string) => void;
}

export function useTauriListeners({
  refreshAllFolderTrees,
  handleSelectSubfolder,
  refreshImageList,
  markGenerated,
}: TauriListenerProps) {
  const refs = useRef({ refreshAllFolderTrees, handleSelectSubfolder, refreshImageList, markGenerated });

  useEffect(() => {
    refs.current = { refreshAllFolderTrees, handleSelectSubfolder, refreshImageList, markGenerated };
  });

  const thumbnailBuffer = useRef<Record<string, string>>({});
  const mediumThumbnailBuffer = useRef<Record<string, string>>({});
  const ratingBuffer = useRef<Record<string, number>>({});
  const editStatusBuffer = useRef<Record<string, boolean>>({});
  const metadataBuffer = useRef<Record<string, { flag: any; tags: any; stack: any }>>({});
  const progressBuffer = useRef<{ current: number; total: number } | null>(null);
  const flushHandle = useRef<number | null>(null);

  useEffect(() => {
    let isEffectActive = true;

    const refreshAfterExport = () => {
      refs.current.refreshAllFolderTrees();
      if (useLibraryStore.getState().currentFolderPath) {
        refs.current.refreshImageList();
      }
    };

    const flushThumbnailBatch = () => {
      flushHandle.current = null;
      if (!isEffectActive) return;

      const pendingThumbs = thumbnailBuffer.current;
      const pendingMediumThumbs = mediumThumbnailBuffer.current;
      const pendingRatings = ratingBuffer.current;
      const pendingEdits = editStatusBuffer.current;
      const pendingMetadata = metadataBuffer.current;
      const pendingProgress = progressBuffer.current;

      thumbnailBuffer.current = {};
      mediumThumbnailBuffer.current = {};
      ratingBuffer.current = {};
      editStatusBuffer.current = {};
      metadataBuffer.current = {};
      progressBuffer.current = null;

      const hasThumbs = Object.keys(pendingThumbs).length > 0;
      if (hasThumbs || pendingProgress) {
        useProcessStore.getState().setProcess((state) => {
          const update: Partial<typeof state> = {};
          if (hasThumbs) {
            update.thumbnails = { ...state.thumbnails, ...pendingThumbs };
            update.mediumThumbnails = { ...state.mediumThumbnails, ...pendingMediumThumbs };
          }
          if (
            pendingProgress &&
            (state.thumbnailProgress?.current !== pendingProgress.current ||
              state.thumbnailProgress?.total !== pendingProgress.total)
          ) {
            update.thumbnailProgress = pendingProgress;
          }
          return update;
        });
      }

      const ratingPaths = Object.keys(pendingRatings);
      const editPaths = Object.keys(pendingEdits);
      const metadataPaths = Object.keys(pendingMetadata);
      if (ratingPaths.length === 0 && editPaths.length === 0 && metadataPaths.length === 0) return;

      const libraryState = useLibraryStore.getState();

      let nextRatings: Record<string, number> | null = null;
      for (const path of ratingPaths) {
        const rating = pendingRatings[path];
        if (libraryState.imageRatings[path] !== rating || !(path in libraryState.imageRatings)) {
          if (!nextRatings) nextRatings = { ...libraryState.imageRatings };
          nextRatings[path] = rating;
        }
      }

      let nextImageList: typeof libraryState.imageList | null = null;
      if (editPaths.length > 0 || metadataPaths.length > 0) {
        const list = libraryState.imageList;
        for (let i = 0; i < list.length; i++) {
          const img = list[i];
          const hasEdit = img.path in pendingEdits;
          const meta = pendingMetadata[img.path];
          if (!hasEdit && !meta) continue;
          const isEdited = hasEdit ? pendingEdits[img.path] : img.is_edited;
          const flag = meta ? meta.flag : img.flag;
          const tags = meta ? (meta.tags ?? img.tags) : img.tags;
          const stack = meta ? meta.stack : img.stack;
          if (isEdited === img.is_edited && flag === img.flag && tags === img.tags && stack === img.stack) continue;
          if (!nextImageList) nextImageList = list.slice();
          nextImageList[i] = meta
            ? { ...img, flag, is_edited: isEdited, stack, tags }
            : { ...img, is_edited: isEdited };
        }
      }

      if (nextRatings || nextImageList) {
        const update: Partial<typeof libraryState> = {};
        if (nextRatings) update.imageRatings = nextRatings;
        if (nextImageList) update.imageList = nextImageList;
        libraryState.setLibrary(update);
      }
    };

    const scheduleFlush = () => {
      if (flushHandle.current !== null) return;
      flushHandle.current = requestAnimationFrame(flushThumbnailBatch);
    };

    const listeners = [
      listen('preview-update-uncropped', (event) => {
        if (isEffectActive) useEditorStore.getState().setEditor({ uncroppedAdjustedPreviewUrl: event.payload });
      }),
      listen('analytics-update', (event) => {
        if (isEffectActive && event.payload.path === useEditorStore.getState().selectedImage?.path) {
          const update: { histogram?: HistogramData; waveform?: WaveformData } = {};
          if (event.payload.histogram != null) update.histogram = event.payload.histogram;
          if (event.payload.waveform != null) update.waveform = event.payload.waveform;
          useEditorStore.getState().setEditor(update);
        }
      }),
      listen('open-with-file', (event) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ initialFileToOpen: event.payload as string });
      }),
      listen('external-edit-session', (event) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ externalEditSession: event.payload });
      }),
      listen('thumbnail-progress', (event) => {
        if (!isEffectActive) return;
        progressBuffer.current = { current: event.payload.current ?? 0, total: event.payload.total };
        scheduleFlush();
      }),
      listen('thumbnail-generation-complete', () => {
        if (!isEffectActive) return;
        progressBuffer.current = null;
        useProcessStore.getState().setProcess({ thumbnailProgress: { current: 0, total: 0 } });
      }),
      listen('thumbnail-generated', (event) => {
        if (!isEffectActive) return;
        const { path, thumbnailPath, previewPath, rating, is_edited, data } = event.payload;

        if (thumbnailPath && previewPath) {
          thumbnailBuffer.current[path] = convertFileSrc(thumbnailPath.replace(/\\/g, '/'));
          mediumThumbnailBuffer.current[path] = convertFileSrc(previewPath.replace(/\\/g, '/'));
          refs.current.markGenerated(path);
        } else if (data) {
          thumbnailBuffer.current[path] = data;
          mediumThumbnailBuffer.current[path] = data;
          refs.current.markGenerated(path);
        }
        if (rating !== undefined) {
          ratingBuffer.current[path] = rating;
        }
        if (is_edited !== undefined) {
          editStatusBuffer.current[path] = is_edited;
        }
        if (thumbnailPath || data || rating !== undefined || is_edited !== undefined) {
          scheduleFlush();
        }
      }),
      listen('image-metadata-loaded', (event) => {
        if (!isEffectActive) return;
        const { path, rating, flag, is_edited, tags, stack } = event.payload;

        ratingBuffer.current[path] = rating;
        editStatusBuffer.current[path] = is_edited;
        metadataBuffer.current[path] = { flag, tags, stack };
        scheduleFlush();
      }),
      listen('ai-model-download-start', (event) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ aiModelDownloadStatus: event.payload });
      }),
      listen('ai-model-download-finish', () => {
        if (isEffectActive) useProcessStore.getState().setProcess({ aiModelDownloadStatus: null });
      }),
      listen('indexing-started', () => {
        if (isEffectActive)
          useProcessStore.getState().setProcess({ isIndexing: true, indexingProgress: { current: 0, total: 0 } });
      }),
      listen('indexing-progress', (event) => {
        if (isEffectActive) useProcessStore.getState().setProcess({ indexingProgress: event.payload });
      }),
      listen('indexing-finished', () => {
        if (isEffectActive) {
          useProcessStore.getState().setProcess({ isIndexing: false, indexingProgress: { current: 0, total: 0 } });
          const currentPath = useLibraryStore.getState().currentFolderPath;
          if (currentPath) {
            refs.current.refreshImageList();
          }
        }
      }),
      listen('batch-export-progress', (event) => {
        if (isEffectActive) useProcessStore.getState().setExportState({ progress: event.payload });
      }),
      listen('export-complete', (event) => {
        if (isEffectActive) {
          useProcessStore.getState().setExportState({ status: Status.Success });
          if (event.payload?.instagramOutputFolder) showInstagramExportComplete(event.payload.instagramOutputFolder);
          refreshAfterExport();
        }
      }),
      listen('export-error', (event) => {
        if (isEffectActive) {
          useProcessStore.getState().setExportState({
            status: Status.Error,
            errorMessage: typeof event.payload === 'string' ? event.payload : 'Unknown error',
          });
          refreshAfterExport();
        }
      }),
      listen('export-cancelling', () => {
        if (isEffectActive) useProcessStore.getState().setExportState({ status: Status.Cancelling });
      }),
      listen('export-cancelled', () => {
        if (isEffectActive) {
          useProcessStore.getState().setExportState({ status: Status.Cancelled });
          refreshAfterExport();
        }
      }),
      listen('import-start', (event) => {
        if (isEffectActive)
          useProcessStore.getState().setImportState({
            errorMessage: '',
            path: '',
            progress: { current: 0, total: event.payload.total },
            status: Status.Importing,
          });
      }),
      listen('import-progress', (event) => {
        if (isEffectActive)
          useProcessStore.getState().setImportState({
            path: event.payload.path,
            progress: { current: event.payload.current, total: event.payload.total },
          });
      }),
      listen('import-complete', (event) => {
        if (isEffectActive) {
          useProcessStore.getState().setImportState({
            status: event.payload.failed ? Status.Error : Status.Success,
            errorMessage: event.payload.errors.join('\n'),
          });
          showTransferSummary(event.payload, 'import');
          refs.current.refreshAllFolderTrees();
          const currentPath = useLibraryStore.getState().currentFolderPath;
          if (currentPath) {
            refs.current.handleSelectSubfolder(currentPath, false);
          }
        }
      }),
      listen('import-error', (event) => {
        if (isEffectActive)
          useProcessStore.getState().setImportState({
            status: Status.Error,
            errorMessage: typeof event.payload === 'string' ? event.payload : 'Unknown error',
          });
      }),
      listen('denoise-progress', (event) => {
        if (isEffectActive && isActiveDenoiseEvent(event.payload))
          useUIStore.getState().setUI((state) => ({
            denoiseModalState: { ...state.denoiseModalState, progressMessage: event.payload.message },
          }));
      }),
      listen('denoise-complete', (event) => {
        if (
          isEffectActive &&
          useUIStore.getState().denoiseModalState.targetPaths.length === 1 &&
          isActiveDenoiseEvent(event.payload)
        ) {
          useUIStore.getState().setUI((state) => ({
            denoiseModalState: {
              ...state.denoiseModalState,
              isProcessing: false,
              previewBase64: event.payload.denoised,
              originalBase64: event.payload.original ?? null,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('denoise-error', (event) => {
        if (isEffectActive && isActiveDenoiseEvent(event.payload)) {
          useUIStore.getState().setUI((state) => ({
            denoiseModalState: {
              ...state.denoiseModalState,
              isProcessing: state.denoiseModalState.targetPaths.length > 1,
              error: String(event.payload.message),
              progressMessage: null,
            },
          }));
        }
      }),
      listen('ai-denoise-progress', (event) => {
        if (!isEffectActive) return;
        const { path, progress } = event.payload;
        useEditorStore
          .getState()
          .setEditor((state) => ({ aiDenoiseProgress: { ...state.aiDenoiseProgress, [path]: progress } }));
      }),
      listen('ai-denoise-ready', (event) => {
        if (!isEffectActive) return;
        useEditorStore.getState().setEditor((state) => {
          const { [event.payload]: _, ...aiDenoiseProgress } = state.aiDenoiseProgress;
          return {
            aiDenoiseProgress,
            ...(state.selectedImage?.path === event.payload ? { adjustments: { ...state.adjustments } } : {}),
          };
        });
      }),
      listen('ai-denoise-error', (event) => {
        if (!isEffectActive) return;
        const { path, error } = event.payload;
        toast.error(`AI denoise failed: ${error}`);
        useEditorStore.getState().setEditor((state) => {
          const { [path]: _, ...aiDenoiseProgress } = state.aiDenoiseProgress;
          return {
            aiDenoiseProgress,
            ...(state.selectedImage?.path === path
              ? { adjustments: { ...state.adjustments, aiDenoiseEnabled: false } }
              : {}),
          };
        });
      }),
      listen('wgpu-frame-ready', (event) => {
        const editorState = useEditorStore.getState();
        if (
          isEffectActive &&
          !editorState.hasRenderedFirstFrame &&
          event.payload?.path === editorState.selectedImage?.path
        ) {
          editorState.setEditor({ hasRenderedFirstFrame: true });
        }
      }),
      listen('panorama-progress', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => {
            if (state.panoramaModalState.finalImageBase64 || state.panoramaModalState.error) return state;
            return { panoramaModalState: { ...state.panoramaModalState, progressMessage: event.payload } };
          });
        }
      }),
      listen('panorama-complete', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            panoramaModalState: {
              ...state.panoramaModalState,
              error: null,
              finalImageBase64: event.payload.base64,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('panorama-error', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            panoramaModalState: {
              ...state.panoramaModalState,
              error: String(event.payload),
              finalImageBase64: null,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('hdr-progress', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            hdrModalState: {
              ...state.hdrModalState,
              error: null,
              finalImageBase64: null,
              isOpen: true,
              progressMessage: event.payload,
            },
          }));
        }
      }),
      listen('hdr-complete', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            hdrModalState: {
              ...state.hdrModalState,
              error: null,
              finalImageBase64: event.payload.base64,
              isProcessing: false,
              progressMessage: 'Hdr Ready',
            },
          }));
        }
      }),
      listen('hdr-error', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            hdrModalState: {
              ...state.hdrModalState,
              error: String(event.payload),
              finalImageBase64: null,
              isProcessing: false,
              progressMessage: 'An error occurred.',
            },
          }));
        }
      }),
      listen('focus-stack-progress', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => {
            if (state.focusStackModalState.finalImageBase64 || state.focusStackModalState.error) return state;
            return { focusStackModalState: { ...state.focusStackModalState, progressMessage: event.payload } };
          });
        }
      }),
      listen('focus-stack-complete', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            focusStackModalState: {
              ...state.focusStackModalState,
              error: null,
              finalImageBase64: event.payload.base64,
              depthMapBase64: event.payload.depthMap,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('focus-stack-error', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            focusStackModalState: {
              ...state.focusStackModalState,
              error: String(event.payload),
              finalImageBase64: null,
              depthMapBase64: null,
              isProcessing: false,
              progressMessage: null,
            },
          }));
        }
      }),
      listen('culling-start', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            cullingModalState: {
              ...state.cullingModalState,
              isOpen: true,
              progress: { current: 0, total: event.payload, stage: 'Initializing...' },
              suggestions: null,
              error: null,
            },
          }));
        }
      }),
      listen('culling-progress', (event) => {
        if (isEffectActive) {
          useUIStore
            .getState()
            .setUI((state) => ({ cullingModalState: { ...state.cullingModalState, progress: event.payload } }));
        }
      }),
      listen('culling-complete', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            cullingModalState: { ...state.cullingModalState, progress: null, suggestions: event.payload },
          }));
        }
      }),
      listen('culling-error', (event) => {
        if (isEffectActive) {
          useUIStore.getState().setUI((state) => ({
            cullingModalState: { ...state.cullingModalState, progress: null, error: String(event.payload) },
          }));
        }
      }),
    ];

    return () => {
      isEffectActive = false;
      if (flushHandle.current !== null) {
        cancelAnimationFrame(flushHandle.current);
        flushHandle.current = null;
      }
      thumbnailBuffer.current = {};
      mediumThumbnailBuffer.current = {};
      ratingBuffer.current = {};
      editStatusBuffer.current = {};
      metadataBuffer.current = {};
      progressBuffer.current = null;
      listeners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, []);
}
