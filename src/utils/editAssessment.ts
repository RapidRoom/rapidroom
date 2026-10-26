import { createResetAdjustmentsForImage, type Adjustments } from './adjustments';
import { sameAdjustmentValue } from './editHistory';

export function assessEdit(adjustments: Adjustments, image: { width: number; height: number }, historyLength: number) {
  const reset = createResetAdjustmentsForImage(image);
  const changedKeys = Object.keys(reset).filter((key) => !sameAdjustmentValue(adjustments[key], reset[key]));
  const crop = new Set(['crop', 'aspectRatio']);
  const basic = new Set([
    'exposure',
    'brightness',
    'contrast',
    'highlights',
    'shadows',
    'whites',
    'blacks',
    'temperature',
    'tint',
    'whiteBalance',
    'baselineExposure',
  ]);
  // These are render caches, not evidence that a user edited a photograph.
  const derived = new Set(['curves', 'lensDistortionParams']);
  if (adjustments.lensCorrectionMode === 'auto') {
    // Auto is the fresh-RAW default (#345), not evidence of an edit.
    derived.add('lensCorrectionMode');
    derived.add('lensMaker');
    derived.add('lensModel');
  }
  const meaningful = changedKeys.filter((key) => !derived.has(key));
  const beyondCrop = meaningful.filter((key) => !crop.has(key));
  const masks = (adjustments.masks?.length ?? 0) > 0 || (adjustments.aiPatches?.length ?? 0) > 0;
  const curves =
    ['pointCurves', 'parametricCurve'].some((key) => changedKeys.includes(key)) || changedKeys.includes('curves');
  const grading = ['colorGrading', 'hsl', 'colorBalance'].some((key) => changedKeys.includes(key));
  const reasons = [
    ...(masks ? ['masks-or-retouching'] : []),
    ...(curves ? ['curves'] : []),
    ...(grading ? ['color-grading-or-hsl'] : []),
    ...(beyondCrop.length > 6 ? ['more-than-six-settings'] : []),
    ...(beyondCrop.some(
      (key) =>
        !basic.has(key) &&
        !['masks', 'aiPatches', 'pointCurves', 'parametricCurve', 'colorGrading', 'hsl', 'colorBalance'].includes(key),
    )
      ? ['edits-beyond-crop-and-basic']
      : []),
  ];
  return {
    baseline: 'fresh-reset-for-image',
    changedKeys: meaningful,
    changedSettingCount: beyondCrop.length,
    historyLength,
    hasMasks: masks,
    hasCurves: curves,
    hasColorGrading: grading,
    recommendVirtualCopy: reasons.length > 0,
    reasons,
  };
}
