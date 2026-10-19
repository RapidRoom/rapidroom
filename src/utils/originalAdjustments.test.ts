import { describe, expect, it } from 'vitest';
import { INITIAL_ADJUSTMENTS } from './adjustments';
import { originalComparisonAdjustments } from './originalAdjustments';

describe('originalComparisonAdjustments', () => {
  it('keeps current geometry and fresh/legacy camera defaults while neutralizing edits', () => {
    const current = {
      ...INITIAL_ADJUSTMENTS,
      exposure: 1.2,
      contrast: 25,
      crop: { unit: '%', x: 5, y: 10, width: 80, height: 75 },
      rotation: 3,
      lensCorrectionMode: 'auto' as const,
      baselineExposure: true,
      cameraExposureCompensation: true,
    };
    const original = originalComparisonAdjustments(current);
    expect(original).toMatchObject({
      exposure: INITIAL_ADJUSTMENTS.exposure,
      contrast: INITIAL_ADJUSTMENTS.contrast,
      crop: current.crop,
      rotation: 3,
      baselineExposure: true,
      cameraExposureCompensation: true,
    });
  });
});
