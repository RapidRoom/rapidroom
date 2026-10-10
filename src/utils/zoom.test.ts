import { describe, expect, it } from 'vitest';
import {
  MAX_ZOOM_PERCENT,
  MIN_ZOOM_PERCENT,
  clampZoomPercent,
  gtkWheelStep,
  isAtMaxZoom,
  isDiscreteWheel,
  maxZoomPercent,
  percentFromTransform,
  stepZoomIn,
  stepZoomOut,
  transformFromPercent,
  zoomLimits,
  zoomReferenceSize,
  zoomStops,
} from './zoom';

const monitors = [
  { width: 1280, height: 720 },
  { width: 2880, height: 1800 },
  { width: 3840, height: 2160 },
];
const images = [
  { width: 6000, height: 4000 },
  { width: 100, height: 80 },
  { width: 1000, height: 500 },
];

describe('physical zoom across displays, crops and orientations', () => {
  for (const viewport of monitors) {
    for (const dpr of [1, 1.6, 2]) {
      for (const image of images) {
        for (const orientation of [0, 1, 3]) {
          it(`${viewport.width} / DPR ${dpr} / ${image.width} / orientation ${orientation}`, () => {
            const reference = zoomReferenceSize(image, orientation);
            const renderScale = Math.min(
              viewport.width / dpr / reference.width,
              viewport.height / dpr / reference.height,
            );
            const fit = renderScale * dpr;
            const limits = zoomLimits(renderScale, dpr);
            const max = maxZoomPercent(fit);
            expect(limits.maxScale).toBeGreaterThanOrEqual(2);
            expect(percentFromTransform(renderScale, limits.maxScale, dpr)).toBeCloseTo(max, 10);
            if (image.width === 6000) expect(max).toBe(MAX_ZOOM_PERCENT);
            for (const percent of [MIN_ZOOM_PERCENT, 0.25, 1, 1.5, 4, 1000]) {
              const transform = transformFromPercent(percent, renderScale, dpr);
              expect(transform).toBeLessThanOrEqual(limits.maxScale);
              expect(transform).toBeGreaterThanOrEqual(limits.minScale);
              expect(percentFromTransform(renderScale, transform, dpr)).toBeCloseTo(clampZoomPercent(percent, fit), 10);
            }
            expect(renderScale * transformFromPercent(1, renderScale, dpr)).toBeCloseTo(1 / dpr, 10);
          });
        }
      }
    }
  }

  it('uses the displayed crop, with coordinates already oriented', () => {
    expect(zoomReferenceSize(images[0], 1, images[2])).toEqual(images[2]);
    expect(zoomReferenceSize(images[0], 1)).toEqual({ width: 4000, height: 6000 });
  });
});

describe('regular stops', () => {
  it('inserts fit in its ordered position and deduplicates exact matches', () => {
    expect(zoomStops(0.4)).toEqual([0.01, 0.02, 0.05, 0.1, 0.25, 0.33, 0.4, 0.5, 0.67, 1, 1.5, 2, 3, 4]);
    expect(zoomStops(0.5).filter((stop) => stop === 0.5)).toHaveLength(1);
  });
  it('moves from 100% to 150% in one notch, with floating-point tolerance', () => {
    expect(stepZoomIn(1, 0.4)).toBe(1.5);
    expect(stepZoomIn(1 - 1e-10, 0.4)).toBe(1.5);
    expect(stepZoomOut(1.5 + 1e-10, 0.4)).toBe(1);
  });
  it.each([0.005, 0.4, 1, 3, 20])('walks both ways without skipping fit %f or the tiny-image maximum', (fit) => {
    const stops = zoomStops(fit);
    for (let i = 0; i < stops.length - 1; i++) {
      expect(stepZoomIn(stops[i], fit)).toBe(stops[i + 1]);
      expect(stepZoomOut(stops[i + 1], fit)).toBe(stops[i]);
    }
    expect(stepZoomIn(stops.at(-1)!, fit)).toBe(maxZoomPercent(fit));
    expect(stepZoomOut(stops[0], fit)).toBe(stops[0]);
  });
  it('uses the next stop from arbitrary smooth zoom', () => {
    expect(stepZoomIn(1.437, 0.4)).toBe(1.5);
    expect(stepZoomOut(1.437, 0.4)).toBe(1);
  });
});

describe('wheel input classification', () => {
  const event = { ctrlKey: false, deltaMode: 0, deltaX: 0, deltaY: -100 };
  it.each([-100, 100, -120, 240])('recognizes common pixel-mode wheel notches %f', (deltaY) => {
    expect(isDiscreteWheel({ ...event, deltaY })).toBe(true);
  });
  it.each([1, 2])('recognizes line/page mode %f', (deltaMode) => {
    expect(isDiscreteWheel({ ...event, deltaY: 3, deltaMode })).toBe(true);
  });
  it.each([0, -1, 2.5, 16, 99.5])('leaves fine and smooth deltas %f continuous', (deltaY) => {
    expect(isDiscreteWheel({ ...event, deltaY })).toBe(false);
  });
  it.each([900, 1200])('recognizes native WebKitGTK ticks for WebView height %f', (height) => {
    const step = gtkWheelStep(height);
    expect(step).toBe(height === 900 ? 93 : 112);
    for (const ticks of [-2, -1, 1, 2]) {
      expect(isDiscreteWheel({ ...event, deltaY: -step * ticks, wheelDeltaY: 120 * ticks }, height)).toBe(true);
    }
    expect(isDiscreteWheel({ ...event, deltaY: -step, wheelDeltaY: -120 }, height)).toBe(false);
    expect(isDiscreteWheel({ ...event, deltaY: -step, wheelDeltaY: 120.1 }, height)).toBe(false);
    expect(isDiscreteWheel({ ...event, deltaY: -step, wheelDeltaY: NaN }, height)).toBe(false);
    expect(isDiscreteWheel({ ...event, deltaY: -step + 0.5, wheelDeltaY: 120 }, height)).toBe(false);
    expect(isDiscreteWheel({ ...event, deltaY: -step, wheelDeltaY: 120, ctrlKey: true }, height)).toBe(false);
    expect(isDiscreteWheel({ ...event, deltaY: -16, wheelDeltaY: 48 }, height)).toBe(false);
  });
  it('never snaps a pinch or two-axis scroll', () => {
    expect(isDiscreteWheel({ ...event, ctrlKey: true })).toBe(false);
    expect(isDiscreteWheel({ ...event, deltaX: 1 })).toBe(false);
  });
});

it('maximum rendering and cursor threshold scales with the actual limit', () => {
  for (const max of [2, 5, 20]) {
    expect(isAtMaxZoom(max, max)).toBe(true);
    expect(isAtMaxZoom(max - 0.1, max)).toBe(false);
  }
});
