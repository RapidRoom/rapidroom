import { toast } from 'react-toastify';
import { isPathInCardRoot } from '../utils/cardMode';
import { describeHistoryChange, sameAdjustmentValue } from '../utils/editHistory';
import { useUIStore } from '../store/useUIStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEditorStore } from '../store/useEditorStore';
import { debouncedSetHistory } from './useEditorActions';
import {
  INITIAL_ADJUSTMENTS,
  buildParametricCurves,
  normalizeLoadedAdjustments,
  type Adjustments,
} from '../utils/adjustments';

interface McpCommand {
  requestId: string;
  kind:
    | 'select-image'
    | 'get-histogram'
    | 'apply-adjustments'
    | 'reset-adjustments'
    | 'undo'
    | 'redo'
    | 'history-list'
    | 'revision-state'
    | 'editor-context';
  path: string;
  adjustments?: Adjustments;
}

interface McpStateResponse {
  imagePath: string;
  adjustments: Adjustments;
  editRevision: string;
  isSelected: boolean;
  validationError: string | null;
}

interface HistogramData {
  red: Array<number>;
  green: Array<number>;
  blue: Array<number>;
  luma: Array<number>;
}

interface McpHistogramResponse {
  imagePath: string;
  histogram: HistogramData;
  channelCount: number;
  isSelected: boolean;
}

function isHistogramData(value: unknown): value is HistogramData {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Record<string, unknown>;
  return ['red', 'green', 'blue', 'luma'].every((channel) => {
    const data = candidate[channel];
    return Array.isArray(data) && data.length === 256 && data.every((entry) => typeof entry === 'number');
  });
}

const wait = (durationMs: number) => new Promise<void>((resolve) => setTimeout(resolve, durationMs));

// 40 s at 250 ms. The backend waits 60 s, so a slow UI reports its own timeout.
const POLL_INTERVAL_MS = 250;
const POLL_ATTEMPTS = 160;

async function waitForImage(path: string): Promise<void> {
  for (let attempt = 0; attempt < POLL_ATTEMPTS; attempt += 1) {
    const selectedImage = useEditorStore.getState().selectedImage;
    if (selectedImage?.path === path && selectedImage.isReady) return;
    await wait(POLL_INTERVAL_MS);
  }
  throw new Error(`RapidRAW did not finish loading ${path}`);
}

async function waitForHistogram(path: string): Promise<HistogramData> {
  for (let attempt = 0; attempt < POLL_ATTEMPTS; attempt += 1) {
    const editor = useEditorStore.getState();
    const histogram = editor.histogram as unknown;
    if (editor.selectedImage?.path === path && editor.selectedImage.isReady && isHistogramData(histogram)) {
      return histogram;
    }
    await wait(POLL_INTERVAL_MS);
  }
  throw new Error(`RapidRAW did not finish calculating the histogram for ${path}`);
}

async function waitForAdjustmentRender(
  path: string,
  previousRenderVersion: number,
  expected: Adjustments,
): Promise<boolean> {
  const expectedKey = JSON.stringify(expected);
  for (let attempt = 0; attempt < POLL_ATTEMPTS; attempt += 1) {
    const editor = useEditorStore.getState();
    if (editor.selectedImage?.path !== path) {
      throw new Error('Active image changed while waiting for the MCP edit preview');
    }
    const rendered = editor.lastRenderedAdjustments;
    if (
      editor.selectedImage?.path === path &&
      editor.selectedImage.isReady &&
      editor.previewRenderVersion > previousRenderVersion &&
      rendered !== null &&
      (rendered === expected || JSON.stringify(rendered) === expectedKey)
    ) {
      return true;
    }
    await wait(POLL_INTERVAL_MS);
  }
  // The edit is applied and in the history either way; only the preview is late.
  return false;
}

// Applies an MCP edit as one undoable step, after any pending GUI history push,
// so an older queued snapshot cannot land on top of it. Returns whether a new
// preview was rendered for it; ongoing user gestures remain a live-test case.
async function applyEdit(path: string, nextAdjustments: Adjustments): Promise<boolean> {
  debouncedSetHistory.flush();
  const editor = useEditorStore.getState();
  if (sameAdjustmentValue(nextAdjustments, editor.adjustments)) {
    return true;
  }
  const previousRenderVersion = editor.previewRenderVersion;
  editor.setEditor({ adjustments: nextAdjustments });
  const details = describeHistoryChange(editor.adjustments, nextAdjustments, 'assistant');
  editor.pushHistory(nextAdjustments, details);
  toast.info(details.label, { autoClose: 2500 });
  return waitForAdjustmentRender(path, previousRenderVersion, nextAdjustments);
}

