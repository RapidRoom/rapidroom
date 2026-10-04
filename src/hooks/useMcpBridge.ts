import { useEffect, useState } from 'react';
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
  kind: 'select-image' | 'get-histogram' | 'apply-adjustments' | 'reset-adjustments';
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
  if (JSON.stringify(nextAdjustments) === JSON.stringify(editor.adjustments)) {
    return true;
  }
  const previousRenderVersion = editor.previewRenderVersion;
  editor.setEditor({ adjustments: nextAdjustments });
  editor.pushHistory(nextAdjustments);
  return waitForAdjustmentRender(path, previousRenderVersion, nextAdjustments);
}

async function syncState(path: string): Promise<McpStateResponse> {
  return invoke<McpStateResponse>('sync_editor_state', {
    path,
    adjustments: useEditorStore.getState().adjustments,
  });
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
          await handleImageSelect(command.path, true);
          await waitForImage(command.path);
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
            await handleImageSelect(command.path, true);
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
  }, [enabled, handleImageSelect]);
}
