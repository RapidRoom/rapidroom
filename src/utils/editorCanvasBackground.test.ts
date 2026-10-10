import { describe, expect, it } from 'vitest';
import { canvasBackgroundRgb, resolveEditorCanvasBackground } from './editorCanvasBackground';

describe('editor canvas background', () => {
  it('preserves the legacy neutral-grey preference and rejects unknown values', () => {
    expect(resolveEditorCanvasBackground(undefined, true)).toBe('mid-grey');
    expect(resolveEditorCanvasBackground('invalid', false)).toBe('theme');
  });

  it('resolves every saved choice for the native WGPU surface', () => {
    const theme = [0.2, 0.2, 0.2, 1] as [number, number, number, number];
    expect(canvasBackgroundRgb('black', theme)).toEqual([0, 0, 0, 1]);
    expect(canvasBackgroundRgb('dark-grey', theme)).toEqual([48 / 255, 48 / 255, 48 / 255, 1]);
    expect(canvasBackgroundRgb('mid-grey', theme)).toEqual([128 / 255, 128 / 255, 128 / 255, 1]);
    expect(canvasBackgroundRgb('white', theme)).toEqual([1, 1, 1, 1]);
    expect(canvasBackgroundRgb('theme', theme)).toBe(theme);
  });
});
