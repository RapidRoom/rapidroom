// @vitest-environment jsdom
import { act, createElement, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { mockCommand } from '../../test/tauriMock';
import { useEditorStore } from '../../store/useEditorStore';
import { useEditorActions, debouncedSetHistory } from '../../hooks/useEditorActions';
import { INITIAL_ADJUSTMENTS, type Adjustments } from '../../utils/adjustments';
import Effects from './Effects';

const capture = vi.hoisted(() => ({
  switches: new Map<string, (value: boolean) => void>(),
  t: (key: string) => key,
  error: vi.fn(),
}));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: capture.t }) }));
vi.mock('react-toastify', () => ({ toast: { error: capture.error, info: vi.fn() } }));
vi.mock('../ui/Switch', () => ({
  default: (props: { label: string; onChange: (value: boolean) => void }) => {
    capture.switches.set(props.label, props.onChange);
    return null;
  },
}));
vi.mock('../ui/Slider', () => ({ default: () => null }));
vi.mock('../ui/LUTControl', () => ({ default: () => null }));
vi.mock('../ui/DepthRangePicker', () => ({ DepthRangePicker: () => null }));
vi.mock('./Color', () => ({ ColorSwatch: () => null }));
vi.mock('./AdjustmentSubSection', () => ({ default: (props: { children: ReactNode }) => props.children }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initial = useEditorStore.getState();
let root: ReturnType<typeof createRoot> | null = null;
let container: HTMLDivElement;
type MapResult = { imagePath: string; dataUrl: string };
const cases = [
  ['relight', 'generate_relight_normal_map', 'relightNormalMap', 'relightEnabled', 'generatingNormalMap'],
  ['fog', 'generate_full_image_depth_map', 'fogDepthMap', 'fogEnabled', 'generatingDepthMap'],
  ['lensBlur', 'generate_full_image_depth_map', 'lensBlurDepthMap', 'lensBlurEnabled', 'generatingDepthMap'],
] as const;

function Harness() {
  const adjustments = useEditorStore((s) => s.adjustments);
  const { setAdjustments } = useEditorActions();
  return createElement(Effects, { adjustments, setAdjustments, appSettings: null, handleLutSelect: () => {} });
}
async function select(path: string, changes: Partial<Adjustments> = {}) {
  debouncedSetHistory.cancel();
  const adjustments = { ...INITIAL_ADJUSTMENTS, ...changes };
  await act(async () => {
    useEditorStore.setState({
      selectedImage: { path, isReady: true } as NonNullable<typeof initial.selectedImage>,
      adjustments,
      history: [adjustments],
      historyIndex: 0,
    });
  });
}
async function mount() {
  container = document.createElement('div');
  root = createRoot(container);
  await act(async () => root!.render(createElement(Harness)));
}
async function toggle(kind: string, enabled = true) {
  await act(async () => capture.switches.get('adjustments.effects.' + kind)!(enabled));
}
function pending(command: string) {
  let finish!: (value: MapResult) => void;
  let fail!: (error: Error) => void;
  const invoked = vi.fn(
    () =>
      new Promise<MapResult>((resolve, reject) => {
        finish = resolve;
        fail = reject;
      }),
  );
  mockCommand(command, invoked);
  return {
    invoked,
    finish: (imagePath = '/a.raw') => finish({ imagePath, dataUrl: 'map-from-' + imagePath }),
    fail: () => fail(new Error('inference failed')),
  };
}
afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  debouncedSetHistory.cancel();
  useEditorStore.setState(initial, true);
  capture.switches.clear();
  capture.error.mockClear();
});

it.each(cases)('applies a completed %s map only to its initiating image', async (kind, command, key) => {
  const job = pending(command);
  await select('/a.raw');
  await mount();
  await toggle(kind);
  expect(job.invoked).toHaveBeenCalledWith({ imagePath: '/a.raw' });
  await act(async () => job.finish());
  expect(useEditorStore.getState().adjustments[key]).toBe('map-from-/a.raw');
  await act(async () => debouncedSetHistory.flush());
  expect(useEditorStore.getState().history.at(-1)?.[key]).toBe('map-from-/a.raw');
});

it.each(cases)(
  'discards a pending %s map after selecting another image',
  async (kind, command, key, enabled, spinner) => {
    const job = pending(command);
    await select('/a.raw');
    await mount();
    await toggle(kind);
    expect(container.textContent).toContain(spinner);
    await select('/b.raw', { [enabled]: true });
    expect(container.textContent).not.toContain(spinner);
    await act(async () => job.finish());
    await act(async () => debouncedSetHistory.flush());
    expect(useEditorStore.getState().adjustments[key]).toBeNull();
    expect(useEditorStore.getState().adjustments[enabled]).toBe(true);
    expect(useEditorStore.getState().history).toHaveLength(1);
  },
);

it.each(cases)(
  'ignores a stale %s failure without disabling the newly selected effect',
  async (kind, command, key, enabled) => {
    const job = pending(command);
    await select('/a.raw');
    await mount();
    await toggle(kind);
    await select('/b.raw', { [enabled]: true });
    await act(async () => job.fail());
    await act(async () => debouncedSetHistory.flush());
    expect(useEditorStore.getState().adjustments[enabled]).toBe(true);
    expect(useEditorStore.getState().adjustments[key]).toBeNull();
    expect(useEditorStore.getState().history).toHaveLength(1);
    expect(capture.error).not.toHaveBeenCalled();
  },
);

it('invalidates the request when returning to the same path', async () => {
  const job = pending('generate_full_image_depth_map');
  await select('/a.raw');
  await mount();
  await toggle('fog');
  await select('/b.raw');
  await select('/a.raw');
  await act(async () => job.finish());
  expect(useEditorStore.getState().adjustments.fogDepthMap).toBeNull();
});

it('keeps a newer request spinner and result when an older request completes', async () => {
  const old = pending('generate_relight_normal_map');
  await select('/a.raw');
  await mount();
  await toggle('relight');
  await select('/b.raw');
  const current = pending('generate_relight_normal_map');
  await toggle('relight');
  await act(async () => old.finish());
  expect(container.textContent).toContain('generatingNormalMap');
  expect(useEditorStore.getState().adjustments.relightNormalMap).toBeNull();
  await act(async () => current.finish('/b.raw'));
  expect(container.textContent).not.toContain('generatingNormalMap');
  expect(useEditorStore.getState().adjustments.relightNormalMap).toBe('map-from-/b.raw');
});

it('discards a result whose native source identity does not match the request', async () => {
  const job = pending('generate_full_image_depth_map');
  await select('/a.raw');
  await mount();
  await toggle('fog');
  await act(async () => job.finish('/b.raw'));
  expect(useEditorStore.getState().adjustments.fogDepthMap).toBeNull();
});

it('discards pending results after the panel unmounts', async () => {
  const job = pending('generate_full_image_depth_map');
  await select('/a.raw');
  await mount();
  await toggle('fog');
  await act(async () => root!.unmount());
  root = null;
  debouncedSetHistory.cancel();
  await act(async () => job.finish());
  await act(async () => debouncedSetHistory.flush());
  expect(useEditorStore.getState().adjustments.fogDepthMap).toBeNull();
});
