// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import CurveGraph from './Curves';
import { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments, type Adjustments } from '../../utils/adjustments';

vi.mock('../../context/ContextMenuContext', () => ({ useContextMenu: () => ({ showContextMenu: vi.fn() }) }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const edited = [
  { x: 0, y: 12 },
  { x: 128, y: 140 },
  { x: 255, y: 255 },
];

function legacyPointEdit(): Adjustments {
  return normalizeLoadedAdjustments({
    ...INITIAL_ADJUSTMENTS,
    curveMode: 'point',
    curves: { ...INITIAL_ADJUSTMENTS.curves, luma: edited },
    // Older point-mode UI changed curves only and left this default stale.
    pointCurves: INITIAL_ADJUSTMENTS.curves,
    nestedLookCurves: [],
  }) as Adjustments;
}

const props = (adjustments: Adjustments, setAdjustments: (updater: (prev: Adjustments) => Adjustments) => void) => ({
  adjustments,
  histogram: null,
  setAdjustments,
  theme: 'dark',
});

describe('legacy point-mode curve state', () => {
  it('shows the edited points instead of the stale default pointCurves', () => {
    const html = renderToStaticMarkup(createElement(CurveGraph, props(legacyPointEdit(), vi.fn())));
    expect(html).toContain('cx="128"');
    expect(html).toContain('cy="243"');
  });

  it('keeps the legacy edit when switching to parametric mode', async () => {
    let actual = legacyPointEdit();
    const node = document.createElement('div');
    document.body.append(node);
    const root = createRoot(node);
    try {
      await act(async () => {
        root.render(
          createElement(
            CurveGraph,
            props(actual, (update) => {
              actual = update(actual);
            }),
          ),
        );
      });
      const toggle = node.querySelector<HTMLButtonElement>('[data-tooltip="adjustments.curves.parametricCurve"]');
      expect(toggle).toBeTruthy();
      await act(async () => toggle!.click());
      expect(actual.curves.luma).toEqual(edited);
    } finally {
      await act(async () => root.unmount());
      node.remove();
    }
  });
});
