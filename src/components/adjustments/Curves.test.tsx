// @vitest-environment jsdom
import { act, createElement, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { INITIAL_ADJUSTMENTS, type Adjustments } from '../../utils/adjustments';
import CurveGraph from './Curves';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../context/ContextMenuContext', () => ({ useContextMenu: () => ({ showContextMenu: vi.fn() }) }));
vi.mock('../ui/Slider', () => ({ default: () => null }));
vi.mock('./CurvePresetControl', () => ({
  default: () => createElement('div', { 'data-testid': 'curve-preset-control' }),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: ReturnType<typeof createRoot> | undefined;
afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it.each([false, true])('shows the curve preset control only outside masks (isForMask=%s)', async (isForMask) => {
  const container = document.createElement('div');
  root = createRoot(container);
  await act(async () =>
    root!.render(
      createElement(CurveGraph, {
        adjustments: INITIAL_ADJUSTMENTS,
        setAdjustments: vi.fn(),
        histogram: null,
        isForMask,
        theme: 'dark',
      }),
    ),
  );
  expect(container.querySelector('[data-testid="curve-preset-control"]') !== null).toBe(!isForMask);
});

it('keeps refused point insertion idle and releases the next accepted drag', async () => {
  // jsdom does not expose the browser's SVGCircleElement constructor.
  vi.stubGlobal('SVGCircleElement', class {});
  const dragChanged = vi.fn();
  const changed = vi.fn();
  function Harness() {
    const [adjustments, setAdjustments] = useState<Adjustments>({
      ...INITIAL_ADJUSTMENTS,
      curveMode: 'point',
      curves: {
        ...INITIAL_ADJUSTMENTS.curves,
        luma: [
          { x: 0, y: 0 },
          { x: 128, y: 128 },
          { x: 255, y: 255 },
        ],
      },
    });
    return createElement(CurveGraph, {
      adjustments,
      setAdjustments: (update) => {
        changed();
        setAdjustments(update);
      },
      histogram: null,
      theme: 'dark',
      onDragStateChange: dragChanged,
    });
  }
  const container = document.createElement('div');
  root = createRoot(container);
  await act(async () => root!.render(createElement(Harness)));
  const svg = container.querySelector('svg[viewBox="0 0 255 255"]')!;
  vi.spyOn(svg, 'getBoundingClientRect').mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: 255,
    bottom: 255,
    width: 255,
    height: 255,
    toJSON: () => ({}),
  });
  dragChanged.mockClear();
  await act(async () => {
    // Same x as an existing point, but beyond the 12px point hit radius.
    svg.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: 128, clientY: 30 }));
    document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
  });
  expect(changed).not.toHaveBeenCalled();
  expect(dragChanged).not.toHaveBeenCalledWith(true);
  expect(svg.querySelectorAll('circle')).toHaveLength(3);

  await act(async () => {
    svg.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: 64, clientY: 150 }));
  });
  expect(changed).toHaveBeenCalledOnce();
  expect(dragChanged).toHaveBeenCalledWith(true);
  expect(svg.querySelectorAll('circle')).toHaveLength(4);
  await act(async () => document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true })));
  expect(dragChanged).toHaveBeenLastCalledWith(false);
});
