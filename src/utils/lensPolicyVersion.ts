import type { Adjustments } from './adjustments';

const owns = (recipe: Partial<Adjustments>, key: string) => Object.prototype.hasOwnProperty.call(recipe, key);

// The version travels with an explicit lens policy, never as an interpolated slider.
export function copyLensPolicyVersion(source: Partial<Adjustments>, patch: Partial<Adjustments>) {
  const result = { ...patch };
  delete result.cameraLensCorrectionVersion;
  if (owns(patch, 'lensCorrectionMode') && source.cameraLensCorrectionVersion === 1)
    result.cameraLensCorrectionVersion = 1;
  return result;
}

export function normalizeLensPolicyVersion(
  merged: Partial<Adjustments>,
  source: Partial<Adjustments>,
  target: Partial<Adjustments>,
  amount = 100,
): Partial<Adjustments> {
  const result = { ...merged };
  const replacing = owns(source, 'lensCorrectionMode') && amount >= 100;
  const version = replacing ? source.cameraLensCorrectionVersion : target.cameraLensCorrectionVersion;
  if (version === 1) result.cameraLensCorrectionVersion = 1;
  else if (owns(target, 'cameraLensCorrectionVersion')) result.cameraLensCorrectionVersion = 0;
  else delete result.cameraLensCorrectionVersion;
  return result;
}
