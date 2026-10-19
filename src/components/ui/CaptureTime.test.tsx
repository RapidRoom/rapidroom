// @vitest-environment jsdom
import React, { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import CaptureTime from './CaptureTime';
import { useSettingsStore } from '../../store/useSettingsStore';
import { type AppSettings, Invokes } from './AppProperties';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) => [key, ...Object.values(values ?? {})].join(' '),
  }),
}));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const initialSettings = useSettingsStore.getState();
const container = document.createElement('div');
document.body.append(container);
let root = createRoot(container);
const exif = { DateTimeOriginal: '2026:10:19 23:30:00', OffsetTimeOriginal: '-10:00' };

afterEach(async () => {
  await act(async () => root.unmount());
  root = createRoot(container);
  useSettingsStore.setState(initialSettings, true);
  invoke.mockReset();
});

async function enterHours(value: string) {
  const input = document.querySelector<HTMLInputElement>('input[aria-label="captureTime.hours"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

it('persists a display-only folder shift using the latest settings and resets only that folder', async () => {
  invoke.mockResolvedValue(undefined);
  useSettingsStore.setState({
    appSettings: { folderCaptureTimeShiftMinutes: { '/other': 30 }, language: 'en' } as unknown as AppSettings,
  });
  await act(async () => root.render(<CaptureTime path="/trip/a.jpg" exif={exif} editable />));
  expect(container.textContent).toContain('2026-10-19 23:30:00 UTC-10:00');
  await act(async () => container.querySelector('button')!.click());
  await enterHours('19');
  await act(async () =>
    useSettingsStore.setState({ appSettings: { ...useSettingsStore.getState().appSettings!, language: 'ja' } }),
  );
  await act(async () =>
    document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })),
  );
  expect(invoke).toHaveBeenCalledExactlyOnceWith(Invokes.SaveSettings, {
    settings: { language: 'ja', folderCaptureTimeShiftMinutes: { '/other': 30, '/trip': 1140 } },
  });
  expect(container.textContent).toContain('2026-10-20 18:30:00');
  expect(container.textContent).toContain('captureTime.shifted +19:00');
  expect(container.textContent).toContain('captureTime.cameraZone UTC-10:00');
  expect(exif).toEqual({ DateTimeOriginal: '2026:10:19 23:30:00', OffsetTimeOriginal: '-10:00' });
  await act(async () => container.querySelector('button')!.click());
  await enterHours('0');
  await act(async () =>
    document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })),
  );
  expect(useSettingsStore.getState().appSettings?.folderCaptureTimeShiftMinutes).toEqual({ '/other': 30 });
  expect(container.textContent).toContain('2026-10-19 23:30:00 UTC-10:00');
});

it('closes the editor on folder navigation without saving the previous folder shift', async () => {
  useSettingsStore.setState({ appSettings: {} as AppSettings });
  await act(async () => root.render(<CaptureTime path="/old/a.jpg" exif={exif} editable />));
  await act(async () => container.querySelector('button')!.click());
  await enterHours('19');
  await act(async () => root.render(<CaptureTime path="/new/b.jpg" exif={exif} editable />));
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(invoke).not.toHaveBeenCalled();
});
