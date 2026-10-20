import { describe, expect, it } from 'vitest';
import { friendlyLensName, formatShutterSpeed, shootingInfoParts } from './shootingInfo';

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
    ).toEqual({ lens: 'Sony 35mm f1.4 GM', details: '35mm · f/1.4 · 1/125 · ISO 1600' });
  });
  it('keeps a differing equivalent without replacing the recorded focal length', () => {
    expect(
      shootingInfoParts({ FocalLength: '23 mm', FocalLengthIn35mmFilm: '35', FNumber: 2, PhotographicSensitivity: 800 })
        .details,
    ).toBe('23mm (35mm eq.) · f/2 · ISO 800');
  });
  it('handles older ISO and equivalent-only EXIF, missing data and lens-only photos', () => {
    expect(shootingInfoParts(null)).toEqual({ lens: '', details: '' });
    expect(shootingInfoParts({ LensModel: 'Unknown lens', ExposureTime: null })).toEqual({
      lens: 'Unknown lens',
      details: '',
    });
    expect(shootingInfoParts({ FocalLengthIn35mmFilm: '50mm', FNumber: 'F 2.8', ISO: 200 }).details).toBe(
      '50mm · f/2.8 · ISO 200',
    );
  });
});

// Owner #375 examples plus spellings from the bundled Lensfun XML database.
describe('friendly EXIF and Lensfun display names', () => {
  it.each([
    ['FE 35mm F1.4 GM', 'Sony 35mm f1.4 GM'],
    ['FE 24-70mm F2.8 GM II', 'Sony 24-70mm f2.8 GM II'],
    ['FE 70-200mm F4 Macro G OSS II', 'Sony 70-200mm f4 Macro G II'],
    ['E 35-150mm F2-2.8 A058', 'Tamron 35-150mm f2-2.8'],
    ['SEL2470GM2', 'Sony 24-70mm f2.8 GM II'],
    ['Tamron E 35-150mm f/2-2.8 2.0-2.8 Di III VXD A058Z', 'Tamron 35-150mm f2-2.8'],
    ['FE 24-70mm f/4 ZA OSS', 'Sony 24-70mm f4 ZA'],
    ['Sigma 35mm f/1.4 DG HSM | A', 'Sigma 35mm f1.4 Art'],
    ['Sigma 24-35mm f/2 DG HSM | Art 015', 'Sigma 24-35mm f2 Art'],
    ['Tamron SP AF 70-200mm f/2.8 Di LD (IF) Macro (A001)', 'Tamron 70-200mm f2.8 Macro'],
    ['Nikon AF-S Nikkor 24-70mm f/2.8E ED VR', 'Nikon AF-S Nikkor 24-70mm f/2.8E ED VR'],
    ['E 20mm F2 Unknown', 'E 20mm F2 Unknown'],
    ['Tamron E 28-75mm F/2.8 Di III VXD G2 A063Z', 'Tamron 28-75mm f2.8 G2'],
    ['Tamron 28-75mm F/2.8 Di III RXD', 'Tamron 28-75mm f2.8'],
    ['Sigma 60-600mm f/4.5-6.3 DG OS HSM | S', 'Sigma 60-600mm f4.5-6.3 Sports'],
    ['Sigma 150-600mm f/5-6.3 DG OS HSM | C', 'Sigma 150-600mm f5-6.3 Contemporary'],
    ['Unknown lens', 'Unknown lens'],
  ])('%s → %s', (raw, expected) => expect(friendlyLensName(raw)).toBe(expected));
  it('uses the lens maker, never the camera maker, to identify third-party E lenses', () => {
    expect(friendlyLensName('E 28-75mm F2.8 Di III VXD', 'TAMRON')).toBe('Tamron 28-75mm f2.8');
    expect(shootingInfoParts({ Make: 'Sony', LensModel: 'E 20mm F2 Unknown' }).lens).toBe('E 20mm F2 Unknown');
  });
});

describe('compact shutter units at the one-second boundary', () => {
  it.each([
    ['1/125 s', '1/125'],
    ['1/2 s', '1/2'],
    ['0.008 s', '1/125'],
    ['0.4 s', '0.4'],
    ['1 s', '1s'],
    [1, '1s'],
    ['2.5 s', '2.5s'],
    ['30 s', '30s'],
    ['5/2 s', '2.5s'],
    ['1/0 s', '1/0 s'],
    ['Bulb', 'Bulb'],
  ])('%s → %s', (raw, expected) => expect(formatShutterSpeed(raw)).toBe(expected));
});
