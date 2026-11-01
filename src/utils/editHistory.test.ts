import { beforeEach, describe, expect, it } from 'vitest';
import { useEditorStore } from '../store/useEditorStore';
import { INITIAL_ADJUSTMENTS, INITIAL_MASK_ADJUSTMENTS, normalizeLoadedAdjustments } from './adjustments';
import { describeHistoryChange, sameAdjustmentValue, historyMenuPlacement } from './editHistory';

beforeEach(() => useEditorStore.getState().resetHistory(INITIAL_ADJUSTMENTS));

describe('labelled editor history', () => {
  it('records explicit action labels with the real changed keys', () => {
    for (const label of ['Style: Teal & Orange 60', 'Paste adjustments', 'Reset adjustments', 'Auto adjustments']) {
      useEditorStore.getState().pushHistory({ ...INITIAL_ADJUSTMENTS, exposure: 0.35 }, label);
      expect(useEditorStore.getState().historyDetails.at(-1)).toMatchObject({
        label,
        changedKeys: ['exposure'],
        actor: 'user',
      });
      useEditorStore.getState().resetHistory(INITIAL_ADJUSTMENTS);
    }
  });

  it('records values and names for sliders, crop, curves, HSL and local masks', () => {
    const base = INITIAL_ADJUSTMENTS;
    expect(describeHistoryChange({ ...base, exposure: 1 }, { ...base, exposure: 0.35 }, 'user').label).toBe(
      'Exposure +1 → +0.35',
    );
    expect(describeHistoryChange(base, { ...base, contrast: -12 }, 'user').label).toBe('Contrast 0 → -12');
    expect(describeHistoryChange(base, { ...base, highlights: 25 }, 'user').label).toBe('Highlights 0 → +25');
    expect(
      describeHistoryChange(
        base,
        { ...base, colorGrading: { ...base.colorGrading, shadows: { ...base.colorGrading.shadows, saturation: 18 } } },
        'user',
      ).label,
    ).toBe('Color Grading: Shadows: Saturation 0 → +18');
    expect(describeHistoryChange(base, { ...base, flipHorizontal: true }, 'user').label).toBe('Flip Horizontal on');
    const many = describeHistoryChange(
      base,
      { ...base, exposure: 1, contrast: 2, saturation: 3, highlights: 4, shadows: 5, blacks: 6, whites: 7 },
      'assistant',
    );
    expect(many.label).toMatch(/^AI: .* \(\+1 more\)$/);

    expect(
      describeHistoryChange(
        base,
        { ...base, aspectRatio: 0.75, crop: { x: 100, y: 0, width: 300, height: 400, unit: 'px' } },
        'user',
      ).label,
    ).toBe('Crop 3:4');
    expect(
      describeHistoryChange(
        base,
        {
          ...base,
          pointCurves: {
            ...base.curves,
            luma: [
              { x: 0, y: 2 },
              { x: 255, y: 255 },
            ],
          },
        },
        'user',
      ).label,
    ).toBe('Tone curve (Luma)');
    const hsl = { ...base.hsl, oranges: { ...base.hsl.oranges, saturation: 15 } };
    expect(describeHistoryChange(base, { ...base, hsl }, 'user').label).toBe('HSL: Orange: Saturation 0 → +15');
    const masks = [
      {
        id: 'face',
        name: 'Face & neck',
        visible: true,
        invert: false,
        opacity: 100,
        adjustments: { ...INITIAL_MASK_ADJUSTMENTS, exposure: 0 },
        subMasks: [],
      },
    ];
    const previous = normalizeLoadedAdjustments({ ...base, masks });
    const next = normalizeLoadedAdjustments({
      ...base,
      masks: [{ ...masks[0], adjustments: { ...INITIAL_MASK_ADJUSTMENTS, exposure: 0.4 } }],
    });
    expect(describeHistoryChange(previous, next, 'assistant').label).toBe('AI: Face & neck: Exposure 0 → +0.4');
  });

  it('keeps a readable history menu inside narrow and bottom-edge viewports', () => {
    for (const viewport of [
      [1280, 800],
      [320, 400],
    ]) {
      const [width, height] = viewport;
      for (const left of [0, width - 50]) {
        const placed = historyMenuPlacement(
          { left, right: left + 50, top: height - 30, bottom: height },
          width,
          height,
        );
        expect(placed.left).toBeGreaterThanOrEqual(8);
        expect(placed.left + placed.width).toBeLessThanOrEqual(width - 8);
        expect(placed.top).toBeGreaterThanOrEqual(8);
        expect(placed.top + placed.maxHeight).toBeLessThanOrEqual(height - 8);
      }
    }
  });

  it('ignores Rust JSON key order while retaining nested and array changes', () => {
    const reordered = JSON.parse(
      JSON.stringify(INITIAL_ADJUSTMENTS, (key, value) =>
        value && typeof value === 'object' && !Array.isArray(value)
          ? Object.fromEntries(Object.entries(value).reverse())
          : value,
      ),
    );
    expect(sameAdjustmentValue(INITIAL_ADJUSTMENTS, reordered)).toBe(true);
    expect(
      describeHistoryChange(INITIAL_ADJUSTMENTS, { ...reordered, exposure: 0.2 }, 'assistant').changedKeys,
    ).toEqual(['exposure']);
    expect(sameAdjustmentValue({ points: [1, 2] }, { points: [2, 1] })).toBe(false);
    expect(sameAdjustmentValue({ nested: { hue: 0 } }, { nested: { hue: 1 } })).toBe(false);
  });

  it('retains AI labels across undo/redo and drops the abandoned redo branch', () => {
    const first = { ...INITIAL_ADJUSTMENTS, exposure: 0.4, highlights: -30 };
    const second = { ...first, contrast: 10 };
    const details = describeHistoryChange(INITIAL_ADJUSTMENTS, first, 'assistant');
    expect(details.label).toBe('AI: Exposure 0 → +0.4, Highlights 0 → -30');
    const store = useEditorStore.getState();
    store.pushHistory(first, details);
    store.pushHistory(second, describeHistoryChange(first, second, 'assistant'));
    store.undo();
    expect(useEditorStore.getState().adjustments).toEqual(first);
    store.redo();
    expect(useEditorStore.getState().historyDetails[1]).toMatchObject(details);
    store.undo();
    store.pushHistory({ ...first, contrast: 20 });
    expect(useEditorStore.getState().historyDetails[0]).toMatchObject({ actor: 'system', label: 'Initial state' });
    expect(useEditorStore.getState().historyDetails[1]).toMatchObject(details);
    expect(useEditorStore.getState().historyDetails[2]).toMatchObject({
      actor: 'user',
      label: 'Contrast 0 → +20',
      changedKeys: ['contrast'],
    });
    expect(useEditorStore.getState().history).toHaveLength(3);
  });

  it('keeps metadata aligned when old history is trimmed, and clears it on photo reset', () => {
    for (let index = 1; index <= 60; index++) {
      const next = { ...INITIAL_ADJUSTMENTS, exposure: index / 100 };
      const store = useEditorStore.getState();
      store.pushHistory(next, describeHistoryChange(store.history.at(-1)!, next, 'assistant'));
    }
    const store = useEditorStore.getState();
    expect(store.history).toHaveLength(50);
    expect(store.historyDetails).toHaveLength(50);
    expect(store.history[0].exposure).toBe(INITIAL_ADJUSTMENTS.exposure);
    expect(store.history[1].exposure).toBe(0.12);
    expect(store.historyDetails.slice(1).every((details) => details?.changedKeys.includes('exposure'))).toBe(true);
    store.resetHistory(INITIAL_ADJUSTMENTS);
    expect(useEditorStore.getState().historyDetails).toMatchObject([{ actor: 'system', label: 'Initial state' }]);
  });
});
