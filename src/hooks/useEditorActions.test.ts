// @vitest-environment jsdom
import { createElement } from 'react';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it } from 'vitest';
import { mockCommand } from '../test/tauriMock';
import { useEditorActions } from './useEditorActions';
import { useEditorStore } from '../store/useEditorStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useProcessStore } from '../store/useProcessStore';
import { Invokes, Theme } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, PasteMode } from '../utils/adjustments';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialSettingsState = useSettingsStore.getState();
const initialProcessState = useProcessStore.getState();

afterEach(() => {
  useEditorStore.setState(initialEditorState, true);
  useSettingsStore.setState(initialSettingsState, true);
  useProcessStore.setState(initialProcessState, true);
});

async function mountEditorActions() {
  let actions!: ReturnType<typeof useEditorActions>;
  function Harness() {
    actions = useEditorActions();
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => {
    root.render(createElement(Harness));
  });
  return { actions, unmount: () => act(async () => root.unmount()) };
}

function mockPaste() {
  const applied: unknown[] = [];
  mockCommand(Invokes.ApplyAdjustmentsToPaths, (args) => {
    applied.push(args);
  });
  useEditorStore.setState({
    copiedAdjustments: {
      ...INITIAL_ADJUSTMENTS,
      exposure: 1,
      contrast: 20,
      baselineExposure: false,
      cameraExposureCompensation: false,
    },
  });
  return applied;
}

describe('useEditorActions.handlePasteAdjustments', () => {
  it('pastes with the default settings when the settings have no copyPasteSettings', async () => {
    // The fallback settings used when load_settings fails have no copyPasteSettings.
    useSettingsStore.setState({ appSettings: { lastRootPath: null, theme: Theme.Dark } });
    const applied = mockPaste();
    const { actions, unmount } = await mountEditorActions();

    expect(() => actions.handlePasteAdjustments(['/photos/a.raw'])).not.toThrow();

    expect(applied).toEqual([
      { paths: ['/photos/a.raw'], adjustments: expect.objectContaining({ exposure: 1, contrast: 20 }) },
    ]);
    expect(useProcessStore.getState().isPasted).toBe(true);
    await unmount();
  });

  it('freezes omitted legacy exposure policies when replacing exposure on a fresh target', async () => {
    useSettingsStore.setState({
      appSettings: {
        lastRootPath: null,
        theme: Theme.Dark,
        copyPasteSettings: {
          mode: PasteMode.Replace,
          includedAdjustments: ['exposure'],
          knownAdjustments: [],
          autoSync: false,
        },
      },
    });
    const applied = mockPaste();
    useEditorStore.setState({ copiedAdjustments: { exposure: 1 } });
    const { actions, unmount } = await mountEditorActions();
    actions.handlePasteAdjustments(['/photos/a.raw']);
    expect(applied).toEqual([
      {
        paths: ['/photos/a.raw'],
        adjustments: { exposure: 1, baselineExposure: false, cameraExposureCompensation: false },
      },
    ]);
    await unmount();
  });

  it('still respects the saved copyPasteSettings', async () => {
    useSettingsStore.setState({
      appSettings: {
        lastRootPath: null,
        theme: Theme.Dark,
        copyPasteSettings: {
          mode: PasteMode.Replace,
          includedAdjustments: ['exposure'],
          knownAdjustments: [],
          autoSync: false,
        },
      },
    });
    const applied = mockPaste();
    const { actions, unmount } = await mountEditorActions();

    actions.handlePasteAdjustments(['/photos/a.raw']);

    expect(applied).toEqual([
      {
        paths: ['/photos/a.raw'],
        adjustments: { exposure: 1, baselineExposure: false, cameraExposureCompensation: false },
      },
    ]);
    await unmount();
  });
});

describe('upstream comparison modes', () => {
  it('persists each selected mode and restores it when toggling comparison', async () => {
    let saved: unknown;
    mockCommand(Invokes.SaveSettings, (args) => {
      saved = args?.settings;
    });
    useSettingsStore.setState({ appSettings: { lastRootPath: null, theme: Theme.Dark } });
    const recipe = { ...INITIAL_ADJUSTMENTS, exposure: 1, rotation: 3, baselineExposure: false };
    useEditorStore.setState({ adjustments: recipe });
    const before = useEditorStore.getState();
    const { actions, unmount } = await mountEditorActions();
    try {
      for (const mode of ['vertical', 'horizontal', 'sideBySide', 'toggle'] as const) {
        await act(async () => actions.selectCompareMode(mode));
        expect(saved).toMatchObject({ compareMode: mode });
        expect(useEditorStore.getState().compareView).toBe(mode === 'toggle' ? 'off' : mode);
        expect(useEditorStore.getState().showOriginal).toBe(mode === 'toggle');
        await act(async () => actions.toggleCompare());
        expect(useEditorStore.getState()).toMatchObject({
          compareView: 'off',
          showOriginal: false,
          previewOverride: null,
        });
        // Rehydration uses the same persisted settings field as a fresh app session.
        useSettingsStore
          .getState()
          .setAppSettings(saved as NonNullable<ReturnType<typeof useSettingsStore.getState>['appSettings']>);
        await act(async () => actions.toggleCompare());
        expect(useEditorStore.getState().compareView).toBe(mode === 'toggle' ? 'off' : mode);
        expect(useEditorStore.getState().showOriginal).toBe(mode === 'toggle');
      }
      expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
      expect(useEditorStore.getState().history).toBe(before.history);
      expect(useEditorStore.getState().previewOverride).toMatchObject({
        exposure: 0,
        rotation: 3,
        baselineExposure: false,
      });
    } finally {
      await unmount();
    }
  });
});
