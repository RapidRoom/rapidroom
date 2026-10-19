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

describe('Original crop orientation compatibility', () => {
  it('keeps Original during image rotation and flips its resolved ratio with X', () => {
    const selectedImage = {
      exif: {},
      height: 4000,
      isRaw: true,
      isReady: true,
      path: '/photos/original.raw',
      thumbnailUrl: '',
      width: 6000,
    };
    let actions: ReturnType<typeof useEditorActions> | undefined;
    function Probe() {
      actions = useEditorActions();
      return null;
    }
    renderToStaticMarkup(createElement(Probe));
    useEditorStore
      .getState()
      .setEditor({ selectedImage, adjustments: { ...INITIAL_ADJUSTMENTS, aspectRatio: 0, crop: null } });
    actions!.handleRotate(90);
    expect(useEditorStore.getState().adjustments.aspectRatio).toBe(0);
    expect(useEditorStore.getState().adjustments.orientationSteps).toBe(1);

    useEditorStore.getState().setEditor({ adjustments: { ...INITIAL_ADJUSTMENTS, aspectRatio: 0, crop: null } });
    actions!.handleCropOrientationToggle();
    const flipped = useEditorStore.getState().adjustments;
    expect(flipped.aspectRatio).toBeCloseTo(2 / 3);
    expect(flipped.crop!.width / flipped.crop!.height).toBeCloseTo(2 / 3, 2);

    useEditorStore.getState().setEditor({ adjustments: { ...INITIAL_ADJUSTMENTS, aspectRatio: 0.75, crop: null } });
    actions!.handleCropOrientationToggle();
    expect(useEditorStore.getState().adjustments.aspectRatio).toBeCloseTo(4 / 3);
  });
});
