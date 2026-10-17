import { describe, expect, it } from 'vitest';
import { createResetAdjustmentsForImage } from './adjustments';
import { assessEdit } from './editAssessment';

const image = { width: 6000, height: 4000 };
const reset = createResetAdjustmentsForImage(image);
describe('edit protection assessment', () => {
  it('edits an untouched or simply cropped/basic image in place, without treating history length as complexity', () => {
    expect(assessEdit(reset, image, 1).recommendVirtualCopy).toBe(false);
    const simple = {
      ...reset,
      crop: { x: 500, y: 0, width: 3000, height: 4000, unit: 'px' as const },
      exposure: 0.6,
      highlights: -20,
      shadows: 15,
    };
    expect(assessEdit(simple, image, 40)).toMatchObject({
      recommendVirtualCopy: false,
      changedSettingCount: 3,
      historyLength: 40,
    });
  });
  it('protects curves/grading and more than six settings even with a short or reopened history', () => {
    expect(
      assessEdit(
        {
          ...reset,
          pointCurves: {
            ...reset.pointCurves!,
            luma: [
              { x: 0, y: 0 },
              { x: 255, y: 220 },
            ],
          },
        },
        image,
        1,
      ),
    ).toMatchObject({ recommendVirtualCopy: true, hasCurves: true });
    expect(assessEdit({ ...reset, colorGrading: { ...reset.colorGrading, balance: 20 } }, image, 1)).toMatchObject({
      recommendVirtualCopy: true,
      hasColorGrading: true,
    });
    const adjusted = {
      ...reset,
      exposure: 1,
      brightness: 2,
      contrast: 3,
      shadows: 4,
      highlights: 5,
      whites: 6,
      blacks: 7,
    };
    expect(assessEdit(adjusted, image, 2).reasons).toContain('more-than-six-settings');
  });
});

it('protects seven nondefault perspective/transform controls rather than calling them only a crop', () => {
  const image = { width: 6000, height: 4000 };
  const reset = createResetAdjustmentsForImage(image);
  const edited = {
    ...reset,
    transformVertical: 12,
    transformHorizontal: -8,
    transformRotate: 2,
    transformAspect: 6,
    transformScale: 110,
    transformXOffset: 3,
    transformYOffset: -4,
  };
  const result = assessEdit(edited, image, 1);

  expect(result.changedKeys).toEqual(
    expect.arrayContaining([
      'transformVertical',
      'transformHorizontal',
      'transformRotate',
      'transformAspect',
      'transformScale',
      'transformXOffset',
      'transformYOffset',
    ]),
  );
  expect(result.changedSettingCount).toBeGreaterThanOrEqual(7);
  expect(result.recommendVirtualCopy).toBe(true);
  expect(result.reasons).toContain('more-than-six-settings');
});
it('keeps plain crop/aspect plus a few Basic sliders in place', () => {
  const image = { width: 6000, height: 4000 };
  const reset = createResetAdjustmentsForImage(image);
  const result = assessEdit(
    {
      ...reset,
      crop: { x: 500, y: 0, width: 3000, height: 4000, unit: 'px' },
      aspectRatio: 0.75,
      exposure: 0.6,
      shadows: 15,
    },
    image,
    20,
  );
  expect(result.changedSettingCount).toBe(2);
  expect(result.recommendVirtualCopy).toBe(false);
});

it('counts an explicitly selected manual lens profile rather than discarding it as a cache', () => {
  const image = { width: 6000, height: 4000 };
  const reset = createResetAdjustmentsForImage(image);
  const result = assessEdit(
    { ...reset, lensCorrectionMode: 'manual', lensMaker: 'Maker', lensModel: 'Manual profile' },
    image,
    1,
  );
  expect(result.changedSettingCount).toBe(2);
  expect(result.recommendVirtualCopy).toBe(true);
});
