import { describe, expect, it } from 'vitest';
import type { Crop } from 'react-image-crop';
import { calculateCenteredCrop, normalizeCropChange } from './cropUtils';

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
