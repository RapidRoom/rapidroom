import { describe, expect, it } from 'vitest';
import type { Crop } from 'react-image-crop';
import {
  calculateCenteredCrop,
  calculateAreaPreservingCrop,
  calculateAutoCropForRotation,
  getOrientedDimensions,
  isCropWithinBounds,
  normalizeCropChange,
  resolveAspectRatio,
  zoomCrop,
} from './cropUtils';

const full: Crop = { unit: 'px', x: 0, y: 0, width: 7008, height: 4672 };

describe('equivalent full-frame crop changes', () => {
  it('keeps a null crop when entering or completing the full frame', () => {
    expect(normalizeCropChange(null, null, 7008, 4672)).toBeNull();
    expect(normalizeCropChange(null, full, 7008, 4672)).toBeNull();
    expect(normalizeCropChange(null, calculateCenteredCrop(7008, 4672, 0, 1.5), 7008, 4672)).toBeNull();
  });

  it('preserves an existing full-frame object without a metadata rewrite', () => {
    expect(normalizeCropChange(full, { ...full }, 7008, 4672)).toBe(full);
    expect(normalizeCropChange(full, null, 7008, 4672)).toBe(full);
    const legacy = { ...full, unit: undefined } as unknown as Crop;
    expect(normalizeCropChange(legacy, full, 7008, 4672)).toBe(legacy);
  });

  it.each([0, 1, 2, 3])('uses the image axes after orientation step %i', (orientation) => {
    const turned = orientation % 2 ? { ...full, width: 4672, height: 7008 } : full;
    expect(normalizeCropChange(null, turned, 7008, 4672, orientation)).toBeNull();
    if (orientation % 2) expect(normalizeCropChange(null, full, 7008, 4672, orientation)).toBe(full);
  });

  it('retains a genuine portrait aspect-ratio crop and a straighten crop', () => {
    const portrait = calculateCenteredCrop(7008, 4672, 0, 0.75)!;
    const straightened = calculateCenteredCrop(7008, 4672, 0, 1.5, 15)!;
    expect(normalizeCropChange(null, portrait, 7008, 4672)).toBe(portrait);
    expect(normalizeCropChange(null, straightened, 7008, 4672, 0, 15)).toBe(straightened);
    expect(normalizeCropChange(null, full, 7008, 4672, 0, 15)).toBe(full);
  });

  it('preserves repeated real crops and treats returning to the full frame as an edit', () => {
    const partial = { ...full, x: 10, width: 6998 };
    expect(normalizeCropChange(partial, { ...partial }, 7008, 4672)).toBe(partial);
    expect(normalizeCropChange(partial, full, 7008, 4672)).toBeNull();
    const onePixel = { ...full, width: 7007 };
    expect(normalizeCropChange(null, onePixel, 7008, 4672)).toBe(onePixel);
  });

  it('does not mistake percentage units or unknown image dimensions for a pixel frame', () => {
    const percent: Crop = { unit: '%', x: 0, y: 0, width: 100, height: 100 };
    expect(normalizeCropChange(null, percent, 100, 100)).toBe(percent);
    expect(normalizeCropChange(null, full, 0, 0)).toBe(full);
  });
});

describe('Original ratio and fixed Instagram crop compatibility', () => {
  it.each([0, 1, 2, 3])('resolves Original against oriented axes at step %i', (steps) => {
    expect(resolveAspectRatio(0, { width: 6000, height: 4000 }, steps)).toBe(steps % 2 ? 2 / 3 : 1.5);
    expect(resolveAspectRatio(null, { width: 6000, height: 4000 }, steps)).toBeNull();
    expect(resolveAspectRatio(1.5, { width: 6000, height: 4000 }, steps)).toBe(1.5);
  });

  it('waits for real dimensions instead of inventing a lock', () => {
    expect(resolveAspectRatio(0, null, 0)).toBeNull();
    for (const width of [0, -1, NaN, Infinity]) {
      expect(resolveAspectRatio(0, { width, height: 4000 }, 0)).toBeNull();
    }
    expect(resolveAspectRatio(0, { width: 6000, height: 0 }, 0)).toBeNull();
  });

  it.each([0.75, 0.8, 1, 1080 / 565])('keeps fixed Instagram ratio %f across image orientations', (ratio) => {
    for (const steps of [0, 1, 2, 3]) {
      const resolved = resolveAspectRatio(ratio, { width: 6000, height: 4000 }, steps);
      const crop = calculateCenteredCrop(6000, 4000, steps, resolved, 5)!;
      const image = getOrientedDimensions(6000, 4000, steps);
      expect(resolved).toBe(ratio);
      expect(crop.width / crop.height).toBeCloseTo(ratio, 2);
      expect(isCropWithinBounds(crop, image.width, image.height, 5)).toBe(true);
    }
  });

  it('uses the image ratio for Original crop zoom and the crop ratio for Free zoom', () => {
    const crop: Crop = { unit: 'px', x: 2000, y: 1000, width: 1000, height: 1000 };
    const original = zoomCrop(crop, 0.8, 6000, 4000, 0, 0);
    const free = zoomCrop(crop, 0.8, 6000, 4000, 0, null);
    expect(original.width / original.height).toBeCloseTo(1.5, 2);
    expect(free.width / free.height).toBe(1);
  });
});

describe('saved percentage crop geometry regression', () => {
  const percent: Crop = { unit: '%', x: 10, y: 20, width: 30, height: 60 };
  const pixels: Crop = { unit: 'px', x: 600, y: 800, width: 1800, height: 2400 };

  it('preserves the same frame area and centre when switching from3:4 to4:5', () => {
    const expected = calculateAreaPreservingCrop(6000, 4000, 0, 0.8, 0, pixels)!;
    const changed = calculateAreaPreservingCrop(6000, 4000, 0, 0.8, 0, percent)!;
    expect(changed).toEqual(expected);
    expect(changed.width).toBeGreaterThan(1800);
    expect(changed.height).toBeGreaterThan(2000);
    expect(Math.abs(changed.x + changed.width / 2 - 1500)).toBeLessThanOrEqual(0.5);
    expect(Math.abs(changed.y + changed.height / 2 - 2000)).toBeLessThanOrEqual(0.5);
  });

  it('straightens a saved percentage ROI in image pixels without collapsing it', () => {
    const changed = calculateAutoCropForRotation(6000, 4000, 0, 0.75, 1, percent, 1)!;
    expect(changed).toEqual(calculateAutoCropForRotation(6000, 4000, 0, 0.75, 1, pixels, 1));
    expect(changed.width).toBeGreaterThan(1000);
    expect(changed.height).toBeGreaterThan(1500);
    expect(isCropWithinBounds(changed, 6000, 4000, 1)).toBe(true);
  });
});
