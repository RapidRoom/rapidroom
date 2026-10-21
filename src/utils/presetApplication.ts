import { create } from 'zustand';
import type { Preset } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { normalizeLoadedAdjustments, type Adjustments } from './adjustments';
import { mixPresetAdjustments } from './presetAmount';
import { presetUnavailableReason } from './presetBrowser';
import {
  rememberAppliedPreset,
  rememberAppliedStyleStack,
  rememberStyleSnapshot,
  styleSnapshot,
  appliedPresetLoading,
} from './appliedPreset';
import { changeStyleStack, keepManualStyleEdits, type StyleStack } from './styleStack';
import { sameAdjustmentValue } from './editHistory';

interface Application {
  path: string;
  preset: Preset;
  definition: string;
  amount: number;
  base: Adjustments;
  result: Adjustments;
  stack?: StyleStack;
  label: string;
  stackChanged?: boolean;
}

export const usePresetApplication = create<{ application: Application | null }>(() => ({ application: null }));

export function activeStyleStack(): StyleStack | null {
  const state = useEditorStore.getState();
  const path = state.selectedImage?.isReady ? state.selectedImage.path : null;
  return path ? (styleSnapshot(path, state.adjustments) ?? null) : null;
}

useEditorStore.subscribe((state, previous) => {
  const active = usePresetApplication.getState().application;
  if (
    active &&
    (state.selectedImage?.path !== active.path ||
      !state.selectedImage?.isReady ||
      state.adjustments !== active.result ||
      state.historyIndex < previous.historyIndex)
  )
    usePresetApplication.setState({ application: null });
  const path = state.selectedImage?.isReady ? state.selectedImage.path : null;
  if (!path || previous.selectedImage?.path !== path || state.adjustments === previous.adjustments) return;
  // resetHistory also initializes a freshly opened photo. Do not erase its
  // sidecar or interpret its controls as edits from the preceding session.
  if (state.history !== previous.history && state.history.length === 1) return;
  const known = styleSnapshot(path, state.adjustments);
  if (known !== undefined) {
    void rememberAppliedStyleStack(path, known, state.adjustments);
    return;
  }
  if (state.history === previous.history && state.historyIndex !== previous.historyIndex) {
    void rememberAppliedStyleStack(path, null, state.adjustments);
    return;
  }
  const stack = styleSnapshot(path, previous.adjustments);
  if (stack?.entries.length)
    void rememberAppliedStyleStack(path, keepManualStyleEdits(stack, state.adjustments), state.adjustments);
});

function stylePlan(change: { preset: Preset; amount: number } | { remove: string }) {
  const editor = useEditorStore.getState();
  const image = editor.selectedImage;
  if (!image?.isReady) throw new Error('Open a ready image before applying a Style');
  if (appliedPresetLoading(image.path)) throw new Error('Please wait for applied Styles to load.');
  const previous = styleSnapshot(image.path, editor.adjustments);
  const old = previous?.entries.length ? previous : null;
  const target = 'remove' in change ? old?.entries.find((entry) => entry.presetId === change.remove) : null;
  const stack = changeStyleStack(old, editor.adjustments, change, image);
  const changed = !sameAdjustmentValue(old?.entries ?? [], stack.entries);
  const result = changed ? { ...stack.result } : editor.adjustments;
  const preset: Preset =
    'preset' in change
      ? change.preset
      : { id: target!.presetId, name: target!.name, adjustments: target!.adjustments, presetType: 'style' };
  const label =
    'remove' in change
      ? `Style ${(old?.entries.indexOf(target!) ?? 0) + 1} removed: ${preset.name}`
      : `Style: ${preset.name} ${change.amount}`;
  const application: Application = {
    path: image.path,
    preset,
    definition: JSON.stringify(preset.adjustments),
    amount: 'amount' in change ? change.amount : target!.amount,
    base: stack.base,
    result,
    stack: { ...stack, result },
    label,
    stackChanged: changed,
  };
  return {
    application,
    current: editor.adjustments,
    isCurrent: () => {
      const now = useEditorStore.getState();
      return (
        now.selectedImage === image &&
        now.adjustments === editor.adjustments &&
        styleSnapshot(image.path, now.adjustments) === previous
      );
    },
  };
}

export function prepareStyleRemoval(presetId: string) {
  return stylePlan({ remove: presetId });
}

export function preparePresetApplication(preset: Preset, amount: number) {
  if (!Number.isInteger(amount) || amount < 0 || amount > 200)
    throw new Error('Amount must be an integer from 0 to 200');
  const editor = useEditorStore.getState();
  const image = editor.selectedImage;
  if (!image?.isReady) throw new Error('Open a ready image before applying a preset');
  const unavailable = presetUnavailableReason(preset, image.exif?.Model);
  if (unavailable) throw new Error(unavailable);
  if (preset.presetType !== 'tool') return stylePlan({ preset, amount });
  const previous = usePresetApplication.getState().application;
  const definition = JSON.stringify({ adjustments: preset.adjustments, presetType: preset.presetType });
  const same =
    previous?.path === image.path && previous.preset.id === preset.id && previous.result === editor.adjustments;
  if (same && previous.definition !== definition)
    throw new Error('Preset definition changed; select another preset or edit before reapplying');
  const base = same ? previous.base : editor.adjustments;
  let result =
    same && previous.amount === amount
      ? previous.result
      : { ...base, ...mixPresetAdjustments(preset, amount, base, image) };
  if (result.masks !== base.masks) {
    // Materialize new preset masks exactly as ordinary image loading does, so
    // saving/opening a copy cannot change its recipe or the original revision.
    const normalized = normalizeLoadedAdjustments(result).masks as Adjustments['masks'];
    result = {
      ...result,
      masks: result.masks.map((mask, index) => (base.masks.includes(mask) ? mask : normalized[index])),
    };
  }
  if (sameAdjustmentValue(result, editor.adjustments)) result = editor.adjustments;
  const application: Application = {
    path: image.path,
    preset,
    definition,
    amount,
    base,
    result,
    label: `Preset: ${preset.name} ${amount}`,
  };
  return {
    application,
    current: editor.adjustments,
    isCurrent: () => {
      const now = useEditorStore.getState();
      return (
        now.selectedImage?.isReady &&
        now.selectedImage.path === image.path &&
        now.adjustments === editor.adjustments &&
        usePresetApplication.getState().application === previous
      );
    },
  };
}

export function commitPresetApplication(
  plan: ReturnType<typeof preparePresetApplication>,
  apply: (next: Adjustments) => void,
): void {
  if (!plan.isCurrent())
    throw new Error('Active image or edit changed before preset application; read the current revision and retry');
  if (plan.application.stack) {
    const path = plan.application.path;
    if (styleSnapshot(path, plan.current) === undefined) rememberStyleSnapshot(path, plan.current, null);
    if (plan.application.stackChanged) rememberStyleSnapshot(path, plan.application.result, plan.application.stack);
  }
  usePresetApplication.setState({ application: plan.application });
  try {
    if (plan.application.result !== plan.current) apply(plan.application.result);
  } catch (error) {
    usePresetApplication.setState({ application: null });
    throw error;
  }
  const stack = styleSnapshot(plan.application.path, plan.application.result);
  if (stack) void rememberAppliedStyleStack(plan.application.path, stack, plan.application.result);
  else
    void rememberAppliedPreset(
      plan.application.path,
      plan.application.preset,
      plan.application.amount,
      plan.application.result,
    );
}
