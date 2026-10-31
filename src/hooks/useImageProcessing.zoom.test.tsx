// @vitest-environment jsdom
import { act, createElement, createRef } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { Invokes, Panel, Theme, type SelectedImage } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { mockCommand } from '../test/tauriMock';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { useImageProcessing } from './useImageProcessing';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const editor = useEditorStore.getState(),
  ui = useUIStore.getState(),
  settings = useSettingsStore.getState();
afterEach(() => {
  useEditorStore.setState(editor, true);
  useUIStore.setState(ui, true);
  useSettingsStore.setState(settings, true);
  vi.useRealTimers();
});

it('settles the preview resolution after zooming out so edits and Undo use the same fitted pixels', async () => {
  vi.useFakeTimers();
  const requests: Array<Record<string, unknown>> = [];
  mockCommand(Invokes.ApplyAdjustments, (args) => {
    requests.push(args!);
    return new ArrayBuffer(0);
  });
  const image: SelectedImage = {
    path: '/photos/sony.ARW',
    loadGeneration: 1,
    isReady: true,
    width: 4608,
    height: 3072,
    isRaw: true,
    exif: {},
    thumbnailUrl: '',
  };
  useUIStore.setState({ activeView: 'editor', activePanel: Panel.Adjustments });
  useSettingsStore.setState({ appSettings: { theme: Theme.Dark, lastRootPath: null } });
  useEditorStore.setState({
    selectedImage: image,
    adjustments: INITIAL_ADJUSTMENTS,
    originalSize: { width: 4608, height: 3072 },
    displaySize: { width: 1000, height: 667 },
  });
  const refs = {
    previewJobIdRef: { current: 0 },
    latestRenderedJobIdRef: { current: 0 },
    currentResRef: { current: 0 },
  };
  const previous = { current: { path: image.path, adjustments: INITIAL_ADJUSTMENTS } };
  function Harness() {
    useImageProcessing(createRef(), previous, refs);
    return null;
  }
  const root = createRoot(document.createElement('div'));
  const settle = () =>
    act(async () => {
      await vi.advanceTimersByTimeAsync(70);
    });
  try {
    await act(async () => root.render(createElement(Harness)));
    await settle();
    expect(requests.at(-1)?.targetResolution).toBe(1280);
    await act(async () => useEditorStore.setState({ displaySize: { width: 4000, height: 2667 } }));
    await settle();
    expect(requests.at(-1)?.targetResolution).toBe(4608);
    await act(async () => useEditorStore.setState({ displaySize: { width: 1000, height: 667 } }));
    await settle();
    expect(requests.at(-1)?.targetResolution).toBe(1280);
    const count = requests.length;
    await act(async () => useEditorStore.setState({ adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 1 } }));
    await settle();
    await act(async () => useEditorStore.setState({ adjustments: INITIAL_ADJUSTMENTS }));
    await settle();
    expect(requests.slice(count).map((row) => row.targetResolution)).toEqual([1280, 1280]);
    // Infinity is navigation's sentinel while a cached photo has no backend source yet.
    refs.currentResRef.current = Infinity;
    const pending = requests.length;
    await act(async () => useEditorStore.setState({ displaySize: { width: 4000, height: 2667 } }));
    await settle();
    expect(requests).toHaveLength(pending);
  } finally {
    await act(async () => root.unmount());
  }
});
