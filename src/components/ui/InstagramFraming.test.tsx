// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { it, expect, vi, afterEach } from 'vitest';
import InstagramFraming from './InstagramFraming';
import type { InstagramRatio } from './ExportImportProperties';
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../hooks/useEditorActions', () => ({
  debouncedSave: { flush: vi.fn() },
  waitForPendingEditorSaves: vi.fn(async () => {}),
}));
vi.mock('../../store/useEditorStore', () => ({
  useEditorStore: (select: (state: { selectedImage: null; adjustments: Record<string, never> }) => unknown) =>
    select({ selectedImage: null, adjustments: {} }),
}));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root | undefined, host: HTMLDivElement;
afterEach(async () => {
  await act(async () => root?.unmount());
  host?.remove();
  vi.useRealTimers();
});
it.each([
  'ratio-success',
  'path-success',
  'inactive-success',
  'unmount-success',
  'ratio-error',
  'path-error',
  'unmount-error',
])('ignores stale save completion: %s', async (mode) => {
  vi.useFakeTimers();
  let saveResolve!: () => void;
  let saveReject!: (error: Error) => void;
  const save = new Promise<void>((resolve, reject) => {
    saveResolve = resolve;
    saveReject = reject;
  });
  const pending = new Promise<void>(() => {});
  const initial = {
    jpeg: '',
    width: 1200,
    height: 800,
    framing: {
      version: 1,
      ratio: '4:5',
      geometrySignature: 'geometry',
      frame: { x: 0.2, y: 0, width: 0.533333, height: 1 },
    },
  };
  let loads = 0;
  mocks.invoke.mockImplementation((name: string) =>
    name === 'save_instagram_framing' ? save : ++loads === 1 ? Promise.resolve(initial) : pending,
  );
  let busy = true;
  const onBusy = (value: boolean) => (busy = value);
  const onFrame = vi.fn();
  const draw = (ratio: InstagramRatio, path = 'photo.jpg', active = true) => (
    <InstagramFraming paths={[path]} ratio={ratio} active={active} disabled={false} onBusy={onBusy} onFrame={onFrame} />
  );
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(draw('4:5')));
  await act(async () => vi.advanceTimersByTimeAsync(300));
  expect(busy).toBe(false);
  const frame = host.querySelector('[data-instagram-frame]')!;
  await act(async () =>
    frame.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })),
  );
  expect(busy).toBe(true);
  expect(mocks.invoke).toHaveBeenCalledWith('save_instagram_framing', expect.anything());
  if (mode.startsWith('unmount')) {
    await act(async () => root!.unmount());
    root = undefined;
  } else {
    await act(async () =>
      root!.render(
        mode.startsWith('path') ? draw('4:5', 'other.jpg') : draw('9:16', 'photo.jpg', !mode.startsWith('inactive')),
      ),
    );
  }
  await act(async () => vi.advanceTimersByTimeAsync(300));
  if (!mode.startsWith('unmount') && !mode.startsWith('inactive')) {
    expect(loads).toBe(2);
    expect(host.querySelector('[data-instagram-frame]')).toBeNull();
  }
  expect(busy).toBe(true);
  await act(async () => {
    if (mode.endsWith('error')) saveReject(new Error('old context error'));
    else saveResolve();
  });
  expect(busy).toBe(true);
  expect(host.querySelector('[role=alert]')).toBeNull();
});
