import { describe, expect, it } from 'vitest';
import { filterPresetLibrary, presetUnavailableReason } from './presetBrowser';
import { mixAdjustments, presetCurveY } from './presetAmount';
import {
  DEFAULT_PARAMETRIC_CURVE_SETTINGS,
  INITIAL_ADJUSTMENTS,
  buildParametricCurvePoints,
  type Adjustments,
} from './adjustments';
import type { Preset } from '../components/ui/AppProperties';
const preset = (name: string, extra: Partial<Preset> = {}): Preset => ({ id: name, name, adjustments: {}, ...extra });

describe('preset browser', () => {
  it('searches group names and preset names without changing the library', () => {
    const items = [
      { folder: { id: 'color', name: 'Color', children: [preset('Warm'), preset('Cool')] } },
      { preset: preset('Neutral') },
    ];
    expect(filterPresetLibrary(items, 'color', false)).toEqual([items[0]]);
    expect(filterPresetLibrary(items, 'warm', false)[0].folder?.children.map((p: Preset) => p.name)).toEqual(['Warm']);
    expect(items[0].folder?.children).toHaveLength(2);
    expect(filterPresetLibrary(items, 'missing', false)).toEqual([]);
  });
  it('keeps unavailable entries visible until compatible-only is selected', () => {
    const items = [
      { preset: preset('Sony', { cameraModelRestriction: 'ILCE-7C' }) },
      { preset: preset('Profile', { unavailableReason: 'Requires camera profile: Adobe Standard' }) },
    ];
    expect(filterPresetLibrary(items, '', false)).toHaveLength(2);
    expect(filterPresetLibrary(items, '', true, 'ilce-7c')).toEqual([items[0]]);
    expect(presetUnavailableReason(items[0].preset, 'Other')).toContain('ILCE-7C');
    expect(presetUnavailableReason(items[1].preset)).toContain('Adobe Standard');
  });
});
describe('preset Amount', () => {
  it('keeps parametric preset fields sparse and composes them over the point curves', () => {
    const pointCurves = {
      ...INITIAL_ADJUSTMENTS.curves,
      luma: [
        { x: 0, y: 8 },
        { x: 128, y: 140 },
        { x: 255, y: 255 },
      ],
    };
    const before: Adjustments = {
      ...INITIAL_ADJUSTMENTS,
      curves: pointCurves,
      curveMode: 'point',
      parametricCurve: {
        luma: { ...DEFAULT_PARAMETRIC_CURVE_SETTINGS, darks: 17 },
        red: { ...DEFAULT_PARAMETRIC_CURVE_SETTINGS },
        green: { ...DEFAULT_PARAMETRIC_CURVE_SETTINGS },
        blue: { ...DEFAULT_PARAMETRIC_CURVE_SETTINGS },
      },
    };
    const presetPatch: Partial<Adjustments> = {
      curveMode: 'parametric',
      parametricCurve: { luma: { shadows: -50 } } as unknown as Adjustments['parametricCurve'],
      pointCurves: { luma: pointCurves.luma } as unknown as Adjustments['pointCurves'],
    };

    const result = mixAdjustments(presetPatch, 100, before);
    expect(result.curveMode).toBe('parametric');
    expect(result.parametricCurve?.luma.shadows).toBe(-50);
    expect(result.parametricCurve?.luma.darks).toBe(17);
    expect(result.pointCurves?.luma).toEqual(pointCurves.luma);
    expect(result.curves?.luma).toHaveLength(16);
    expect(result.curves?.luma).not.toEqual(pointCurves.luma);
    const sample = result.curves?.luma.find((point) => point.x === 136);
    const parametric = buildParametricCurvePoints({
      ...DEFAULT_PARAMETRIC_CURVE_SETTINGS,
      shadows: -50,
      darks: 17,
    });
    expect(sample?.y).toBe(Math.round(presetCurveY(pointCurves.luma, presetCurveY(parametric, 136))));
    expect(result.curves?.red).toEqual(before.curves.red);
  });

  it('keeps an explicit zero parametric curve exactly equal to its point curve', () => {
    const before: Adjustments = { ...INITIAL_ADJUSTMENTS, curveMode: 'point' };
    const presetPatch: Partial<Adjustments> = {
      curveMode: 'parametric',
      parametricCurve: { luma: { shadows: 0 } } as unknown as Adjustments['parametricCurve'],
      pointCurves: { luma: before.curves.luma } as unknown as Adjustments['pointCurves'],
    };
    const result = mixAdjustments(presetPatch, 100, before);

    expect(result.curves?.luma).toEqual(before.curves.luma);
  });

  it('scales from before the preset and preserves omitted/nested settings', () => {
    const before = {
      ...INITIAL_ADJUSTMENTS,
      exposure: 1,
      contrast: 7,
      hsl: { ...INITIAL_ADJUSTMENTS.hsl, reds: { hue: 5, saturation: 12, luminance: 8 } },
    };
    const patch = { exposure: 2, hsl: { reds: { hue: 15 } } } as Partial<typeof before>;
    expect(mixAdjustments(patch, 0, before)).toEqual(before);
    const half = mixAdjustments(patch, 50, before);
    expect(half.exposure).toBe(1.5);
    expect(half.contrast).toBe(7);
    expect(half.hsl?.reds).toEqual({ hue: 10, saturation: 12, luminance: 8 });
    expect(mixAdjustments(patch, 200, before).exposure).toBe(3);
    expect(mixAdjustments(patch, 50, before)).toEqual(half);
    expect(before.exposure).toBe(1);
  });
  it('respects control bounds at 200% and interpolates union curve knots', () => {
    const before = { ...INITIAL_ADJUSTMENTS, exposure: 4 };
    expect(mixAdjustments({ exposure: 5 }, 200, before).exposure).toBe(5);
    const patch = {
      curves: {
        ...before.curves,
        luma: [
          { x: 0, y: 0 },
          { x: 128, y: 200 },
          { x: 255, y: 255 },
        ],
      },
    };
    expect(mixAdjustments(patch, 50, before).curves?.luma).toContainEqual({ x: 128, y: 164 });
    expect(mixAdjustments(patch, 200, before).curves?.luma.every((p) => p.y >= 0 && p.y <= 255)).toBe(true);
  });
});