async function syncState(path: string): Promise<McpStateResponse> {
  const editor = useEditorStore.getState();
  if (editor.selectedImage?.path !== path || !editor.selectedImage.isReady) {
    throw new Error('Active image changed before the MCP command completed');
  }
  return invoke<McpStateResponse>('sync_editor_state', { path, adjustments: editor.adjustments });
}

function normalizeMcpAdjustments(adjustments: Adjustments): Adjustments {
  const normalized = normalizeLoadedAdjustments(adjustments);
  if (normalized.curveMode === 'parametric' && normalized.parametricCurve) {
    return { ...normalized, curves: buildParametricCurves(normalized.parametricCurve) };
  }
  if (normalized.curveMode !== 'parametric' && normalized.pointCurves) {
    return { ...normalized, curves: normalized.pointCurves };
  }
  return normalized;
}

export function useMcpBridge(handleImageSelect: (path: string, openInEditor?: boolean) => Promise<void>) {
  const selectedImage = useEditorStore((state) => state.selectedImage);
  const adjustments = useEditorStore((state) => state.adjustments);
  const [enabled, setEnabled] = useState(false);
  const selectImageRef = useRef(handleImageSelect);
  useEffect(() => {
    selectImageRef.current = handleImageSelect;
  }, [handleImageSelect]);

  useEffect(() => {
    // The commands below exist only in builds with the `mcp` cargo feature.
    invoke('mcp_status')
      .then(() => setEnabled(true))
      .catch(() => setEnabled(false));
  }, []);

  useEffect(() => {
    if (!enabled) return;
    if (!selectedImage?.path || !selectedImage.isReady) {
      invoke('clear_editor_session').catch((error) => console.warn('Failed to clear the MCP editor session:', error));
      return;
    }

    const sync = () => {
      invoke('sync_editor_state', {
        path: selectedImage.path,
        adjustments,
      }).catch((error) => console.warn('Failed to mirror editor state for MCP:', error));
    };
    sync();
  }, [enabled, selectedImage?.path, selectedImage?.isReady, adjustments]);

  useEffect(() => {
    if (!enabled) return;
    let active = true;
    const unlistenPromise = listen<McpCommand>('mcp-command', async (event) => {
      if (!active) return;
      const command = event.payload;
      let renderPending = false;

      try {
        if (command.kind === 'select-image') {
          await selectImageRef.current(command.path, true);
          await waitForImage(command.path);
        } else if (command.kind === 'revision-state') {
          const current = useEditorStore.getState();
          if (current.selectedImage?.path !== command.path || !current.selectedImage.isReady) {
            throw new Error('Active image changed before the MCP revision read');
          }
          const state = await invoke<McpStateResponse>('sync_editor_state', {
            path: command.path,
            adjustments: current.adjustments,
          });
          const actor = sameAdjustmentValue(current.adjustments, current.history[current.historyIndex])
            ? (current.historyDetails[current.historyIndex]?.actor ?? 'user')
            : 'user';
          await invoke('ui_response', {
            requestId: command.requestId,
            response: { state, actor, history: current.history.map((adjustments) => ({ adjustments })) },
            error: null,
          });
          return;
        } else if (command.kind === 'history-list' || command.kind === 'editor-context') {
          const editor = useEditorStore.getState();
          if (command.path && (editor.selectedImage?.path !== command.path || !editor.selectedImage.isReady)) {
            throw new Error('Active image changed before the MCP read');
          }
          const current = useEditorStore.getState();
          const response =
            command.kind === 'history-list'
              ? {
                  imagePath: command.path,
                  historyIndex: current.historyIndex,
                  canUndo: current.historyIndex > 0,
                  canRedo: current.historyIndex < current.history.length - 1,
                  entries: current.history.map((snapshot, index) => ({
                    index,
                    active: index === current.historyIndex,
                    undone: index > current.historyIndex,
                    ...(current.historyDetails[index] ??
                      (index === 0
                        ? { label: 'Initial State', actor: 'user', changedKeys: [], timestamp: null }
                        : { ...describeHistoryChange(current.history[index - 1], snapshot, 'user'), timestamp: null })),
                  })),
                }
              : {
                  imagePath: current.selectedImage?.isReady ? current.selectedImage.path : null,
                  editRevision: current.selectedImage?.isReady
                    ? (await syncState(current.selectedImage.path)).editRevision
                    : null,
                  dimensions: current.selectedImage?.isReady
                    ? { width: current.selectedImage.width, height: current.selectedImage.height }
                    : null,
                  virtualCopy: current.selectedImage?.path.includes('?vc=')
                    ? current.selectedImage.path.split('?vc=').at(-1)
                    : null,
                  exif: Object.fromEntries(
                    [
                      'Make',
                      'Model',
                      'LensModel',
                      'FNumber',
                      'ExposureTime',
                      'PhotographicSensitivity',
                      'ISO',
                      'FocalLength',
                      'DateTimeOriginal',
                    ]
                      .filter((key) => current.selectedImage?.exif?.[key] !== undefined)
                      .map((key) => [key, current.selectedImage?.exif[key]]),
                  ),
                  crop: current.selectedImage?.isReady ? current.adjustments.crop : null,
                  maskCount: current.selectedImage?.isReady ? current.adjustments.masks.length : 0,
                  masks: (current.selectedImage?.isReady ? current.adjustments.masks.slice(0, 256) : []).map(
                    (mask) => ({
                      id: mask.id,
                      name: String(mask.name ?? '').slice(0, 128),
                      opacity: mask.opacity,
                      visible: mask.visible,
                    }),
                  ),
                  activePanel: useUIStore.getState().activePanel,
                  activePanels: useUIStore.getState().activePanels,
                  activeMaskContainerId: current.activeMaskContainerId,
                  activeMaskId: current.activeMaskId,
                  cardMode: isPathInCardRoot(command.path.split('?vc=')[0], useLibraryStore.getState().cardBrowseRoot),
                };
          await invoke('ui_response', { requestId: command.requestId, response, error: null });
          return;
        } else if (command.kind === 'undo' || command.kind === 'redo') {
          const editor = useEditorStore.getState();
          if (editor.selectedImage?.path !== command.path || !editor.selectedImage.isReady) {
            throw new Error('Active image changed before the MCP history action');
          }
          debouncedSetHistory.flush();
          const before = useEditorStore.getState();
          const previousRenderVersion = before.previewRenderVersion;
          before[command.kind]();
          const after = useEditorStore.getState();
          if (after.historyIndex !== before.historyIndex) {
            renderPending = !(await waitForAdjustmentRender(command.path, previousRenderVersion, after.adjustments));
          }
        } else if (command.kind === 'get-histogram') {
          const histogram = await waitForHistogram(command.path);
          const response: McpHistogramResponse = {
            imagePath: command.path,
            histogram,
            channelCount: 256,
            isSelected: true,
          };
          await invoke('ui_response', {
            requestId: command.requestId,
            response,
            error: null,
          });
          return;
        } else {
          if (useEditorStore.getState().selectedImage?.path !== command.path) {
            await selectImageRef.current(command.path, true);
            await waitForImage(command.path);
          }

          let nextAdjustments: Adjustments;
          if (command.kind === 'reset-adjustments') {
            // Same result as the library's reset, but as an undoable step;
            // autosave writes it to the sidecar like any other edit.
            const image = useEditorStore.getState().selectedImage;
            const aspectRatio = image && image.width > 0 && image.height > 0 ? image.width / image.height : null;
            nextAdjustments = { ...INITIAL_ADJUSTMENTS, aspectRatio, aiPatches: [] };
          } else if (command.adjustments) {
            nextAdjustments = normalizeMcpAdjustments(command.adjustments);
          } else {
            throw new Error('MCP adjustment command did not include adjustments');
          }
          renderPending = !(await applyEdit(command.path, nextAdjustments));
        }

        const response = await syncState(command.path);
        await invoke('ui_response', {
          requestId: command.requestId,
          response: { ...response, isSelected: true, renderPending },
          error: null,
        });
      } catch (error) {
        await invoke('ui_response', {
          requestId: command.requestId,
          response: {},
          error: error instanceof Error ? error.message : String(error),
        }).catch((responseError) => console.warn('Failed to report MCP UI error:', responseError));
      }
    });

    return () => {
      active = false;
      unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [enabled]);
}
