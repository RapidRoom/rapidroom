import { createElement, type SyntheticEvent } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useEditorStore } from '../store/useEditorStore';
import { useProcessStore } from '../store/useProcessStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { debouncedSetHistory, useEditorActions, withRotation, reloadAdjustmentsForPaths } from './useEditorActions';
import { useUIStore } from '../store/useUIStore';

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

describe('auto straighten keeps current photo edits and crop history safe', () => {
  const image = {
    exif: {},
    width: 6000,
    height: 4000,
    isRaw: true,
    isReady: true,
    path: '/photos/straighten.raw',
    thumbnailUrl: '',
  };
  function setup(ready = true) {
    vi.mocked(invoke).mockReset();
    useEditorStore.getState().setEditor({
      selectedImage: { ...image, isReady: ready, width: ready ? 6000 : 0, height: ready ? 4000 : 0 },
      adjustments: { ...INITIAL_ADJUSTMENTS, aspectRatio: 1.5, crop: null },
      isAutoStraightening: false,
    });
    useLibraryStore.getState().setLibrary({ libraryActivePath: null });
    useUIStore.getState().setUI({ activeView: 'editor' });
    let actions: ReturnType<typeof useEditorActions> | undefined;
    function Probe() {
      actions = useEditorActions();
      return null;
    }
    renderToStaticMarkup(createElement(Probe));
    return actions!;
  }

  it('does not create an edit for a zero correction of the untouched full frame', () => {
    const adjustments = { ...INITIAL_ADJUSTMENTS, aspectRatio: 1.5, crop: null };
    expect(withRotation(adjustments, image, 0)).toBe(adjustments);
  });

  it('does not apply a result to another photo', async () => {
    const actions = setup();
    let resolve!: (value: number) => void;
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const task = actions.handleAutoStraighten();
    const adjustments = { ...INITIAL_ADJUSTMENTS, exposure: 0.75 };
    useEditorStore.getState().setEditor({ selectedImage: { ...image, path: '/photos/another.raw' }, adjustments });
    resolve(5);
    await task;
    expect(useEditorStore.getState().adjustments).toBe(adjustments);
    expect(useEditorStore.getState().isAutoStraightening).toBe(false);
  });

  it('does not overwrite edits made while analysis was running', async () => {
    const actions = setup();
    let resolve!: (value: number) => void;
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const task = actions.handleAutoStraighten();
    const adjustments = { ...useEditorStore.getState().adjustments, exposure: 0.5 };
    useEditorStore.getState().setEditor({ adjustments });
    resolve(5);
    await task;
    expect(useEditorStore.getState().adjustments).toBe(adjustments);
    expect(useEditorStore.getState().isAutoStraightening).toBe(false);
  });

  it('refits the crop using dimensions from the completed image load', async () => {
    const actions = setup(false);
    vi.mocked(invoke).mockResolvedValue(5);
    const task = actions.handleAutoStraighten();
    expect(invoke).not.toHaveBeenCalled();
    useEditorStore.getState().setEditor({ selectedImage: image });
    await task;
    const adjustments = useEditorStore.getState().adjustments;
    expect(adjustments.rotation).toBe(5);
    expect(adjustments.crop).not.toBeNull();
    expect(adjustments.crop!.width).toBeGreaterThan(1000);
    expect(adjustments.crop!.width).toBeLessThanOrEqual(6000);
    expect(adjustments.crop!.height).toBeLessThanOrEqual(4000);
  });

  it('clears a failed loading wait without leaving the action busy forever', async () => {
    vi.useFakeTimers();
    const actions = setup(false);
    const task = actions.handleAutoStraighten();
    await vi.advanceTimersByTimeAsync(60_000);
    await task;
    expect(invoke).not.toHaveBeenCalled();
    expect(useEditorStore.getState().isAutoStraightening).toBe(false);
  });

  it('keeps edits unchanged when no trustworthy lines were found', async () => {
    const actions = setup();
    const before = useEditorStore.getState().adjustments;
    vi.mocked(invoke).mockResolvedValue(null);
    await actions.handleAutoStraighten();
    expect(useEditorStore.getState().adjustments).toBe(before);
    expect(useEditorStore.getState().isAutoStraightening).toBe(false);
  });

  it('straightens the active editor photo in place and batches only the other paths', async () => {
    const actions = setup();
    vi.mocked(invoke).mockResolvedValue(null);
    actions.handleAutoStraightenPaths([image.path, '/photos/other.raw']);
    await Promise.resolve();
    await Promise.resolve();
    expect(
      vi.mocked(invoke).mock.calls.some(([command, args]) => command === 'calculate_auto_straighten' && !!args),
    ).toBe(true);
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'apply_auto_straighten_to_paths')).toEqual([
      ['apply_auto_straighten_to_paths', { paths: ['/photos/other.raw'] }],
    ]);
  });
});

it('does not reload a draft edited while batch processing was in progress', async () => {
  vi.mocked(invoke).mockReset();
  const selectedImage = {
    exif: {},
    width: 6000,
    height: 4000,
    isRaw: true,
    isReady: true,
    path: '/photos/batch.raw',
    thumbnailUrl: '',
  };
  const before = { ...INITIAL_ADJUSTMENTS, exposure: 0 };
  const edited = { ...before, exposure: 1 };
  useEditorStore.getState().setEditor({ selectedImage, adjustments: edited });
  useLibraryStore.getState().setLibrary({ libraryActivePath: null });
  await reloadAdjustmentsForPaths([selectedImage.path], { path: selectedImage.path, adjustments: before });
  expect(invoke).not.toHaveBeenCalled();
  expect(useEditorStore.getState().adjustments).toBe(edited);
});
