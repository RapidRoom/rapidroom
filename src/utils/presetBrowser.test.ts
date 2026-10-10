import { describe, expect, it } from 'vitest';
import { filterPresetLibrary, presetUnavailableReason } from './presetBrowser';
import { mixAdjustments } from './presetAmount';
import { INITIAL_ADJUSTMENTS } from './adjustments';
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
