import { create } from 'zustand';
import type { Preset } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import type { Adjustments } from './adjustments';
import { mixPresetAdjustments } from './presetAmount';
import { presetUnavailableReason } from './presetBrowser';
import { rememberAppliedPreset } from './appliedPreset';

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
  const definition = JSON.stringify(preset);
  const same =
    previous?.path === image.path && previous.preset.id === preset.id && previous.result === editor.adjustments;
  if (same && previous.definition !== definition)
    throw new Error('Preset definition changed; select another preset or edit before reapplying');
  const base = same ? previous.base : editor.adjustments;
  const result =
    same && previous.amount === amount
      ? previous.result
      : { ...base, ...mixPresetAdjustments(preset, amount, base, image) };
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
    apply(plan.application.result);
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