describe('edited preset curves', () => {
  const target = [
    { x: 0, y: 0 },
    { x: 128, y: 200 },
    { x: 255, y: 255 },
  ];
  it('preserves exact endpoint curves independently of prior knots', () => {
    const before = {
      ...INITIAL_ADJUSTMENTS,
      curves: {
        ...INITIAL_ADJUSTMENTS.curves,
        luma: [
          { x: 0, y: 0 },
          { x: 64, y: 25 },
          { x: 255, y: 255 },
        ],
      },
    };
    const patch = { curves: { ...before.curves, luma: target } };
    expect(mixAdjustments(patch, 100, before).curves?.luma).toEqual(target);
    expect(mixAdjustments(patch, 0, before)).toBe(before);
    expect(presetCurveY(target, 64)).toBeCloseTo(109.035433, 5);
    const half = mixAdjustments(patch, 50, before).curves!.luma;
    expect(half.find((p) => p.x === 64)?.y).toBeCloseTo((25 + 109.035433) / 2, 5);
  });
  it('bounds two sixteen-point curves to sixteen points at intermediate Amount', () => {
    const source = Array.from({ length: 16 }, (_, i) => ({ x: i * 17, y: i * 17 }));
    const target = source.map((p, i) => ({ ...p, x: i === 0 || i === 15 ? p.x : p.x + 1 }));
    const before = { ...INITIAL_ADJUSTMENTS, curves: { ...INITIAL_ADJUSTMENTS.curves, luma: source } };
    const curve = mixAdjustments({ curves: { ...before.curves, luma: target } }, 50, before).curves!.luma;
    expect(curve).toHaveLength(16);
    expect(curve[0]).toEqual({ x: 0, y: 0 });
    expect(curve.at(-1)).toEqual({ x: 255, y: 255 });
  });
});
