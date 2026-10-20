// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it } from 'vitest';
import ShootingInfo from './ShootingInfo';
import { useSettingsStore } from '../../store/useSettingsStore';
import type { AppSettings } from './AppProperties';
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initial = useSettingsStore.getState();
afterEach(() => useSettingsStore.setState(initial, true));

it('defaults existing settings to Always and reacts to hover/off without altering metadata', async () => {
  const host = document.createElement('div');
  const root = createRoot(host);
  const exif = Object.freeze({
    LensModel: 'FE 35mm F1.4 GM',
    FocalLength: '35',
    FNumber: '1.4',
    ExposureTime: '1/125 s',
    ISO: '1600',
  });
  try {
    await act(async () => root.render(<ShootingInfo exif={exif} />));
    expect(host.textContent).toBe('Sony 35mm f1.4 GM·35mm · f/1.4 · 1/125 · ISO 1600');
    expect(host.querySelector('[data-shooting-info]')?.getAttribute('title')).toBe(
      'Sony 35mm f1.4 GM · 35mm · f/1.4 · 1/125 · ISO 1600',
    );
    await act(async () =>
      useSettingsStore.setState({ appSettings: { shootingInfoVisibility: 'hover' } as AppSettings }),
    );
    expect(host.querySelector('.opacity-0')).not.toBeNull();
    await act(async () => useSettingsStore.setState({ appSettings: { shootingInfoVisibility: 'off' } as AppSettings }));
    expect(host.textContent).toBe('');
  } finally {
    await act(async () => root.unmount());
  }
});
