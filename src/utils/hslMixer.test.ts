import { describe, expect, it } from 'vitest';
import { sampleHslPresence } from './hslMixer';

// The backend probe carries hue/360, saturation and luminance per pixel, sampled at the HSL stage.
const probe = (...hueSat: Array<[number, number]>) =>
  Uint8Array.from(hueSat.flatMap(([hue, saturation]) => [(hue / 360) * 255, saturation * 255, 128, 255]));

describe('sampleHslPresence', () => {
  it('ignores neutral pixels', () => {
    expect(sampleHslPresence(probe([0, 0], [200, 0]), 'saturation')).toEqual({});
  });

  it('makes the sampled band dominant', () => {
    expect(sampleHslPresence(probe([0, 1]), 'hue').reds).toBe(1);
    expect(sampleHslPresence(probe([240, 1]), 'hue').blues).toBe(1);
  });

  it('weights bands by the hue the shader reports', () => {
    const presence = sampleHslPresence(probe([60, 1]), 'hue');
    expect(presence.yellows).toBe(1);
    expect(presence.oranges ?? 0).toBeLessThan(0.5);
  });
});
