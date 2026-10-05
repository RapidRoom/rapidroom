import * as prettier from 'prettier';
import { describe, expect, it } from 'vitest';
import basicSource from '../components/adjustments/Basic.tsx?raw';
import colorSource from '../components/adjustments/Color.tsx?raw';
import curvesSource from '../components/adjustments/Curves.tsx?raw';
import detailsSource from '../components/adjustments/Details.tsx?raw';
import effectsSource from '../components/adjustments/Effects.tsx?raw';
import cropPanelSource from '../components/panel/right/CropPanel.tsx?raw';
import masksPanelSource from '../components/panel/right/MasksPanel.tsx?raw';
import lutControlSource from '../components/ui/LUTControl.tsx?raw';
import enMessages from '../i18n/locales/en.json';
import { buildAdjustmentReference, collectSliderRanges, parseSliderProps, sliderPath } from './adjustmentReference';
import { ADJUSTMENT_NOTES, MASK_TYPE_NOTES } from './adjustmentReferenceNotes';
import { Mask } from '../components/panel/right/Masks';

// The reference ships with the RapidRoom skill. Paths are relative to this file for the
// snapshot and to the repository root (Vitest's working directory) for Prettier.
const REFERENCE = 'rapidroom/plugin/skills/rapidroom/references/adjustments.md';

// Every component whose sliders edit adjustments.
const SLIDER_SOURCES = {
  'adjustments/Basic.tsx': basicSource,
  'adjustments/Color.tsx': colorSource,
  'adjustments/Curves.tsx': curvesSource,
  'adjustments/Details.tsx': detailsSource,
  'adjustments/Effects.tsx': effectsSource,
  'panel/right/CropPanel.tsx': cropPanelSource,
  'ui/LUTControl.tsx': lutControlSource,
};

describe('slider parsing', () => {
  it('documents all three properties of the tabbed HSL mixer', () => {
    const result = collectSliderRanges({
      color: '<Slider min={-100} max={100} step={1} value={hsl[color][property]} />',
    });
    expect(result.unmapped).toEqual([]);
    for (const property of ['hue', 'saturation', 'luminance']) {
      expect(result.ranges[`hsl.<band>.${property}`]).toEqual({ min: -100, max: 100, step: 1 });
    }
  });

  it('reads props, including multi-line expressions', () => {
    const [props] = parseSliderProps(`<Slider
      label={t('x')}
      max={100}
      min={isForMask ? -100 : 0}
      onChange={(e: any) => handle({ a: 1 }, e.target.value)}
      value={adjustments.clarity ?? 0}
      trackClassName="bg-surface"
    />`);
    expect(props).toMatchObject({ max: '100', min: 'isForMask ? -100 : 0', trackClassName: 'bg-surface' });
    expect(sliderPath(props.value)).toBe('clarity');
  });
});

describe('adjustment reference for the RapidRoom skill', () => {
  const { markdown, unmapped, rows } = buildAdjustmentReference({
    sliderSources: SLIDER_SOURCES,
    maskPanelSource: masksPanelSource,
    enMessages,
  });

  it('maps every editor slider to an adjustment key', () => {
    expect(unmapped).toEqual([]);
  });

  it('has a note for every key, and no notes for keys that are gone', () => {
    const paths = rows.map((row) => row.path);
    expect(paths.filter((path) => !ADJUSTMENT_NOTES[path])).toEqual([]);
    expect(Object.keys(ADJUSTMENT_NOTES).filter((path) => !paths.includes(path))).toEqual([]);
    expect(Object.values(Mask).filter((type) => !MASK_TYPE_NOTES[type])).toEqual([]);
  });

  // Fails when the committed reference drifts from INITIAL_ADJUSTMENTS, the sliders or the
  // notes. Regenerate with: npx vitest run src/utils/adjustmentReference.test.ts -u
  it('matches the committed reference', async () => {
    const options = await prettier.resolveConfig(REFERENCE);
    const formatted = await prettier.format(markdown, { ...options, parser: 'markdown' });
    await expect(formatted).toMatchFileSnapshot(`../../${REFERENCE}`);
  });
});
