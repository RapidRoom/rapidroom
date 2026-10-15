import { describe, expect, it } from 'vitest';
import { clampDockHeight } from './dockHeight';

describe('dock height limits', () => {
  it('leaves most of short and tall windows for photos', () => {
    expect(clampDockHeight(800, 600)).toBe(270);
    expect(clampDockHeight(800, 1200)).toBe(400);
    expect(clampDockHeight(240, 400)).toBe(180);
    expect(clampDockHeight(360, 1000)).toBe(360);
  });

  it('handles malformed persisted heights and keeps the resize minimum', () => {
    expect(clampDockHeight(NaN)).toBe(240);
    expect(clampDockHeight(Infinity)).toBe(240);
    expect(clampDockHeight(-100)).toBe(80);
  });
});
