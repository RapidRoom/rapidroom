import { createResetAdjustmentsWithLensDefaults } from '../utils/rawLensDefaults';
import { adoptVirtualCopy } from '../utils/adoptVirtualCopy';
import { assessEdit } from '../utils/editAssessment';
import { getReferenceLabel } from '../utils/referenceView';
import { afterPendingLibraryMetadataWrites } from '../utils/libraryMetadataWrites';
import { listMcpPresets, resolveMcpPreset, presetCatalogSelector, type PresetSelector } from '../utils/mcpPresets';
import { usePresetCatalog } from '../store/usePresetCatalog';
import { preparePresetApplication, commitPresetApplication } from '../utils/presetApplication';
import { clearPresetHoverPreview } from '../utils/presetHoverPreview';
import { hashPresetAdjustments, useAppliedPresetStore, loadAppliedPreset } from '../utils/appliedPreset';
import { toast } from 'react-toastify';
import { isPathInCardRoot } from '../utils/cardMode';
import { describeHistoryChange, sameAdjustmentValue } from '../utils/editHistory';
import { useUIStore } from '../store/useUIStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEditorStore } from '../store/useEditorStore';
import type { HistogramData } from '../components/ui/AppProperties';
import { debouncedSetHistory } from './useEditorActions';
import { buildParametricCurves, normalizeLoadedAdjustments, type Adjustments } from '../utils/adjustments';

interface McpCommand {
  requestId: string;
  kind:
    | 'adopt-virtual-copy'
    | 'create-virtual-copy'
    | 'set-image-metadata'
    | 'assess-edits'
    | 'open-reference-view'
    | 'request-delete-image'
    | 'list-presets'
    | 'apply-preset'
    | 'select-image'
    | 'get-histogram'
    | 'apply-adjustments'
    | 'reset-adjustments'
    | 'undo'
    | 'redo'
    | 'history-list'
    | 'history-state'
    | 'revision-state'
    | 'editor-context';
  path: string;
  adjustments?: Adjustments;
  baseAdjustments?: Adjustments;
  historyIndex?: number;
  options?: PresetSelector;
  imagePaths?: string[];
  changes?: Record<string, unknown>;
  referencePath?: string;
  sourceEditRevision?: string;
  originalExpectedRevision?: string;
  deleteCopyAfterAdoption?: boolean;
}

