import { expect, it } from 'vitest';
import { INITIAL_ADJUSTMENTS, INITIAL_MASK_ADJUSTMENTS } from './adjustments';
import { createPointColor } from './pointColor';
import {
  selectedPointColorTarget,
  pointColorNumericValue,
  setPointColorNumericValue,
  removePointColor,
  pointColorCommandId,
  parsePointColorCommandId,
  pointColorScope,
} from './pointColorCommands';
const point = (id: string) => createPointColor({ lightness: 0.7, chroma: 0.1, hue: 30 }, id);
const recipe = () => ({
  ...INITIAL_ADJUSTMENTS,
  pointColor: [point('shared'), point('other')],
  masks: [
    {
      id: 'mask',
      name: 'Local',
      invert: false,
      opacity: 1,
      visible: true,
      subMasks: [],
      adjustments: { ...INITIAL_MASK_ADJUSTMENTS, pointColor: [point('shared')] },
    },
  ],
});
it('matches the panel selection fallback but never falls back from a missing mask to root', () => {
  const before = recipe();
  expect(selectedPointColorTarget(before, null, 'other')).toEqual({ maskId: null, pointId: 'other' });
  expect(selectedPointColorTarget(before, 'mask', 'unavailable')).toEqual({ maskId: 'mask', pointId: 'shared' });
  expect(selectedPointColorTarget(before, 'deleted', 'shared')).toBeNull();
});
it('edits by stable swatch ID after reordering and preserves unrelated swatches and masks', () => {
  const before = recipe();
  const target = selectedPointColorTarget(before, null, 'shared')!;
  const reordered = { ...before, pointColor: [before.pointColor[1], before.pointColor[0]] };
  const after = setPointColorNumericValue(reordered, target, 'hueShift', 70);
  expect(after.pointColor![1].hueShift).toBe(70);
  expect(after.pointColor![0]).toBe(before.pointColor[1]);
  expect(after.masks).toBe(before.masks);
  expect(before.pointColor[0].hueShift).toBe(0);
});
it('scopes edits and removal even when root and mask swatch IDs match', () => {
  const before = recipe();
  const target = selectedPointColorTarget(before, 'mask', 'shared')!;
  const after = setPointColorNumericValue(before, target, 'saturationShift', 25);
  expect(after.masks[0].adjustments.pointColor![0].saturationShift).toBe(25);
  expect(after.pointColor).toBe(before.pointColor);
  const removed = removePointColor(after, target);
  expect(removed.masks[0].adjustments.pointColor).toEqual([]);
  expect(removed.pointColor).toBe(before.pointColor);
});
it('rejects stale targets and nonfinite values and clamps the native slider ranges', () => {
  const before = recipe();
  const target = { maskId: null, pointId: 'shared' };
  expect(setPointColorNumericValue(before, target, 'hueShift', Infinity)).toBe(before);
  expect(setPointColorNumericValue(before, { ...target, pointId: 'deleted' }, 'hueShift', 70)).toBe(before);
  expect(removePointColor(before, { ...target, maskId: 'deleted' })).toBe(before);
  expect(pointColorNumericValue(before, { ...target, maskId: 'deleted' }, 'hueShift')).toBeUndefined();
  const after = setPointColorNumericValue(before, target, 'smoothness', -20);
  expect(pointColorNumericValue(after, target, 'smoothness')).toBe(10);
});

it('keeps mask and point identifiers distinct and rejects malformed numeric command IDs', () => {
  const target = { maskId: 'local:one / %', pointId: 'swatch:雪' };
  expect(parsePointColorCommandId(pointColorCommandId(target, 'hueShift'))).toEqual({ target, key: 'hueShift' });
  expect(parsePointColorCommandId('pointColor:::hueShift')).toBeNull();
  expect(parsePointColorCommandId('pointColor::%broken:hueShift')).toBeNull();
  expect(parsePointColorCommandId('pointColor::point:__proto__')).toBeNull();
});

it('requires an existing selected mask when the mask panel owns the command', () => {
  const before = recipe();
  expect(pointColorScope(before, false, 'mask')).toBeNull();
  expect(pointColorScope(before, true, 'mask')).toBe('mask');
  expect(pointColorScope(before, true, null)).toBeUndefined();
  expect(pointColorScope(before, true, 'deleted')).toBeUndefined();
});
