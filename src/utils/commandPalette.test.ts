import { describe, expect, it } from 'vitest';
import { INITIAL_ADJUSTMENTS, createResetAdjustmentsForImage } from './adjustments';
import { composeParametricCurves } from './presetAmount';
import {
  PALETTE_PARAMETERS,
  getParameterValue,
  parseTypedValue,
  searchScore,
  setParameterValue,
} from './commandPalette';
import { executeAppAction, isAppActionAvailable, registerAppActions } from './appActions';

it('exposes every Basic control with the shared native MCP range', () => {
  for (const id of ['exposure', 'brightness', 'contrast', 'highlights', 'shadows', 'whites', 'blacks']) {
    const entry = PALETTE_PARAMETERS.find((entry) => entry.id === id);
    expect(entry).toBeDefined();
    expect(entry!.minimum).toBeLessThan(0);
    expect(entry!.maximum).toBeGreaterThan(0);
  }
  expect(PALETTE_PARAMETERS.find((entry) => entry.id === 'exposure')).toMatchObject({
    minimum: -5,
    maximum: 5,
    step: 0.01,
  });
});

it('finds controls by Lightroom aliases and fuzzy spelling', () => {
  for (const [query, id] of [
    ['presence', 'clarity'],
    ['texture', 'structure'],
    ['color mixer', 'hsl.oranges.hue'],
    ['hsl', 'hsl.blues.saturation'],
  ]) {
    const entry = PALETTE_PARAMETERS.find((entry) => entry.id === id)!;
    expect(searchScore(query, entry.title, [...entry.aliases, entry.section])).toBeGreaterThan(0);
  }
  expect(searchScore('expsre', 'Exposure')).toBeGreaterThan(0);
  expect(searchScore('Exposure', 'Exposure')).toBeGreaterThan(searchScore('Exposure', 'Reset Exposure'));
});

it('nested changes preserve the original and sibling channels', () => {
  const entry = PALETTE_PARAMETERS.find((entry) => entry.id === 'hsl.oranges.saturation')!;
  const edited = setParameterValue(INITIAL_ADJUSTMENTS, entry, 30);
  expect(getParameterValue(edited, entry.id)).toBe(30);
  expect(getParameterValue(INITIAL_ADJUSTMENTS, entry.id)).toBe(0);
  expect(edited.hsl.blues).toBe(INITIAL_ADJUSTMENTS.hsl.blues);
  expect(edited.hsl.oranges.hue).toBe(0);
  expect(setParameterValue(edited, entry, NaN)).toBe(edited);
  expect(getParameterValue(setParameterValue(edited, entry, 10000), entry.id)).toBe(entry.maximum);
});

it('parametric command-palette edits stay composed with the saved point curve', () => {
  const pointCurve = [
    { x: 0, y: 8 },
    { x: 128, y: 140 },
    { x: 255, y: 255 },
  ];
  const before = {
    ...INITIAL_ADJUSTMENTS,
    curves: { ...INITIAL_ADJUSTMENTS.curves, luma: pointCurve },
    curveMode: 'point' as const,
  };
  const entry = PALETTE_PARAMETERS.find((parameter) => parameter.id === 'parametricCurve.luma.shadows')!;
  const edited = setParameterValue(before, entry, -50);

  expect(edited.curveMode).toBe('parametric');
  expect(edited.pointCurves?.luma).toEqual(pointCurve);
  expect(edited.curves.luma).toHaveLength(16);
  expect(edited.curves.luma).not.toEqual(pointCurve);
});

it('parametric command-palette edits compose a retained Look once in point mode', () => {
  const lookPoints = [
    { x: 0, y: 12 },
    { x: 128, y: 165 },
    { x: 255, y: 255 },
  ];
  const before = {
    ...INITIAL_ADJUSTMENTS,
    curveMode: 'point' as const,
    curves: { ...INITIAL_ADJUSTMENTS.curves, luma: lookPoints },
    pointCurves: INITIAL_ADJUSTMENTS.curves,
    nestedLookCurves: [{ amount: 1, points: lookPoints }],
  };
  const entry = PALETTE_PARAMETERS.find((parameter) => parameter.id === 'parametricCurve.luma.highlights')!;
  const edited = setParameterValue(before, entry, 20);

  expect(edited.curves).toEqual(
    composeParametricCurves(edited.parametricCurve!, before.pointCurves!, before.nestedLookCurves),
  );
  expect(edited.curves.luma.find((point) => point.x === 136)?.y).toBe(176);
});

describe('typed values', () => {
  it.each([
    ['exposure 0.7', { name: 'exposure', value: 0.7, relative: false }],
    ['Orange saturation -12', { name: 'Orange saturation', value: -12, relative: false }],
    ['exposure x+0.3', { name: 'exposure', value: 0.3, relative: true }],
    ['exposure x-0.3', { name: 'exposure', value: -0.3, relative: true }],
  ])('parses %s', (query, result) => {
    expect(parseTypedValue(query)).toEqual(result);
  });
  it.each(['exposure NaN', 'exposure Infinity', 'temperature 5600k', 'crop 4:3', '0.7'])(
    'rejects unsupported numeric input %s',
    (query) => {
      expect(parseTypedValue(query)).toBeNull();
    },
  );
});

it('application actions share guards and dispose safely across re-registration', () => {
  let enabled = false;
  let count = 0;
  const dispose = registerAppActions({
    undo: {
      available: () => enabled,
      execute: () => {
        count++;
      },
    },
  });
  expect(isAppActionAvailable('undo')).toBe(false);
  expect(executeAppAction('undo')).toBe(false);
  enabled = true;
  expect(executeAppAction('undo')).toBe(true);
  expect(count).toBe(1);
  const disposeNext = registerAppActions({
    redo: {
      available: () => true,
      execute: () => {
        count++;
      },
    },
  });
  dispose();
  expect(executeAppAction('redo')).toBe(true);
  expect(executeAppAction('missing')).toBe(false);
  disposeNext();
  expect(executeAppAction('redo')).toBe(false);
});

it('reset keeps the Original ratio sentinel and does not reuse mutable default arrays', () => {
  const reset = createResetAdjustmentsForImage({ width: 6000, height: 4000 });
  expect(reset.aspectRatio).toBe(0);
  expect(reset.exposure).toBe(0);
  expect(reset.aiPatches).toEqual([]);
  expect(reset.aiPatches).not.toBe(INITIAL_ADJUSTMENTS.aiPatches);
  expect(createResetAdjustmentsForImage(null).aspectRatio).toBe(INITIAL_ADJUSTMENTS.aspectRatio);
});