interface McpStateResponse {
  imagePath: string;
  adjustments: Adjustments;
  editRevision: string;
  isSelected: boolean;
  validationError: string | null;
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
async function applyEdit(
  path: string,
  nextAdjustments: Adjustments,
  baseAdjustments?: Adjustments,
  label?: string,
): Promise<boolean> {
  debouncedSetHistory.flush();
  const editor = useEditorStore.getState();
  if (
    baseAdjustments &&
    (editor.selectedImage?.path !== path ||
      !editor.selectedImage.isReady ||
      !sameAdjustmentValue(baseAdjustments, editor.adjustments))
  )
    throw new Error(
      'Active image or edit changed before the single-mask operation; read the current revision and retry',
    );
  if (sameAdjustmentValue(nextAdjustments, editor.adjustments)) {
    return true;
  }
  const previousRenderVersion = editor.previewRenderVersion;
  editor.setEditor({ adjustments: nextAdjustments });
  const details = describeHistoryChange(editor.adjustments, nextAdjustments, 'assistant');
  if (label) details.label = label;
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

interface McpWorkflowActions {
  requestDeletePaths?: (paths: string[], assistantRequest?: boolean) => void;
  refreshImageList?: () => Promise<void>;
}

export function useMcpBridge(
  handleImageSelect: (path: string, openInEditor?: boolean) => Promise<void>,
  workflow: McpWorkflowActions = {},
) {
  const workflowRef = useRef(workflow);
  workflowRef.current = workflow;
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
        const assertCurrentDraft = () => {
          const current = useEditorStore.getState();
          if (
            !current.selectedImage?.isReady ||
            current.selectedImage.path !== command.path ||
            !sameAdjustmentValue(command.baseAdjustments, current.adjustments)
          )
            throw new Error('Active image or edit changed; read the current revision and retry');
          return current;
        };
        if (command.kind === 'adopt-virtual-copy') {
          const response = await adoptVirtualCopy(command, {
            open: (path, editor) => selectImageRef.current(path, editor),
            waitForImage,
            sync: syncState,
            apply: applyEdit,
            requestDeletePaths: workflowRef.current.requestDeletePaths,
          });
          await invoke('ui_response', { requestId: command.requestId, response, error: null });
          return;
        }
        if (command.kind === 'set-image-metadata') {
          await new Promise<void>((resolve) => afterPendingLibraryMetadataWrites(resolve));
          const paths = command.imagePaths ?? [];
          if (command.path) assertCurrentDraft();
          else if (paths.includes(useEditorStore.getState().selectedImage?.path ?? ''))
            throw new Error('An affected image became active; read its revision and retry');
          const readOnly = paths.some((path) =>
            isPathInCardRoot(path.split('?vc=')[0], useLibraryStore.getState().cardBrowseRoot),
          );
          if (readOnly) throw new Error('Card mode is read-only');
          await invoke('set_mcp_image_metadata', { paths, changes: command.changes });
          let refreshPending = false;
          try {
            await workflowRef.current.refreshImageList?.();
          } catch {
            refreshPending = true;
          }
          await invoke('ui_response', {
            requestId: command.requestId,
            response: { imagePaths: paths, changes: command.changes, metadataUpdated: true, refreshPending },
            error: null,
          });
          return;
        }
        if (
          ['create-virtual-copy', 'assess-edits', 'open-reference-view', 'request-delete-image'].includes(command.kind)
        ) {
          const current = assertCurrentDraft();
          const readOnly = isPathInCardRoot(command.path.split('?vc=')[0], useLibraryStore.getState().cardBrowseRoot);
          if (readOnly && ['create-virtual-copy', 'request-delete-image'].includes(command.kind))
            throw new Error('Card mode is read-only');
          if (command.kind === 'assess-edits') {
            const state = await syncState(command.path);
            assertCurrentDraft();
            await invoke('ui_response', {
              requestId: command.requestId,
              response: {
                imagePath: command.path,
                editRevision: state.editRevision,
                assessment: assessEdit(current.adjustments, current.selectedImage!, current.history.length),
              },
              error: null,
            });
          } else if (command.kind === 'create-virtual-copy') {
            const imagePath = await invoke<string>('create_mcp_virtual_copy', {
              path: command.path,
              adjustments: current.adjustments,
            });
            let refreshPending = false;
            try {
              await workflowRef.current.refreshImageList?.();
            } catch {
              refreshPending = true;
            }
            toast.info('Created an AI edit virtual copy. The original is preserved.', { autoClose: 3500 });
            await invoke('ui_response', {
              requestId: command.requestId,
              response: {
                imagePath,
                sourceImagePath: command.path,
                sourceEditRevision: command.sourceEditRevision,
                virtualCopy: imagePath.split('?vc=').at(-1),
                label: 'AI edit',
                isSelected: false,
                refreshPending,
              },
              error: null,
            });
          } else if (command.kind === 'open-reference-view') {
            if (!command.referencePath || command.referencePath === command.path)
              throw new Error('Choose a separate reference image');
            current.dispatchReferenceView({
              type: 'set-reference',
              image: { path: command.referencePath, label: getReferenceLabel(command.referencePath) },
            });
            current.dispatchReferenceView({ type: 'toggle-wipe' });
            await invoke('ui_response', {
              requestId: command.requestId,
              response: { imagePath: command.path, referencePath: command.referencePath, mode: 'wipe' },
              error: null,
            });
          } else {
            const requestDelete = workflowRef.current.requestDeletePaths;
            if (!requestDelete) throw new Error('The app deletion confirmation flow is unavailable');
            if (useUIStore.getState().confirmModalState.isOpen)
              throw new Error('Finish the current confirmation dialog before requesting deletion');
            requestDelete([command.path], true);
            await invoke('ui_response', {
              requestId: command.requestId,
              response: {
                imagePath: command.path,
                confirmationPending: true,
                includesVirtualCopies: !command.path.includes('?vc='),
              },
              error: null,
            });
          }
          return;
        }
        if (command.kind === 'list-presets') {
          const response = await listMcpPresets(command.options ?? {});
          await invoke('ui_response', { requestId: command.requestId, response, error: null });
          return;
        }
        if (command.kind === 'apply-preset') {
          debouncedSetHistory.flush();
          const before = useEditorStore.getState();
          if (
            before.selectedImage?.path !== command.path ||
            !before.selectedImage.isReady ||
            !sameAdjustmentValue(command.baseAdjustments, before.adjustments)
          )
            throw new Error('Active image or edit changed before preset lookup; read the current revision and retry');
          const selected = await resolveMcpPreset(command.options ?? {});
          if (
            useEditorStore.getState().adjustments !== before.adjustments ||
            useEditorStore.getState().selectedImage !== before.selectedImage
          )
            throw new Error('Active image or edit changed during preset lookup; retry');
          const plan = preparePresetApplication(selected.preset, command.options?.amount ?? 100);
          await invoke('validate_mcp_adjustments', { adjustments: plan.application.result });
          if (!plan.isCurrent() || usePresetCatalog.getState().presets !== selected.snapshot)
            throw new Error(
              'Active image, edit, or preset catalog changed during validation; read the current revision and retry',
            );
          clearPresetHoverPreview();
          let rendered = Promise.resolve(true);
          commitPresetApplication(plan, (next) => {
            rendered = applyEdit(command.path, next, before.adjustments);
          });
          renderPending = !(await rendered);
          const response = await syncState(command.path);
          const changedKeys = Object.keys(plan.application.result).filter(
            (key) => !sameAdjustmentValue(before.adjustments[key], plan.application.result[key]),
          );
          await invoke('ui_response', {
            requestId: command.requestId,
            response: {
              imagePath: response.imagePath,
              editRevision: response.editRevision,
              changedKeys,
              renderPending,
              appliedPreset: {
                id: selected.id,
                name: selected.preset.name,
                groupId: selected.groupId,
                amount: plan.application.amount,
              },
            },
            error: null,
          });
          return;
        }
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
        } else if (command.kind === 'history-state') {
          const current = useEditorStore.getState();
          const index = command.historyIndex;
          if (current.selectedImage?.path !== command.path || !current.selectedImage.isReady) {
            throw new Error('Active image changed before the history snapshot read');
          }
          if (index === undefined || !Number.isSafeInteger(index) || index < 0 || index >= current.history.length) {
            throw new Error('historyIndex is outside the active editor history');
          }
          await invoke('ui_response', {
            requestId: command.requestId,
            response: {
              imagePath: command.path,
              historyIndex: index,
              historyLabel: current.historyDetails[index]?.label ?? (index === 0 ? 'Initial State' : 'Adjustment'),
              adjustments: current.history[index],
            },
            error: null,
          });
          return;
        } else if (command.kind === 'history-list' || command.kind === 'editor-context') {
          const editor = useEditorStore.getState();
          if (command.path && (editor.selectedImage?.path !== command.path || !editor.selectedImage.isReady)) {
            throw new Error('Active image changed before the MCP read');
          }
          const current = useEditorStore.getState();
          let appliedPreset = null;
          if (command.kind === 'editor-context' && current.selectedImage?.isReady) {
            const path = current.selectedImage.path;
            await loadAppliedPreset(path);
            const record = useAppliedPresetStore.getState().records[path];
            if (record && (await hashPresetAdjustments(current.adjustments)) === record.adjustmentsHash)
              appliedPreset = {
                id: await presetCatalogSelector('preset', record.id),
                name: record.name,
                amount: record.amount,
              };
            if (
              useEditorStore.getState().adjustments !== current.adjustments ||
              useEditorStore.getState().selectedImage !== current.selectedImage
            )
              throw new Error('Active image or edit changed during context read; retry');
          }
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
                  appliedPreset,
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
          if (command.baseAdjustments && useEditorStore.getState().selectedImage?.path !== command.path) {
            throw new Error('Active image changed before the single-mask operation');
          }
          if (useEditorStore.getState().selectedImage?.path !== command.path) {
            await selectImageRef.current(command.path, true);
            await waitForImage(command.path);
          }

          let nextAdjustments: Adjustments;
          if (command.kind === 'reset-adjustments') {
            // Same result as the library's reset, but as an undoable step;
            // autosave writes it to the sidecar like any other edit.
            const before = useEditorStore.getState();
            nextAdjustments = await createResetAdjustmentsWithLensDefaults(before.selectedImage);
            const current = useEditorStore.getState();
            if (current.selectedImage?.path !== command.path || current.adjustments !== before.adjustments) {
              throw new Error('Active edit changed while preparing Reset');
            }
          } else if (command.adjustments) {
            nextAdjustments = normalizeMcpAdjustments(command.adjustments);
          } else {
            throw new Error('MCP adjustment command did not include adjustments');
          }
          renderPending = !(await applyEdit(command.path, nextAdjustments, command.baseAdjustments));
        }

        const response = await syncState(command.path);
        const history = useEditorStore.getState();
        await invoke('ui_response', {
          requestId: command.requestId,
          response: {
            ...response,
            isSelected: true,
            renderPending,
            ...(['undo', 'redo'].includes(command.kind)
              ? {
                  historyIndex: history.historyIndex,
                  historyLabel: history.historyDetails[history.historyIndex]?.label ?? 'Initial State',
                  canUndo: history.historyIndex > 0,
                  canRedo: history.historyIndex < history.history.length - 1,
                }
              : {}),
          },
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
