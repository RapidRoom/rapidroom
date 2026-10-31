import { expect, it } from 'vitest';
import contract from '../../rapidroom/validation/lens-policy-version-cases.json';
import { INITIAL_ADJUSTMENTS, type Adjustments } from './adjustments';
import { mixAdjustments } from './presetAmount';
import { copyLensPolicyVersion, normalizeLensPolicyVersion } from './lensPolicyVersion';

it('keeps the shared legacy/camera version contract while preserving actual preset Amount interpolation', () => {
  for (const row of contract.cases) {
    const before = { ...INITIAL_ADJUSTMENTS, ...row.target } as Adjustments;
    const source = row.source as Partial<Adjustments>;
    const mixed = mixAdjustments(source, row.amount, before);
    expect(mixed.lensVignetteAmount, row.name).toBeCloseTo(row.expected_vignette_amount);
    if (row.expected_version === null)
      expect(Object.hasOwn(mixed, 'cameraLensCorrectionVersion'), row.name).toBe(false);
    else expect(mixed.cameraLensCorrectionVersion, row.name).toBe(row.expected_version);
    if (row.amount === 100) {
      const copied = copyLensPolicyVersion(source, source);
      const pasted = normalizeLensPolicyVersion({ ...before, ...copied }, copied, before);
      expect(pasted.cameraLensCorrectionVersion, row.name).toBe(mixed.cameraLensCorrectionVersion);
    }
  }
});
