// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import '../../i18n';
import PointColorPanel from '../adjustments/PointColor';
import { createPointColor, normalizePointColors, POINT_COLOR_RANGES } from '../../utils/pointColor';
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
afterEach(() => vi.unstubAllGlobals());
it.each(Object.keys(POINT_COLOR_RANGES) as Array<keyof typeof POINT_COLOR_RANGES>)(
  'writes numeric %s through the real Slider',
  async (key) => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        disconnect() {}
      },
    );
    const onChange = vi.fn();
    const host = document.createElement('div');
    const root = createRoot(host);
    await act(async () =>
      root.render(
        createElement(PointColorPanel, {
          points: [createPointColor({ lightness: 0.7, chroma: 0.1, hue: 30 }, 'skin')],
          onChange,
        }),
      ),
    );
    const slider =
      host.querySelectorAll<HTMLInputElement>('input[type=range]')[Object.keys(POINT_COLOR_RANGES).indexOf(key)];
    slider.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      width: 100,
      height: 20,
      right: 100,
      bottom: 20,
      x: 0,
      y: 0,
      toJSON() {
        return {};
      },
    });
    await act(async () =>
      slider.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, buttons: 1, clientX: 75 })),
    );
    await act(async () => document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 })));
    const recipe = onChange.mock.calls.at(-1)![0];
    await act(async () => root.unmount());
    expect(typeof recipe[0][key]).toBe('number');
    const [min, max, step] = POINT_COLOR_RANGES[key];
    expect(recipe[0][key]).toBeCloseTo(Math.round((min + (max - min) * 0.75) / step) * step, 3);
    expect(normalizePointColors(recipe)[0][key]).toBe(recipe[0][key]);
  },
);
