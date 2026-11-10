import { ADJUSTMENT_GROUPS, type Adjustments } from './adjustments';
import { copyLensPolicyVersion } from './lensPolicyVersion';

const owns = (value: Partial<Adjustments>, key: string) => Object.prototype.hasOwnProperty.call(value, key);
const exposurePolicies = ['baselineExposure', 'cameraExposureCompensation'];
const lensKeys = ADJUSTMENT_GROUPS.geometry.find((group) => group.keys.includes('lensCorrectionMode'))!.keys;

export function captureAdjustments(
  source: Partial<Adjustments>,
  keys: string[],
  defaults?: Adjustments,
): Partial<Adjustments> {
  const patch: Partial<Adjustments> = {};
  for (const key of keys) {
    if (owns(source, key) && (!defaults || JSON.stringify(source[key]) !== JSON.stringify(defaults[key]))) {
      patch[key] = structuredClone(source[key]);
    }
  }

  // An intentional component/policy edit needs its interpretation, even when
  // its mode or numeric Exposure happens to equal the source photo's default.
  if (defaults && keys.includes('lensCorrectionMode')) {
    const versionChanged = (source.cameraLensCorrectionVersion ?? 0) !== (defaults.cameraLensCorrectionVersion ?? 0);
    if (lensKeys.some((key) => owns(patch, key)) || (source.lensCorrectionMode === 'auto' && versionChanged)) {
      patch.lensCorrectionMode = source.lensCorrectionMode;
    }
  }
  if (
    defaults &&
    keys.includes('exposure') &&
    exposurePolicies.some((key) => owns(source, key) && source[key] !== defaults[key])
  ) {
    patch.exposure = source.exposure;
  }
  if (owns(patch, 'exposure')) {
    for (const key of exposurePolicies) {
      if (owns(source, key)) patch[key] = source[key];
    }
  }
  // The interpretation travels with captured tone controls, including local masks.
  if (
    owns(source, 'toneSliderVersion') &&
    Object.keys(patch).some((key) => ['highlights', 'shadows', 'whites', 'blacks', 'masks'].includes(key))
  ) {
    patch.toneSliderVersion = source.toneSliderVersion;
  }
  return copyLensPolicyVersion(source, patch);
}
