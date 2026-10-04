import { describe, expect, it } from 'vitest';
import { sampleHslPresence } from './hslMixer';

const pixels = (...rgb: Array<[number, number, number]>) =>
  Uint8ClampedArray.from(rgb.flatMap(([r, g, b]) => [r, g, b, 255]));

describe('sampleHslPresence', () => {
  it('ignores neutral pixels', () => {
    expect(sampleHslPresence(pixels([128, 128, 128], [255, 255, 255]), 'saturation')).toEqual({});
  });

  it('makes the sampled band dominant', () => {
    expect(sampleHslPresence(pixels([255, 0, 0]), 'hue').reds).toBe(1);
    expect(sampleHslPresence(pixels([0, 0, 255]), 'hue').blues).toBe(1);
  });

  it('weights bands by linear-RGB hue, as the shader does', () => {
    // sRGB (255, 200, 0) has a 47° hue (yellow) as encoded, but 35° (orange) in linear light.
    const presence = sampleHslPresence(pixels([255, 200, 0]), 'hue');
    expect(presence.oranges).toBe(1);
    expect(presence.yellows ?? 0).toBeLessThan(0.5);
  });
});
