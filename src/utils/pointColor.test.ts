import { expect, it } from 'vitest';
import { createPointColor, normalizePointColors } from './pointColor';
import { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from './adjustments';
it('new swatches and old recipes have zero shifts', () => {
  expect(createPointColor({ lightness: 0.7, chroma: 0.1, hue: 30 }, 'skin')).toMatchObject({
    hueShift: 0,
    saturationShift: 0,
    luminanceShift: 0,
  });
  expect(normalizePointColors(undefined)).toEqual([]);
  expect(normalizeLoadedAdjustments(INITIAL_ADJUSTMENTS).pointColor).toEqual([]);
});
it('normalizes bounded values, preserves unknown saved fields and limits swatches', () => {
  const point = {
    ...createPointColor({ lightness: 0.7, chroma: 0.1, hue: 390 }, 'skin'),
    hueShift: 1000,
    smoothness: 0,
    futureProperty: 'retained',
  };
  const loaded = normalizePointColors(Array.from({ length: 10 }, (_, index) => ({ ...point, id: `skin${index}` })));
  expect(loaded).toHaveLength(8);
  expect(loaded[0]).toMatchObject({ hueShift: 100, smoothness: 10, futureProperty: 'retained', color: { hue: 30 } });
  expect(point.color.hue).toBe(390);
});
it('drops malformed and nonfinite color centers', () => {
  expect(normalizePointColors([null, {}, { id: 'a', color: { lightness: NaN, chroma: 0.1, hue: 20 } }])).toEqual([]);
});
it('round-trips an edited swatch through normal adjustment loading', () => {
  const point = {
    ...createPointColor({ lightness: 0.7, chroma: 0.1, hue: 30 }, 'skin'),
    saturationShift: 25,
    picked: { x: 0.4, y: 0.3 },
  };
  const recipe = { ...INITIAL_ADJUSTMENTS, pointColor: [point] };
  expect(normalizeLoadedAdjustments(JSON.parse(JSON.stringify(recipe))).pointColor).toEqual([point]);
});
