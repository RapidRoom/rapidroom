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

  it.each([0, 1, 2, 3])('rotates a saved percent ROI like its pixel frame at orientation %i', (orientationSteps) => {
    const width = orientationSteps % 2 ? image.height : image.width;
    const height = orientationSteps % 2 ? image.width : image.height;
    const percent = {
      ...INITIAL_ADJUSTMENTS,
      orientationSteps,
      aspectRatio: (width * 0.3) / (height * 0.6),
      crop: { unit: '%' as const, x: 10, y: 20, width: 30, height: 60 },
    };
    const pixels = {
      ...percent,
      crop: { unit: 'px' as const, x: width * 0.1, y: height * 0.2, width: width * 0.3, height: height * 0.6 },
    };
    expect(withRotation(percent, image, 1).crop).toEqual(withRotation(pixels, image, 1).crop);
    expect(withRotation(percent, image, 1).crop?.width).toBeGreaterThan(1000);
    expect(withRotation(percent, image, percent.rotation)).toBe(percent);
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

describe('labelled async editor actions', () => {
  const path = '/photos/history.raw';
  function setup() {
    vi.useFakeTimers();
    vi.mocked(invoke).mockClear();
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
      if (command === 'get_default_raw_lens_adjustments')
        await new Promise<void>((resolve) => {
          finish = resolve;
        });
      if (command === 'get_default_raw_lens_adjustments')
        return { lensCorrectionMode: 'auto', cameraLensCorrectionVersion: 1 };
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
  it.each(['edit', 'navigate'])('discards Auto adjustments after a later %s', async (race) => {
    let finish!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'calculate_auto_adjustments')
        return new Promise((resolve) => {
          finish = resolve;
        });
      return {};
    });
    const actions = setup();
    const operation = actions.handleAutoAdjustments();
    const latest = { ...INITIAL_ADJUSTMENTS, exposure: 2.5 };
    useEditorStore.getState().resetHistory(latest);
    if (race === 'navigate')
      useEditorStore.setState({
        selectedImage: { ...useEditorStore.getState().selectedImage!, path: '/photos/other.raw' },
      });
    finish({ exposure: 0.4 });
    await operation;
    expect(useEditorStore.getState().adjustments).toBe(latest);
    expect(useEditorStore.getState().historyDetails.some((entry) => entry?.label === 'Auto adjustments')).toBe(false);
  });
  it('keeps the live draft when Auto lens resolves from an older saved recipe', async () => {
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'get_default_raw_lens_adjustments'
        ? { exposure: 0, lensDistortionAmount: 100, lensDistortionEnabled: true }
        : {},
    );
    const actions = setup();
    actions.setAdjustments({ exposure: 0.7, lensDistortionAmount: 37 });
    const draft = useEditorStore.getState().adjustments;
    actions.handleAutoLensCorrection([path]);
    await vi.advanceTimersByTimeAsync(600);
    expect(useEditorStore.getState().adjustments.exposure).toBe(draft.exposure);
    expect(useEditorStore.getState().adjustments.lensDistortionAmount).toBe(37);
    expect(invoke).toHaveBeenCalledWith('get_default_raw_lens_adjustments', { path, forceAuto: true });
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === 'apply_auto_lens_correction_to_paths')).toBe(
      false,
    );
  });
  it('never sends the active draft to Reset or Auto lens bulk sidecar writers', async () => {
    vi.mocked(invoke).mockImplementation(async () => ({}));
    const actions = setup();
    actions.setAdjustments({ exposure: 0.7 });
    actions.handleResetAdjustments([path, '/photos/library.raw']);
    await vi.advanceTimersByTimeAsync(0);
    actions.handleAutoLensCorrection([path, '/photos/library.raw']);
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledWith('reset_adjustments_for_paths', { paths: ['/photos/library.raw'] });
    expect(invoke).toHaveBeenCalledWith('apply_auto_lens_correction_to_paths', { paths: ['/photos/library.raw'] });
    for (const [command, args] of vi.mocked(invoke).mock.calls) {
      if (command === 'reset_adjustments_for_paths' || command === 'apply_auto_lens_correction_to_paths')
        expect((args as { paths: string[] }).paths).not.toContain(path);
    }
  });

  it.each(['edit', 'navigate'])('discards late Reset defaults after a later %s', async (race) => {
    let finish!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'get_default_raw_lens_adjustments'
        ? new Promise((resolve) => {
            finish = resolve;
          })
        : {},
    );
    const actions = setup();
    actions.handleResetAdjustments([path]);
    const latest = { ...INITIAL_ADJUSTMENTS, exposure: 2.5 };
    useEditorStore.getState().resetHistory(latest);
    if (race === 'navigate')
      useEditorStore.setState({
        selectedImage: { ...useEditorStore.getState().selectedImage!, path: '/photos/b.raw' },
      });
    finish({ lensCorrectionMode: 'auto' });
    await vi.advanceTimersByTimeAsync(600);
    expect(useEditorStore.getState().adjustments).toBe(latest);
    expect(useEditorStore.getState().historyDetails.some((entry) => entry?.label === 'Reset adjustments')).toBe(false);
  });
});

describe('Original crop orientation compatibility', () => {
  it('flips a saved percentage ROI around the same image-space centre as pixels', () => {
    let actions: ReturnType<typeof useEditorActions> | undefined;
    function Probe() {
      actions = useEditorActions();
      return null;
    }
    renderToStaticMarkup(createElement(Probe));
    const selectedImage = {
      exif: {},
      height: 4000,
      isRaw: true,
      isReady: true,
      path: '/photos/percent.raw',
      thumbnailUrl: '',
      width: 6000,
    };
    const frames = [
      { unit: '%' as const, x: 10, y: 20, width: 30, height: 60 },
      { unit: 'px' as const, x: 600, y: 800, width: 1800, height: 2400 },
    ];
    const outputs = frames.map((crop) => {
      useEditorStore
        .getState()
        .setEditor({ selectedImage, adjustments: { ...INITIAL_ADJUSTMENTS, aspectRatio: 0.75, crop } });
      actions!.handleCropOrientationToggle();
      return useEditorStore.getState().adjustments;
    });
    expect(outputs[0].crop).toEqual(outputs[1].crop);
    expect(outputs[0].aspectRatio).toBeCloseTo(4 / 3);
    expect(outputs[0].crop!.width).toBeGreaterThan(1000);
  });

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
