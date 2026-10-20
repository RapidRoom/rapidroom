import { create } from 'zustand';
import type { Preset } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { normalizeLoadedAdjustments, type Adjustments } from './adjustments';
import { mixPresetAdjustments } from './presetAmount';
import { presetUnavailableReason } from './presetBrowser';
import { rememberAppliedPreset } from './appliedPreset';
import { sameAdjustmentValue } from './editHistory';

interface Application {
  path: string;
  preset: Preset;
  definition: string;
  amount: number;
  base: Adjustments;
  result: Adjustments;
}

export const usePresetApplication = create<{ application: Application | null }>(() => ({ application: null }));

// Independent of the panel's lifetime. Undo, image switches and unrelated edits
// invalidate the Amount baseline; the persisted, hash-checked indicator is separate.
useEditorStore.subscribe((state, previous) => {
  const active = usePresetApplication.getState().application;
  if (
    active &&
    (state.selectedImage?.path !== active.path ||
      !state.selectedImage.isReady ||
      state.adjustments !== active.result ||
      state.historyIndex < previous.historyIndex)
  )
    usePresetApplication.setState({ application: null });
});

export function preparePresetApplication(preset: Preset, amount: number) {
  if (!Number.isInteger(amount) || amount < 0 || amount > 200)
    throw new Error('Amount must be an integer from 0 to 200');
  const editor = useEditorStore.getState();
  const image = editor.selectedImage;
  if (!image?.isReady) throw new Error('Open a ready image before applying a preset');
  const unavailable = presetUnavailableReason(preset, image.exif?.Model);
  if (unavailable) throw new Error(unavailable);
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
  const application: Application = { path: image.path, preset, definition, amount, base, result };
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
  usePresetApplication.setState({ application: plan.application });
  try {
    if (plan.application.result !== plan.current) apply(plan.application.result);
  } catch (error) {
    usePresetApplication.setState({ application: null });
    throw error;
  }
  void rememberAppliedPreset(
    plan.application.path,
    plan.application.preset,
    plan.application.amount,
    plan.application.result,
  );
}
