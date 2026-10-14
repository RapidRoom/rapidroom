import { describe, expect, it } from 'vitest';
import {
  INITIAL_ADJUSTMENTS,
  INITIAL_MASK_ADJUSTMENTS,
  normalizeLoadedAdjustments,
  type Adjustments,
} from './adjustments';

const normalize = (value: unknown) => normalizeLoadedAdjustments(value as Adjustments) as Adjustments;

describe('stored partial adjustment defaults', () => {
  it('fills nested mask HSL and grading without losing explicit edits', () => {
    const loaded = {
      masks: [
        {
          id: 'local',
          subMasks: [],
          adjustments: {
            exposure: 0.5,
            hsl: { yellows: { hue: 25, saturation: -20 } },
            colorGrading: { shadows: { hue: 60 }, global: { saturation: 0 }, balance: -12 },
            parametricCurve: { red: { shadows: 15 } },
          },
        },
      ],
    };
    const original = JSON.stringify(loaded);
    const mask = normalize(loaded).masks[0].adjustments;
    expect(mask.hsl.yellows).toEqual({ hue: 25, saturation: -20, luminance: 0 });
    expect(mask.hsl.blues).toEqual(INITIAL_MASK_ADJUSTMENTS.hsl.blues);
    expect(mask.colorGrading.shadows).toEqual({ hue: 60, saturation: 0, luminance: 0 });
    expect(mask.colorGrading.global).toEqual({ hue: 0, saturation: 0, luminance: 0 });
    expect(mask.colorGrading.balance).toBe(-12);
    expect(mask.parametricCurve?.red).toMatchObject({ shadows: 15, split1: 25, split2: 50, split3: 75 });
    expect(mask.exposure).toBe(0.5);
    expect(JSON.stringify(loaded)).toBe(original);
  });

  it('fills the same missing band and wheel keys globally', () => {
    const global = normalize({ hsl: { yellows: { hue: 25 } }, colorGrading: { highlights: { luminance: -8 } } });
    expect(global.hsl.yellows).toEqual({ hue: 25, saturation: 0, luminance: 0 });
    expect(global.colorGrading.highlights).toEqual({ hue: 0, saturation: 0, luminance: -8 });
  });

  it('keeps default objects independent across masks, globals and later loads', () => {
    const result = normalize({ masks: [{ id: 'one' }, { id: 'two' }] });
    result.masks[0].adjustments.hsl.yellows.hue = 99;
    result.masks[0].adjustments.colorGrading.shadows.saturation = 99;
    result.hsl.reds.luminance = -99;
    expect(result.masks[1].adjustments.hsl.yellows.hue).toBe(0);
    expect(result.masks[1].adjustments.colorGrading.shadows.saturation).toBe(0);
    expect(INITIAL_MASK_ADJUSTMENTS.hsl.yellows.hue).toBe(0);
    expect(INITIAL_ADJUSTMENTS.hsl.reds.luminance).toBe(0);
    expect(normalize({}).colorGrading.shadows.saturation).toBe(0);
  });

  it('preserves stored curve and sub-mask arrays instead of filling or merging array entries', () => {
    const points = [
      { x: 0, y: 10 },
      { x: 255, y: 240 },
    ];
    const result = normalize({
      masks: [
        {
          id: 'one',
          adjustments: { curves: { red: points } },
          subMasks: [{ id: 'r', type: 'radial', mode: 'intersect', parameters: { centerX: 20 } }],
        },
      ],
    });
    expect(result.masks).toHaveLength(1);
    expect(result.masks[0].adjustments.curves.red).toEqual(points);
    expect(result.masks[0].adjustments.curves.red).not.toBe(points);
    expect(result.masks[0].subMasks).toHaveLength(1);
    expect(result.masks[0].subMasks[0]).toMatchObject({ id: 'r', mode: 'intersect', parameters: { centerX: 20 } });
  });

  it('uses curves as canonical for legacy point mode with a stale default pointCurves field', () => {
    const edited = [
      { x: 0, y: 8 },
      { x: 128, y: 140 },
      { x: 255, y: 255 },
    ];
    const result = normalize({
      curveMode: 'point',
      curves: { ...INITIAL_ADJUSTMENTS.curves, luma: edited },
      pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
    });
    expect(result.pointCurves.luma).toEqual(edited);
  });

  it('preserves raw pointCurves when a point-mode Look is retained', () => {
    const look = [
      { x: 0, y: 12 },
      { x: 128, y: 165 },
      { x: 255, y: 255 },
    ];
    const raw = INITIAL_ADJUSTMENTS.pointCurves;
    const result = normalize({
      curveMode: 'point',
      curves: { ...raw, luma: look },
      pointCurves: raw,
      nestedLookCurves: [{ amount: 1, points: look }],
    });
    expect(result.pointCurves).toEqual(raw);
    expect(result.curves.luma).toEqual(look);
  });

  it('uses mask curves as canonical for legacy point mode', () => {
    const edited = [
      { x: 0, y: 8 },
      { x: 128, y: 140 },
      { x: 255, y: 255 },
    ];
    const result = normalize({
      masks: [
        {
          id: 'legacy',
          adjustments: {
            curveMode: 'point',
            curves: { ...INITIAL_ADJUSTMENTS.curves, luma: edited },
            pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
          },
        },
      ],
    });
    expect(result.masks[0].adjustments.pointCurves.luma).toEqual(edited);
  });
});
