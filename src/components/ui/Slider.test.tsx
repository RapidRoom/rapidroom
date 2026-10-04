// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import Slider, { SliderDensity, SliderDensityScope } from './Slider';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

let root: Root | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  act(() => root?.unmount());
  container?.remove();
  root = null;
  container = null;
  vi.restoreAllMocks();
});

function mount(density: SliderDensity | undefined, value = 0) {
  const onChange = vi.fn();
  container = document.createElement('div');
  document.body.appendChild(container);
  root = createRoot(container);
  const slider = createElement(Slider, {
    label: 'Exposure',
    min: -100,
    max: 100,
    step: 1,
    value,
    onChange,
  });
  act(() => {
    root!.render(density ? createElement(SliderDensityScope, { density }, slider) : slider);
  });
  const range = container.querySelector('input[type="range"]') as HTMLInputElement;
  const row = range.parentElement!.parentElement!;
  return { range, row, onChange };
}

function setWidth(el: Element, width: number) {
  vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    width,
    top: 0,
    height: 20,
    right: width,
    bottom: 20,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect);
}

function drag(range: HTMLInputElement, fromX: number, toX: number) {
  act(() => {
    range.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: fromX }));
  });
  act(() => {
    window.dispatchEvent(new MouseEvent('mousemove', { clientX: toX }));
  });
  act(() => {
    window.dispatchEvent(new MouseEvent('mouseup'));
  });
}

const lastValue = (onChange: ReturnType<typeof vi.fn>) => onChange.mock.calls.at(-1)![0].target.value;

describe('Slider density', () => {
  it('keeps the stacked layout outside a compact scope', () => {
    const { range } = mount(undefined);
    const outer = container!.firstElementChild!;
    expect(outer.className).toContain('mb-2');
    expect(outer.children).toHaveLength(2);
    expect(range.hasAttribute('aria-label')).toBe(false);
  });

  it('puts label, track and value on one row when compact', () => {
    const { range, row } = mount('compact');
    expect(row.children).toHaveLength(3);
    expect(row.children[0].textContent).toContain('Exposure');
    expect(row.children[1].contains(range)).toBe(true);
    expect(row.children[2].textContent).toBe('0');
    expect(range.getAttribute('aria-label')).toBe('Exposure');
  });

  it('gears a drag to the track width in the stacked layout', () => {
    const { range, onChange } = mount('comfortable');
    setWidth(range, 200);
    drag(range, 100, 120);
    expect(lastValue(onChange)).toBe(20);
  });

  it('gears a compact drag to the whole row, matching the stacked feel', () => {
    const { range, row, onChange } = mount('compact');
    setWidth(range, 100);
    setWidth(row, 200);
    drag(range, 50, 70);
    expect(lastValue(onChange)).toBe(20);
  });

  it('accepts typed values and double-click reset in compact mode', () => {
    const { row, onChange } = mount('compact', 30);
    const valueSpan = row.children[2].querySelector('span')!;
    act(() => valueSpan.dispatchEvent(new MouseEvent('click', { bubbles: true })));
    const input = row.children[2].querySelector('input')!;
    expect(input).not.toBeNull();

    const label = row.children[0];
    act(() => label.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })));
    expect(lastValue(onChange)).toBe(0);
  });
});
