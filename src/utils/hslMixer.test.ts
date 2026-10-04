import { describe, expect, it } from 'vitest';
import { sampleHslPresence } from './hslMixer';

const pixels = (...rgb: Array<[number, number, number]>) =>
  Uint8ClampedArray.from(rgb.flatMap(([r, g, b]) => [r, g, b, 255]));

describe('sampleHslPresence', () => {
  it('ignores neutral pixels', () => {
    expect(sampleHslPresence(pixels([128, 128, 128], [255, 255, 255]), 'saturation')).toEqual({});
  });

  it('preserves the shader linear-RGB neutral gate near black', () => {
    for (const property of ['hue', 'saturation', 'luminance'] as const) {
      expect(sampleHslPresence(pixels([1, 0, 0], [2, 1, 0]), property)).toEqual({});
      expect(sampleHslPresence(pixels([255, 0, 0]), property).reds).toBe(1);
    }
  });

  it('makes the sampled band dominant', () => {
    expect(sampleHslPresence(pixels([255, 0, 0]), 'hue').reds).toBe(1);
    expect(sampleHslPresence(pixels([0, 0, 255]), 'hue').blues).toBe(1);
  });

  it('weights bands by perceptual sRGB hue from the current shader', () => {
    // Current main encodes linear RGB before deriving hue: this is 47 degrees,
    // closest to Yellow. The old linear-hue sampler incorrectly favored Orange.
    const presence = sampleHslPresence(pixels([255, 200, 0]), 'hue');
    expect(presence.yellows).toBe(1);
    expect(presence.oranges ?? 0).toBeLessThan(0.5);
  });
});
