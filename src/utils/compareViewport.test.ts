import { expect, it } from 'vitest';
import { preserveCompareViewport } from './compareViewport';

it('keeps the zoomed photo point and pixel magnification across split and side-by-side layouts', () => {
  const full = { width: 1000, height: 600, offsetX: 0, offsetY: 100, containerWidth: 1000, containerHeight: 800 };
  const half = { width: 500, height: 300, offsetX: 0, offsetY: 250, containerWidth: 500, containerHeight: 800 };
  const before = { scale: 2, positionX: -300, positionY: -400 };
  const split = preserveCompareViewport(full, half, before);
  expect(split).toEqual({ scale: 4, positionX: -550, positionY: -1200 });
  expect(split.scale * half.width).toBe(before.scale * full.width);
  expect((half.containerWidth / 2 - split.positionX) / split.scale / half.width).toBe(0.4);
  expect(preserveCompareViewport(half, full, split)).toEqual(before);
  expect(preserveCompareViewport(full, full, before)).toEqual(before);
});
