import { RefObject } from 'react';
import { v4 as uuid } from 'uuid';
import type { SavedHistory, HistorySnapshot } from '../utils/savedHistory';
import { normalizeLoadedAdjustments } from '../utils/adjustments';
import { describeHistoryChange, sameAdjustmentValue, type HistoryDetails } from '../utils/editHistory';
import { create } from 'zustand';
import {
  Adjustments,
  INITIAL_ADJUSTMENTS,
  MaskContainer,
  AiPatch,
  CropGeometry,
  pickCropGeometry,
} from '../utils/adjustments';
import { SelectedImage, WaveformData, BrushSettings, HistogramData, CompareView } from '../components/ui/AppProperties';
import { ImageDimensions } from '../hooks/useImageRenderSize';
import { ToolType } from '../components/panel/right/Masks';
import { OverlayMode } from '../components/panel/right/CropPanel';
import { HslMixerProperty } from '../utils/hslMixer';
import {
  DEFAULT_REFERENCE_VIEW_STATE,
  ReferenceViewCommand,
  ReferenceViewState,
  reduceReferenceView,
} from '../utils/referenceView';
import type { SpeedEditTarget } from '../components/ui/Slider';

export type CenterMarker = 'vignette' | 'centre';

interface InteractivePatch {
  url: string;
  normX: number;
  normY: number;
  normW: number;
  normH: number;
}

interface BaseRenderSize extends ImageDimensions {
  containerHeight: number;
  containerWidth: number;
  offsetX: number;
  offsetY: number;
  // CSS px per image px when fitted (transform scale 1)
  renderScale: number;
}

interface EditorState {
  // Core Image & Adjustments
  selectedImage: SelectedImage | null;
  adjustments: Adjustments;
  previewOverride: Adjustments | null;

  // History State
  history: Adjustments[];
  historyDetails: Array<HistoryDetails | null>;
  historyIndex: number;
  historyPath: string | null;
  historyRevision: number;
  historySnapshots: HistorySnapshot[];
  historyCompacted: number;
  historyWarning: string | null;
  historyWritable: boolean;
  loadSavedHistory: (path: string, history: SavedHistory | null, warning?: string | null, writable?: boolean) => void;
  createHistorySnapshot: (name: string) => void;
  renameHistorySnapshot: (id: string, name: string) => void;
  deleteHistorySnapshot: (id: string) => void;
  restoreHistorySnapshot: (id: string) => void;
  photoHistories: ReadonlyMap<string, PhotoHistory>;

  // Previews & Overlays
  finalPreviewUrl: string | null;
  uncroppedAdjustedPreviewUrl: string | null;
  previewRenderVersion: number;
  lastRenderedAdjustments: Adjustments | null;
  interactivePatch: InteractivePatch | null;
  showOriginal: boolean;
  referenceView: ReferenceViewState;
  compareView: CompareView;
  splitPosition: number;
  originalPreview: { path: string; sourcePath?: string; error?: boolean; url: string | null } | null;

  // Analytics
  histogram: HistogramData | null;
  waveform: WaveformData | null;
  isWaveformVisible: boolean;
  activeWaveformChannel: string;
  waveformHeight: number;

  // Interaction State
  isSliderDragging: boolean;
  speedEditTarget: RefObject<SpeedEditTarget | null> | null;
  isSpeedEditHeld: boolean;
  zoom: number;
  zoomRequest: { imagePath: string; percent: number | null; resetPosition?: boolean } | null;
  displaySize: ImageDimensions;
  previewSize: ImageDimensions;
  baseRenderSize: BaseRenderSize;
  originalSize: ImageDimensions;

  // Tools State
  isRotationActive: boolean;
  overlayMode: OverlayMode;
  overlayRotation: number;
  isStraightenActive: boolean;
  isAutoStraightening: boolean;
  isWbPickerActive: boolean;
  mixerPickerProperty: HslMixerProperty | null;
  isRelightPickerActive: boolean;
  activeRelightLightId: string | null;
  activeCenterMarker: CenterMarker | null;
  isGuidedPerspectiveActive: boolean;
  liveRotation: number | null;
  cropSessionSnapshot: { path: string | null; geometry: CropGeometry } | null;
  brushSettings: BrushSettings | null;

