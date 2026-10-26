// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import InstagramFrameStrip from './InstagramFrameStrip';
import type { InstagramPreview } from './InstagramFraming';
import type { InstagramRatio } from './ExportImportProperties';
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it('bounds batch preview work and discards an in-flight old-ratio thumbnail without writing edits', async () => {
  const paths = Array.from({ length: 100 }, (_, i) => `/photo${i}.raw`);
  const preview = (ratio: InstagramRatio): InstagramPreview => ({
    jpeg: ratio,
    width: 1200,
    height: 800,
    framing: { version: 1, ratio, geometrySignature: 'geometry', frame: { x: 0.2, y: 0, width: 0.5, height: 1 } },
  });
  let release!: (value: InstagramPreview) => void;
  const first = new Promise<InstagramPreview>((resolve) => {
    release = resolve;
  });
  let inFlight = 0,
    maximum = 0;
  const calls: Array<{ path: string; ratio: InstagramRatio; maxSize: number }> = [];
  mocks.invoke.mockImplementation(async (command, args) => {
    expect(command).toBe('get_instagram_framing_preview');
    calls.push(args);
    maximum = Math.max(maximum, ++inFlight);
    try {
      return calls.length === 1 ? await first : preview(args.ratio);
    } finally {
      --inFlight;
    }
  });
  const host = document.createElement('div');
  document.body.append(host);
  const root = createRoot(host);
  const onSelect = vi.fn();
  const draw = (ratio: InstagramRatio, ready: boolean) => (
    <InstagramFrameStrip
      paths={paths}
      index={0}
      ratio={ratio}
      preview={ready ? preview(ratio) : null}
      active
      disabled={false}
      onSelect={onSelect}
    />
  );
  try {
    await act(async () => root.render(draw('3:4', true)));
    expect(calls).toHaveLength(1);
    expect(calls[0].maxSize).toBe(320);
    await act(async () => root.render(draw('1.91:1', false)));
    await act(async () => release(preview('3:4')));
    expect(calls).toHaveLength(1);
    expect(host.querySelector('image')).toBeNull();
    await act(async () => root.render(draw('1.91:1', true)));
    expect(calls.slice(1).every((call) => call.ratio === '1.91:1')).toBe(true);
    expect(calls).toHaveLength(5);
    expect(maximum).toBe(1);
    expect(host.querySelectorAll('[data-instagram-strip-index]')).toHaveLength(5);
    expect(host.querySelectorAll('image[href="data:image/jpeg;base64,3:4"]')).toHaveLength(0);
    await act(async () => (host.querySelector('[data-instagram-strip-index="3"]') as HTMLButtonElement).click());
    expect(onSelect).toHaveBeenCalledExactlyOnceWith(3);
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});
