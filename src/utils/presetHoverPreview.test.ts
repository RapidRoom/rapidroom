import { afterEach, describe, expect, it } from 'vitest';
import { invoke } from '../test/tauriMock';
import { useEditorStore } from '../store/useEditorStore';
import { INITIAL_ADJUSTMENTS } from './adjustments';
import { clearPresetHoverPreview, showPresetHoverPreview } from './presetHoverPreview';

const initialEditorState = useEditorStore.getState();

afterEach(() => {
  clearPresetHoverPreview();
  useEditorStore.setState(initialEditorState, true);
});

describe('preset hover preview', () => {
  it('previews through previewOverride and restores the edit on clear', () => {
    const adjustments = { ...INITIAL_ADJUSTMENTS, exposure: 0.5 };
    const history = useEditorStore.getState().history;
    useEditorStore.setState({ adjustments });

    expect(showPresetHoverPreview({ contrast: 30 })).toBe(true);
    expect(useEditorStore.getState().previewOverride).toMatchObject({ exposure: 0.5, contrast: 30 });
    expect(useEditorStore.getState().adjustments).toBe(adjustments);

    clearPresetHoverPreview();
    expect(useEditorStore.getState().previewOverride).toBeNull();
    expect(useEditorStore.getState().adjustments).toBe(adjustments);
    expect(useEditorStore.getState().history).toBe(history);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('replaces the preview when hovering from one preset to the next', () => {
    showPresetHoverPreview({ contrast: 30 });
    showPresetHoverPreview({ contrast: -20 });
    expect(useEditorStore.getState().previewOverride).toMatchObject({ contrast: -20 });

    clearPresetHoverPreview();
    expect(useEditorStore.getState().previewOverride).toBeNull();
  });

  it('leaves a preview it does not own alone', () => {
    const lutPreview = { ...INITIAL_ADJUSTMENTS, lutPath: '/luts/film.cube' };
    useEditorStore.setState({ previewOverride: lutPreview });

    expect(showPresetHoverPreview({ contrast: 30 })).toBe(false);
    clearPresetHoverPreview();
    expect(useEditorStore.getState().previewOverride).toBe(lutPreview);
  });

  it('does not clear a preview that replaced it, such as the compare view', () => {
    showPresetHoverPreview({ contrast: 30 });
    const original = { ...INITIAL_ADJUSTMENTS };
    useEditorStore.setState({ showOriginal: true, previewOverride: original });

    clearPresetHoverPreview();
    expect(useEditorStore.getState().previewOverride).toBe(original);
  });

  it('does not preview while the original is shown or a slider is dragged', () => {
    useEditorStore.setState({ showOriginal: true });
    expect(showPresetHoverPreview({ contrast: 30 })).toBe(false);

    useEditorStore.setState({ showOriginal: false, isSliderDragging: true });
    expect(showPresetHoverPreview({ contrast: 30 })).toBe(false);
    expect(useEditorStore.getState().previewOverride).toBeNull();
  });

  it('is a no-op after the image changed and navigation reset the preview', () => {
    showPresetHoverPreview({ contrast: 30 });
    useEditorStore.setState({ previewOverride: null });

    clearPresetHoverPreview();
    expect(useEditorStore.getState().previewOverride).toBeNull();
    expect(showPresetHoverPreview({ contrast: 10 })).toBe(true);
  });
});