  // Masks & AI
  activeMaskContainerId: string | null;
  activeMaskId: string | null;
  activeAiPatchContainerId: string | null;
  activeAiSubMaskId: string | null;
  isMaskControlHovered: boolean;
  isGeneratingAiMask: boolean;
  isGeneratingAi: boolean;
  aiDenoiseProgress: Record<string, number>;
  isAIConnectorConnected: boolean;
  hasRenderedFirstFrame: boolean;
  patchesSentToBackend: Set<string>;

  // Clipboard
  copiedSectionAdjustments: any | null;
  copiedMask: MaskContainer | null;
  copiedAdjustments: Partial<Adjustments> | null;
  copiedAdjustmentDefaults: { adjustments: Partial<Adjustments>; defaults: Adjustments } | null;

  // Actions
  setEditor: (updater: Partial<EditorState> | ((state: EditorState) => Partial<EditorState>)) => void;
  pushHistory: (newAdjustments: Adjustments, details?: HistoryDetails | string) => void;
  undo: () => void;
  redo: () => void;
  resetHistory: (initialState: Adjustments) => void;
  saveHistory: () => void;
  restoreHistory: (initialState: Adjustments, options?: { keepSavedOnMismatch?: boolean }) => void;
  goToHistoryIndex: (index: number) => void;
  dispatchReferenceView: (command: ReferenceViewCommand) => void;
}

interface PhotoHistory {
  historySnapshots: HistorySnapshot[];
  historyCompacted: number;
  history: Adjustments[];
  historyDetails: Array<HistoryDetails | null>;
  historyIndex: number;
}

const MAX_PHOTO_HISTORIES = 50;

const rememberPhotoHistory = (
  photoHistories: ReadonlyMap<string, PhotoHistory>,
  path: string,
  photoHistory: PhotoHistory,
) => {
  const next = new Map(photoHistories);
  next.delete(path);
  next.set(path, photoHistory);
  if (next.size > MAX_PHOTO_HISTORIES) {
    const oldest = next.keys().next().value;
    if (oldest !== undefined) next.delete(oldest);
  }
  return next;
};
type CanvasToolState = Pick<
  EditorState,
  'isWbPickerActive' | 'isRelightPickerActive' | 'activeCenterMarker' | 'mixerPickerProperty'
>;

export const canvasToolState = (activeTool: Partial<CanvasToolState> = {}): CanvasToolState => ({
  isWbPickerActive: false,
  isRelightPickerActive: false,
  activeCenterMarker: null,
  mixerPickerProperty: null,
  ...activeTool,
});

