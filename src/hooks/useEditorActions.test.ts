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
  useEditorStore.setState({ copiedAdjustments: { ...INITIAL_ADJUSTMENTS, exposure: 1, contrast: 20 } });
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

    expect(applied).toEqual([{ paths: ['/photos/a.raw'], adjustments: { exposure: 1 } }]);
    await unmount();
  });
});
