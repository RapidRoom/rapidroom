import { Adjustments, INITIAL_ADJUSTMENTS } from './adjustments';

/** The neutral recipe used by the Editor's Original control, with display geometry retained. */
export const originalComparisonAdjustments = (adjustments: Adjustments): Adjustments => {
  const original: Adjustments = { ...INITIAL_ADJUSTMENTS };
  const preservedKeys: Array<keyof Adjustments> = [
    'toneSliderVersion',
    'baselineExposure',
    'cameraExposureCompensation',
    'crop',
    'rotation',
    'flipHorizontal',
    'flipVertical',
    'orientationSteps',
    'aspectRatio',
    'transformDistortion',
    'transformVertical',
    'transformHorizontal',
    'transformRotate',
    'transformAspect',
    'transformScale',
    'transformXOffset',
    'transformYOffset',
    'lensDistortionAmount',
    'lensCorrectionMode',
    'cameraLensCorrectionVersion',
    'lensVignetteAmount',
    'lensTcaAmount',
    'lensDistortionParams',
    'lensMaker',
    'lensModel',
    'lensDistortionEnabled',
    'lensTcaEnabled',
    'lensVignetteEnabled',
  ];
  preservedKeys.forEach((key) => {
    original[key] = adjustments[key];
  });
  return original;
};
