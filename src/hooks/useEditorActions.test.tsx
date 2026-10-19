import { createElement, type SyntheticEvent } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useEditorStore } from '../store/useEditorStore';
import { useProcessStore } from '../store/useProcessStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { debouncedSetHistory, useEditorActions } from './useEditorActions';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

afterEach(() => {
  debouncedSetHistory.cancel();
  vi.useRealTimers();
});

describe('unchanged editor actions', () => {
  it('leaves history and Original unchanged, then records one real edit', () => {
    vi.useFakeTimers();
    const initial = { ...INITIAL_ADJUSTMENTS };
    useEditorStore.getState().resetHistory(initial);
    useEditorStore.getState().setEditor({ showOriginal: true, previewOverride: initial });
    let actions: ReturnType<typeof useEditorActions> | undefined;
    function Probe() {
      actions = useEditorActions();
      return null;
    }
    renderToStaticMarkup(createElement(Probe));
    const before = useEditorStore.getState();
    const subscriber = vi.fn();
    const unsubscribe = useEditorStore.subscribe(subscriber);
    try {
      actions!.setAdjustments((prev) => prev);
      vi.advanceTimersByTime(600);
      expect(useEditorStore.getState()).toBe(before);
      expect(subscriber).not.toHaveBeenCalled();
      expect(useEditorStore.getState().history).toHaveLength(1);
      expect(useEditorStore.getState().showOriginal).toBe(true);

      actions!.setAdjustments((prev) => ({ ...prev, exposure: 0.2 }));
      vi.advanceTimersByTime(600);
      expect(useEditorStore.getState().history).toHaveLength(2);
      expect(useEditorStore.getState().historyIndex).toBe(1);
      expect(useEditorStore.getState().adjustments.exposure).toBe(0.2);
      expect(useEditorStore.getState().showOriginal).toBe(false);
    } finally {
      unsubscribe();
    }
  });
});

describe('copy adjustments', () => {
  it('copies the selected photo when invoked by the ordinary Copy button click', async () => {
    const adjustments = { ...INITIAL_ADJUSTMENTS, exposure: 0.75 };
    useEditorStore.getState().setEditor({
      selectedImage: {
        exif: {},
        height: 100,
        isRaw: true,
        isReady: true,
        path: '/photos/selected.raw',
        thumbnailUrl: '',
        width: 100,
      },
      adjustments,
      copiedAdjustments: null,
    });
    useLibraryStore.getState().setLibrary({ libraryActivePath: null, multiSelectedPaths: [] });
    useProcessStore.getState().setProcess({ isCopied: false });

    let actions: ReturnType<typeof useEditorActions> | undefined;
    function Probe() {
      actions = useEditorActions();
      return null;
    }
    renderToStaticMarkup(createElement(Probe));

    await actions!.handleCopyAdjustments({} as SyntheticEvent);

    expect(invoke).toHaveBeenCalledWith('get_default_raw_lens_adjustments', { path: '/photos/selected.raw' });
    expect(useEditorStore.getState().copiedAdjustments?.exposure).toBe(0.75);
    expect(useProcessStore.getState().isCopied).toBe(true);
  });
});

describe('labelled async editor actions', () => {
  const path = '/photos/history.raw';
  function setup() {
    vi.useFakeTimers();
    useEditorStore.getState().resetHistory({ ...INITIAL_ADJUSTMENTS });
    useEditorStore.setState({
      selectedImage: { path, width: 6000, height: 4000, isRaw: true, isReady: true, thumbnailUrl: '', exif: {} },
    });
    useLibraryStore.setState({ multiSelectedPaths: [], libraryActivePath: null });
    let actions: ReturnType<typeof useEditorActions> | undefined;
    function Probe() {
      actions = useEditorActions();
      return null;
    }
    renderToStaticMarkup(createElement(Probe));
    return actions!;
  }

  it('preserves the pending slider state as the undo target of Reset', async () => {
    vi.mocked(invoke).mockImplementation(async () => ({}));
    const actions = setup();
    actions.setAdjustments({ exposure: 0.75 });
    const pending = useEditorStore.getState().adjustments;
    actions.handleResetAdjustments([path]);
    await vi.advanceTimersByTimeAsync(0);
    const after = useEditorStore.getState();
    expect(after.adjustments.exposure).toBe(0);
    expect(after.historyDetails.at(-1)?.label).toBe('Reset adjustments');
    after.undo();
    expect(useEditorStore.getState().adjustments).toEqual(pending);
    await vi.advanceTimersByTimeAsync(600);
    expect(useEditorStore.getState().historyIndex).toBe(1);
  });

  it('does not overwrite a later edit when Auto lens finishes', async () => {
    let finish!: () => void;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'apply_auto_lens_correction_to_paths')
        await new Promise<void>((resolve) => {
          finish = resolve;
        });
      if (command === 'load_metadata')
        return { adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 0.75, lensDistortionEnabled: true } };
      return {};
    });
    const actions = setup();
    actions.setAdjustments({ exposure: 0.75 });
    actions.handleAutoLensCorrection([path]);
    actions.setAdjustments({ exposure: 1 });
    const later = useEditorStore.getState().adjustments;
    finish();
    await vi.advanceTimersByTimeAsync(600);
    expect(useEditorStore.getState().adjustments).toEqual(later);
    expect(useEditorStore.getState().historyDetails.some((entry) => entry?.label === 'Auto lens correction')).toBe(
      false,
    );
  });
});
