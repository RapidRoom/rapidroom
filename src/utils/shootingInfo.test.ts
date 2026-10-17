import { describe, expect, it } from 'vitest';
import { shootingInfoParts } from './shootingInfo';

describe('shooting info from recorded EXIF', () => {
  it('includes lens and recorded focal length before exposure values', () => {
    expect(
      shootingInfoParts({
        LensModel: 'FE 35mm F1.4 GM',
        FocalLength: '35',
        FocalLengthIn35mmFilm: '35 mm',
        FNumber: 'f/1.4',
        ExposureTime: '1/125 s',
        ISO: '1600',
      }),
    ).toEqual({ lens: 'FE 35mm F1.4 GM', details: '35 mm · f/1.4 · 1/125 s · ISO 1600' });
  });
  it('keeps a differing equivalent without replacing the recorded focal length', () => {
    expect(
      shootingInfoParts({ FocalLength: '23 mm', FocalLengthIn35mmFilm: '35', FNumber: 2, PhotographicSensitivity: 800 })
        .details,
    ).toBe('23 mm (35 mm eq.) · f/2 · ISO 800');
  });
  it('handles older ISO and equivalent-only EXIF, missing data and lens-only photos', () => {
    expect(shootingInfoParts(null)).toEqual({ lens: '', details: '' });
    expect(shootingInfoParts({ LensModel: 'Unknown lens', ExposureTime: null })).toEqual({
      lens: 'Unknown lens',
      details: '',
    });
    expect(shootingInfoParts({ FocalLengthIn35mmFilm: '50mm', FNumber: 'F 2.8', ISO: 200 }).details).toBe(
      '50 mm · f/2.8 · ISO 200',
    );
  });
});
