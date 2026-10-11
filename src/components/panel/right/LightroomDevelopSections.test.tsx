// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { beforeEach, afterEach, it, expect, vi } from 'vitest';
import LightroomDevelopSections from './LightroomDevelopSections';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
import type { AppSettings } from '../../ui/AppProperties';
const tr = vi.hoisted(() => ({ t: (key: string) => key }));
vi.mock('react-i18next', () => ({ useTranslation: () => tr }));
vi.mock('../../adjustments/Basic', () => ({ default: () => null }));
vi.mock('../../adjustments/Curves', () => ({ default: () => null }));
vi.mock('../../adjustments/Color', () => ({ default: () => null }));
vi.mock('../../adjustments/Details', () => ({ default: () => null }));
vi.mock('../../adjustments/Effects', () => ({ default: () => null }));
vi.mock('./CropPanel', () => ({ default: () => null }));
vi.mock('../editor/Waveform', () => ({ default: () => null }));
vi.mock('../../ui/Slider', () => ({ default: () => null }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root, node: HTMLDivElement;
const ui = useUIStore.getState(),
  settings = useSettingsStore.getState(),
  editor = useEditorStore.getState();
beforeEach(async () => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
      unobserve() {}
    },
  );
  useUIStore.setState({
    ...ui,
    collapsibleSectionsState: { basic: true, color: false, curves: false, details: false, effects: false },
  });
  useSettingsStore.setState({
    ...settings,
    appSettings: { ...settings.appSettings, workspacePreset: 'lightroom', enableFocusMode: true } as AppSettings,
  });
  useEditorStore.setState({
    ...editor,
    adjustments: {
      ...INITIAL_ADJUSTMENTS,
      sectionVisibility: { ...INITIAL_ADJUSTMENTS.sectionVisibility, basic: false },
    },
  });
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () => root.render(<LightroomDevelopSections />));
});
afterEach(async () => {
  await act(async () => root.unmount());
  node.remove();
  useUIStore.setState(ui, true);
  useSettingsStore.setState(settings, true);
  useEditorStore.setState(editor, true);
});
const section = (key: string) =>
  [...node.querySelectorAll('[role=button]')].find((el) => el.textContent?.includes(key))!;
it('reviewer: solo mode closes initially open Basic when opening Tone Curve', async () => {
  expect(section('lightroomWorkspace.basic').getAttribute('aria-expanded')).toBe('true');
  await act(async () => (section('lightroomWorkspace.curve') as HTMLElement).click());
  expect(section('lightroomWorkspace.curve').getAttribute('aria-expanded')).toBe('true');
  expect(section('lightroomWorkspace.basic').getAttribute('aria-expanded')).toBe('false');
});
it('reviewer: disabled engine Basic is indicated in the regrouped UI', () => {
  const basic = section('lightroomWorkspace.basic').parentElement!;
  expect(basic.querySelector('.opacity-30')).not.toBeNull();
});

it('retains engine section IDs and exposes disabled Color, Details, Mixer, Grading and Effects', async () => {
  await act(async () =>
    useEditorStore.setState({
      adjustments: {
        ...INITIAL_ADJUSTMENTS,
        sectionVisibility: {
          ...INITIAL_ADJUSTMENTS.sectionVisibility,
          color: false,
          details: false,
          colorMixer: false,
          colorGrading: false,
          effects: false,
        },
      },
    }),
  );
  for (const key of ['mixer', 'grading', 'detail', 'effects', 'calibration']) {
    expect(section(`lightroomWorkspace.${key}`).parentElement!.querySelector('.opacity-30')).not.toBeNull();
    expect(section(`lightroomWorkspace.${key}`).querySelector('button[aria-pressed=false]')).not.toBeNull();
  }
  const basic = section('lightroomWorkspace.basic').parentElement!;
  expect(basic.querySelector('[data-engine-section=color] .opacity-30')).not.toBeNull();
  expect(basic.querySelector('[data-engine-section=details] .opacity-30')).not.toBeNull();
  await act(async () => (section('lightroomWorkspace.detail').querySelector('button') as HTMLButtonElement).click());
  expect(useEditorStore.getState().adjustments.sectionVisibility.details).toBe(true);
  expect(useEditorStore.getState().adjustments.sectionVisibility.color).toBe(false);
});