export const useEditorStore = create<EditorState>((set) => ({
  selectedImage: null,
  adjustments: INITIAL_ADJUSTMENTS,
  previewOverride: null,
  history: [INITIAL_ADJUSTMENTS],
  historyDetails: [{ id: uuid(), actor: 'system', label: 'Initial state', changedKeys: [], timestamp: Date.now() }],
  historyIndex: 0,
  historyPath: null,
  historyRevision: 0,
  historySnapshots: [],
  historyCompacted: 0,
  historyWarning: null,
  historyWritable: false,
  photoHistories: new Map(),

  finalPreviewUrl: null,
  uncroppedAdjustedPreviewUrl: null,
  previewRenderVersion: 0,
  lastRenderedAdjustments: null,
  showOriginal: false,
  referenceView: DEFAULT_REFERENCE_VIEW_STATE,
  compareView: 'off',
  splitPosition: 0.5,
  originalPreview: null,
  histogram: null,
  waveform: null,
  isWaveformVisible: false,
  activeWaveformChannel: 'luma',
  waveformHeight: 220,

  isSliderDragging: false,
  speedEditTarget: null,
  isSpeedEditHeld: false,
  interactivePatch: null,
  activeMaskContainerId: null,
  activeMaskId: null,
  activeAiPatchContainerId: null,
  activeAiSubMaskId: null,

  zoom: 1,
  zoomRequest: null,
  displaySize: { width: 0, height: 0 },
  previewSize: { width: 0, height: 0 },
  baseRenderSize: {
    width: 0,
    height: 0,
    offsetX: 0,
    offsetY: 0,
    containerWidth: 0,
    containerHeight: 0,
    renderScale: 0,
  },
  originalSize: { width: 0, height: 0 },

  isRotationActive: false,
  overlayMode: 'thirds',
  overlayRotation: 0,
  isStraightenActive: false,
  isAutoStraightening: false,
  isWbPickerActive: false,
  mixerPickerProperty: null,
  isRelightPickerActive: false,
  activeRelightLightId: null,
  activeCenterMarker: null,
  isGuidedPerspectiveActive: false,
  liveRotation: null,
  cropSessionSnapshot: null,

  copiedSectionAdjustments: null,
  copiedMask: null,
  brushSettings: { size: 50, feather: 50, tool: ToolType.Brush },
  copiedAdjustments: null,
  copiedAdjustmentDefaults: null,

  isGeneratingAiMask: false,
  isAIConnectorConnected: false,
  isGeneratingAi: false,
  aiDenoiseProgress: {},
  isMaskControlHovered: false,
  hasRenderedFirstFrame: false,
  patchesSentToBackend: new Set<string>(),

  setEditor: (updater) => set((state) => (typeof updater === 'function' ? updater(state) : updater)),

  pushHistory: (newAdj, details) =>
    set((state) => {
      const newHistory = state.history.slice(0, state.historyIndex + 1);
      const historyDetails = state.history
        .slice(0, state.historyIndex + 1)
        .map((_, index) => state.historyDetails[index] ?? null);
      newHistory.push(newAdj);
      const recorded =
        details && typeof details === 'object'
          ? details
          : {
              ...describeHistoryChange(state.history[state.historyIndex] ?? INITIAL_ADJUSTMENTS, newAdj, 'user'),
              ...(typeof details === 'string' && details.trim() ? { label: details, display: undefined } : {}),
            };
      historyDetails.push({ ...recorded, id: uuid() });
      const trimmed = newHistory.length > 50;
      if (trimmed) {
        const removable = 1;
        newHistory.splice(removable, 1);
        historyDetails.splice(removable, 1);
      }
      return {
        historyRevision: state.historyRevision + 1,
        history: newHistory,
        historyDetails,
        historyIndex: newHistory.length - 1,
        historyCompacted: state.historyCompacted + (trimmed ? 1 : 0),
      };
    }),

  undo: () =>
    set((state) => {
      if (state.historyIndex > 0) {
        let newIndex = state.historyIndex - 1;
        while (newIndex >= 0 && state.historyDetails[newIndex]?.actor === 'imported') newIndex--;
        return newIndex >= 0
          ? { historyIndex: newIndex, adjustments: state.history[newIndex], historyRevision: state.historyRevision + 1 }
          : state;
      }
      return state;
    }),

  redo: () =>
    set((state) => {
      if (state.historyIndex < state.history.length - 1) {
        let newIndex = state.historyIndex + 1;
        while (newIndex < state.history.length && state.historyDetails[newIndex]?.actor === 'imported') newIndex++;
        return newIndex < state.history.length
          ? { historyIndex: newIndex, adjustments: state.history[newIndex], historyRevision: state.historyRevision + 1 }
          : state;
      }
      return state;
    }),

  resetHistory: (initialState) =>
    set((state) => {
      const photoHistories = new Map(state.photoHistories);
      if (state.selectedImage) photoHistories.delete(state.selectedImage.path);
      return {
        historyRevision: state.historyRevision + 1,
        history: [initialState],
        historyDetails: [
          { id: uuid(), actor: 'system', label: 'Initial state', changedKeys: [], timestamp: Date.now() },
        ],
        historySnapshots: [],
        historyCompacted: 0,
        historyIndex: 0,
        historyPath: state.selectedImage?.path ?? null,
        adjustments: initialState,
        photoHistories,
        ...(state.cropSessionSnapshot
          ? {
              cropSessionSnapshot: {
                path: state.selectedImage?.path ?? null,
                geometry: pickCropGeometry(initialState),
              },
            }
          : {}),
      };
    }),

  saveHistory: () =>
    set((state) => {
      const path = state.selectedImage?.path;
      if (!path || state.historyPath !== path) return state;
      return {
        photoHistories: rememberPhotoHistory(state.photoHistories, path, {
          history: [...state.history],
          historyDetails: [...state.historyDetails],
          historyIndex: state.historyIndex,
          historySnapshots: state.historySnapshots,
          historyCompacted: state.historyCompacted,
        }),
      };
    }),

  restoreHistory: (initialState, { keepSavedOnMismatch = false } = {}) =>
    set((state) => {
      const path = state.selectedImage?.path;
      const saved = path ? state.photoHistories.get(path) : undefined;
      if (path && saved && sameAdjustmentValue(saved.history[saved.historyIndex], initialState)) {
        return {
          ...saved,
          historyPath: path,
          adjustments: initialState,
          photoHistories: rememberPhotoHistory(state.photoHistories, path, saved),
        };
      }
      const fresh: PhotoHistory = {
        history: [initialState],
        historyDetails: [
          { id: uuid(), actor: 'system', label: 'Initial state', changedKeys: [], timestamp: Date.now() },
        ],
        historyIndex: 0,
        historySnapshots: [],
        historyCompacted: 0,
      };
      return {
        ...fresh,
        historyPath: path ?? null,
        adjustments: initialState,
        photoHistories:
          path && !keepSavedOnMismatch ? rememberPhotoHistory(state.photoHistories, path, fresh) : state.photoHistories,
      };
    }),

  loadSavedHistory: (path, history, warning = null, writable = true) =>
    set((state) => {
      if (state.selectedImage?.path !== path || state.historyPath !== path) return state;
      const base = { historyWarning: warning, historyWritable: writable };
      if (!history) return base;
      const index = history.steps.findIndex((step) => step.id === history.currentStepId);
      if (
        index < 0 ||
        !sameAdjustmentValue(normalizeLoadedAdjustments(history.steps[index].adjustments), state.adjustments)
      )
        return base;
      return {
        ...base,
        history: history.steps.map((step) => normalizeLoadedAdjustments(step.adjustments)),
        historyDetails: history.steps.map(({ id, label, actor, timestamp, changedKeys, display, source }) => ({
          id,
          label,
          actor,
          timestamp,
          changedKeys,
          ...(display ? { display } : {}),
          ...(source ? { source } : {}),
        })),
        historyIndex: index,
        historySnapshots: history.snapshots,
        historyCompacted: history.compactedSteps,
      };
    }),

  createHistorySnapshot: (name) =>
    set((state) => {
      name = [...name.trim()].slice(0, 120).join('');
      if (!name || state.historySnapshots.length >= 20) return state;
      return {
        historyRevision: state.historyRevision + 1,
        historySnapshots: [
          ...state.historySnapshots,
          { id: uuid(), name, createdAt: Date.now(), adjustments: structuredClone(state.adjustments) },
        ],
      };
    }),
  renameHistorySnapshot: (id, name) =>
    set((state) => {
      name = [...name.trim()].slice(0, 120).join('');
      return name
        ? {
            historyRevision: state.historyRevision + 1,
            historySnapshots: state.historySnapshots.map((snapshot) =>
              snapshot.id === id ? { ...snapshot, name } : snapshot,
            ),
          }
        : state;
    }),
  deleteHistorySnapshot: (id) =>
    set((state) => ({
      historyRevision: state.historyRevision + 1,
      historySnapshots: state.historySnapshots.filter((snapshot) => snapshot.id !== id),
    })),
  restoreHistorySnapshot: (id) => {
    const state = useEditorStore.getState();
    const snapshot = state.historySnapshots.find((snapshot) => snapshot.id === id);
    if (!snapshot) return;
    const adjustments = normalizeLoadedAdjustments(structuredClone(snapshot.adjustments));
    state.pushHistory(adjustments, `Snapshot: ${snapshot.name}`);
    state.setEditor({ adjustments });
  },

  goToHistoryIndex: (index) =>
    set((state) => {
      if (index >= 0 && index < state.history.length && state.historyDetails[index]?.actor !== 'imported') {
        return { historyIndex: index, adjustments: state.history[index], historyRevision: state.historyRevision + 1 };
      }
      return state;
    }),

  dispatchReferenceView: (command) =>
    set((state) => {
      const referenceView = reduceReferenceView(state.referenceView, command);
      if (command.type === 'close-chooser') return { referenceView };
      if (command.type === 'set-wipe-position')
        return { referenceView, splitPosition: referenceView.wipePosition / 100 };
      if (command.type === 'swap-wipe-sides') return { referenceView };
      return {
        referenceView,
        compareView: referenceView.mode === 'off' ? 'off' : referenceView.mode === 'wipe' ? 'vertical' : 'sideBySide',
        showOriginal: false,
        previewOverride: null,
      };
    }),
}));
