import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';
import { debouncedSave, debouncedSetHistory, waitForPendingEditorSaves } from '../hooks/useEditorActions';
import type { Adjustments } from './adjustments';
import { sameAdjustmentValue } from './editHistory';
import { isPathInCardRoot } from './cardMode';
import {
  hashPresetAdjustments,
  loadAppliedPreset,
  rememberAppliedPreset,
  rememberAppliedStyleStack,
  useAppliedPresetStore,
  styleSnapshot,
} from './appliedPreset';
import { clearPresetHoverPreview } from './presetHoverPreview';

interface AdoptionRequest {
  path: string;
  baseAdjustments?: Adjustments;
  originalExpectedRevision?: string;
  deleteCopyAfterAdoption?: boolean;
}

interface AdoptionActions {
  open(path: string, openInEditor?: boolean): Promise<void>;
  waitForImage(path: string): Promise<void>;
  sync(path: string): Promise<{ imagePath: string; editRevision: string }>;
  apply(path: string, next: Adjustments, base: Adjustments, label: string): Promise<boolean>;
  requestDeletePaths?: (paths: string[], assistantRequest?: boolean) => void;
}

export async function adoptVirtualCopy(command: AdoptionRequest, actions: AdoptionActions) {
  const copy = useEditorStore.getState();
  const originalPath = command.path.split('?vc=')[0];
  if (!command.path.includes('?vc=') || !command.originalExpectedRevision)
    throw new Error('An active virtual copy and originalExpectedRevision are required');
  if (
    !copy.selectedImage?.isReady ||
    copy.selectedImage.path !== command.path ||
    !sameAdjustmentValue(command.baseAdjustments, copy.adjustments)
  )
    throw new Error('AI copy changed; read its current revision and retry');
  if (isPathInCardRoot(originalPath, useLibraryStore.getState().cardBrowseRoot))
    throw new Error('Card mode is read-only');
  const requestDeletion = command.deleteCopyAfterAdoption !== false;
  if (requestDeletion && (!actions.requestDeletePaths || useUIStore.getState().confirmModalState.isOpen))
    throw new Error('Finish the current confirmation dialog before adopting the AI copy');
  const recipe = copy.adjustments;
  const assertCopy = () => {
    const now = useEditorStore.getState();
    if (now.selectedImage?.path !== command.path || !now.selectedImage.isReady || now.adjustments !== recipe)
      throw new Error('AI copy changed during adoption preflight; retry');
  };
  await invoke('validate_mcp_adjustments', { adjustments: recipe });
  assertCopy();
  // Save the copy's live draft before navigating away, using ordinary autosave.
  // A rejected adoption must still leave that version available to the user.
  debouncedSetHistory.flush();
  debouncedSave(command.path, recipe);
  await debouncedSave.flush();
  await waitForPendingEditorSaves();
  assertCopy();
  await loadAppliedPreset(command.path);
  const record = useAppliedPresetStore.getState().records[command.path];
  const stack = styleSnapshot(command.path, recipe);
  const applied =
    record && stack
      ? { ...record, stack }
      : record && (await hashPresetAdjustments(recipe)) === record.adjustmentsHash
        ? record
        : null;
  assertCopy();
  debouncedSetHistory.flush();
  await actions.open(originalPath, true);
  await actions.waitForImage(originalPath);
  debouncedSetHistory.flush();
  const original = useEditorStore.getState();
  if (!original.selectedImage?.isReady || original.selectedImage.path !== originalPath)
    throw new Error('Active image changed while opening the original');
  const state = await actions.sync(originalPath);
  if (state.editRevision !== command.originalExpectedRevision)
    throw new Error(
      'Original edit changed since the copy was created/reviewed. No adoption was applied. Review the original and choose whether to replace it.',
    );
  const now = useEditorStore.getState();
  if (
    now.selectedImage?.path !== originalPath ||
    !now.selectedImage.isReady ||
    now.adjustments !== original.adjustments
  )
    throw new Error('Original changed during adoption preflight; read its current revision and retry');
  if (isPathInCardRoot(originalPath, useLibraryStore.getState().cardBrowseRoot))
    throw new Error('Card mode is read-only');
  clearPresetHoverPreview();
  const changedKeys = [...new Set([...Object.keys(original.adjustments), ...Object.keys(recipe)])]
    .sort()
    .filter((key) => !sameAdjustmentValue(original.adjustments[key], recipe[key]));
  // Capture the committed revision before waiting for a preview. A later image
  // switch only makes the preview pending; it cannot undo an already committed edit.
  const rendered = actions.apply(originalPath, recipe, original.adjustments, 'AI: adopted AI edit').catch(() => false);
  const committed = useEditorStore.getState();
  if (committed.selectedImage?.path !== originalPath || !sameAdjustmentValue(committed.adjustments, recipe)) {
    await rendered;
    throw new Error('Original changed before adoption committed; no AI-copy deletion was requested');
  }
  const response = await actions.sync(originalPath);
  if (applied?.stack) void rememberAppliedStyleStack(originalPath, applied.stack, recipe);
  else if (applied) void rememberAppliedPreset(originalPath, applied, applied.amount, recipe);
  else void rememberAppliedStyleStack(originalPath, null, recipe);
  const stillOriginal = () => {
    const current = useEditorStore.getState();
    return (
      current.selectedImage?.path === originalPath &&
      current.selectedImage.isReady &&
      sameAdjustmentValue(current.adjustments, recipe)
    );
  };
  // Persist the adopted original before offering copy deletion: a subsequent
  // library refresh must not reload the older base recipe from disk.
  let originalSaved = false;
  if (stillOriginal()) {
    try {
      debouncedSave(originalPath, recipe);
      await debouncedSave.flush();
      await waitForPendingEditorSaves();
      originalSaved = true;
    } catch {
      /* Ordinary autosave reports the failure; retain the AI copy. */
    }
  }
  const renderPending = !(await rendered);
  let copyDeletionConfirmationPending = false;
  let copyDeletionReason: string | undefined;
  if (requestDeletion) {
    if (!originalSaved) copyDeletionReason = 'Original save did not complete; the AI copy is retained';
    else if (!stillOriginal()) copyDeletionReason = 'Original changed after adoption; the AI copy is retained';
    else if (useUIStore.getState().confirmModalState.isOpen)
      copyDeletionReason = 'Another confirmation is open; the AI copy is retained';
    else {
      actions.requestDeletePaths!([command.path], true);
      copyDeletionConfirmationPending = true;
    }
  }
  toast.info('Adopted the AI edit on the original. Undo restores your previous edit.', { autoClose: 3500 });
  return {
    imagePath: response.imagePath,
    editRevision: response.editRevision,
    sourceCopyPath: command.path,
    adopted: true,
    originalSaved,
    changedKeys,
    renderPending,
    copyDeletionConfirmationPending,
    ...(copyDeletionReason ? { copyDeletionReason } : {}),
  };
}
