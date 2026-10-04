import { useEditorStore } from '../store/useEditorStore';
import { Adjustments } from './adjustments';

// A preset hover preview only ever sets previewOverride: adjustments, history and sidecars stay untouched.
// It remembers the override it set, so it never clears one owned by the compare view or a LUT preview.
let hoverOverride: Adjustments | null = null;

export function showPresetHoverPreview(presetAdjustments: Partial<Adjustments>): boolean {
  const { adjustments, previewOverride, showOriginal, isSliderDragging, setEditor } = useEditorStore.getState();
  if (showOriginal || isSliderDragging || (previewOverride && previewOverride !== hoverOverride)) {
    return false;
  }
  const override = { ...adjustments, ...presetAdjustments };
  hoverOverride = override;
  setEditor({ previewOverride: override });
  return true;
}

export function clearPresetHoverPreview() {
  const override = hoverOverride;
  if (!override) return;
  hoverOverride = null;
  const { previewOverride, setEditor } = useEditorStore.getState();
  if (previewOverride === override) setEditor({ previewOverride: null });
}
