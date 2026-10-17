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
