// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { emit, listen, mockCommand } from '../test/tauriMock';
import { Theme } from '../components/ui/AppProperties';
import { useSettingsStore } from '../store/useSettingsStore';
import { visibleControls, WindowControlState } from './windowControls';
import TitleBar from './TitleBar';

const windowMock = vi.hoisted(() => ({
  listen: vi.fn(),
  onResized: vi.fn(),
  minimize: vi.fn(),
  close: vi.fn(),
  maximize: vi.fn(),
  unmaximize: vi.fn(),
  setFullscreen: vi.fn(),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => windowMock }));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'linux' }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const state: WindowControlState = { canMinimize: false, canMaximize: true, maximized: false, fullscreen: false };
const originalSettings = useSettingsStore.getState();
let root: Root | null = null;
let element: HTMLDivElement;
beforeEach(() => {
  vi.clearAllMocks();
  windowMock.listen.mockImplementation(listen);
  windowMock.onResized.mockResolvedValue(() => {});
  mockCommand('get_window_control_state', () => state);
  element = document.createElement('div');
  document.body.append(element);
  root = createRoot(element);
});
afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  element.remove();
  useSettingsStore.setState(originalSettings, true);
});
const mount = () => act(async () => root!.render(createElement(TitleBar)));
const button = (label: string) => element.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);

it('hides only unsupported controls in Auto and retains maximize and close', async () => {
  await mount();
  expect(button('Minimize window')).toBeNull();
  expect(button('Maximize window')).not.toBeNull();
  expect(button('Close window')).not.toBeNull();
  await act(async () =>
    useSettingsStore.setState({ appSettings: { lastRootPath: null, theme: Theme.Dark, windowControls: 'always' } }),
  );
  expect(button('Minimize window')).not.toBeNull();
  await act(async () =>
    useSettingsStore.setState({ appSettings: { lastRootPath: null, theme: Theme.Dark, windowControls: 'hide' } }),
  );
  expect(button('Minimize window')).toBeNull();
  expect(button('Maximize window')).toBeNull();
  expect(button('Close window')).not.toBeNull();
});

it('accepts a compositor state event without a resize and rejects a stale snapshot', async () => {
  let resolve!: (s: WindowControlState) => void;
  mockCommand(
    'get_window_control_state',
    () =>
      new Promise<WindowControlState>((r) => {
        resolve = r;
      }),
  );
  await mount();
  await act(async () => emit('window-control-state', { ...state, maximized: true }));
  expect(button('Restore window')).not.toBeNull();
  await act(async () => resolve(state));
  expect(button('Restore window')).not.toBeNull();
});

it('does not show Restore until the compositor grants maximization', async () => {
  await mount();
  await act(async () => button('Maximize window')!.click());
  expect(windowMock.maximize).toHaveBeenCalledOnce();
  expect(button('Maximize window')).not.toBeNull();
  await act(async () => emit('window-control-state', { ...state, maximized: true }));
  mockCommand('get_window_control_state', () => ({ ...state, maximized: true }));
  await act(async () => button('Restore window')!.click());
  expect(windowMock.unmaximize).toHaveBeenCalledOnce();
});

it('cleans up a listener that finishes attaching after unmount', async () => {
  let resolve!: (unlisten: () => void) => void;
  const unlisten = vi.fn();
  windowMock.listen.mockImplementation(
    () =>
      new Promise<() => void>((r) => {
        resolve = r;
      }),
  );
  await mount();
  await act(async () => root!.unmount());
  root = null;
  await act(async () => resolve(unlisten));
  expect(unlisten).toHaveBeenCalledOnce();
  expect(windowMock.onResized).not.toHaveBeenCalled();
});

it('unknown compositors remain usable and user overrides are independent', () => {
  expect(visibleControls({ ...state, canMinimize: true }, 'auto')).toEqual({ minimize: true, maximize: true });
  expect(visibleControls(null, 'always')).toEqual({ minimize: true, maximize: true });
  expect(visibleControls(state, 'hide')).toEqual({ minimize: false, maximize: false });
});
