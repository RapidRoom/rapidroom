// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Basic from './Basic';
import { INITIAL_ADJUSTMENTS } from '../../utils/adjustments';
const translations = vi.hoisted(() => ({ t: (key: string) => key }));
vi.mock('react-i18next', () => ({ useTranslation: () => translations }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let node: HTMLDivElement;
let root: Root;
beforeEach(() => {
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
});
afterEach(async () => {
  await act(async () => root.unmount());
  node.remove();
});
it('renders the familiar Tone slider order and preserves default workspace Brightness', async () => {
  await act(async () =>
    root.render(
      <Basic
        adjustments={INITIAL_ADJUSTMENTS}
        setAdjustments={() => {}}
        includeBrightness={false}
        appSettings={{ tonemapperOverrideEnabled: true }}
      />,
    ),
  );
  const ranges = [...node.querySelectorAll('input[type=range]')];
  expect(ranges).toHaveLength(6);
  const text = node.textContent!;
  const tone = ['exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks'];
  expect(tone.map((key) => text.indexOf(`adjustments.basic.${key}`))).toEqual(
    [...tone.map((key) => text.indexOf(`adjustments.basic.${key}`))].sort((a, b) => a - b),
  );
  expect(text).not.toContain('adjustments.basic.brightness');
  expect(ranges[0].getAttribute('min')).toBe('-5');
  expect(ranges[0].getAttribute('max')).toBe('5');
  await act(async () =>
    root.render(
      <Basic
        adjustments={INITIAL_ADJUSTMENTS}
        setAdjustments={() => {}}
        appSettings={{ tonemapperOverrideEnabled: true }}
      />,
    ),
  );
  expect(node.querySelectorAll('input[type=range]')).toHaveLength(7);
  expect(node.textContent).toContain('adjustments.basic.brightness');
});
