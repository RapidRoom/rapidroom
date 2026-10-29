// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { toast } from 'react-toastify';
import { mockCommand } from '../test/tauriMock';
import { Invokes, Theme, type Preset, type SelectedImage } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, PasteMode, type Adjustments } from '../utils/adjustments';
import { mixAdjustments } from '../utils/presetAmount';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { usePresetCatalog } from '../store/usePresetCatalog';
import { debouncedSave, debouncedSetHistory, useEditorActions } from './useEditorActions';
import { usePresets } from './usePresets';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const editorInitial = useEditorStore.getState();
const libraryInitial = useLibraryStore.getState();
const settingsInitial = useSettingsStore.getState();
const auto = {
  lensCorrectionMode: 'auto',
  cameraLensCorrectionVersion: 1,
  lensMaker: 'Sony',
  lensModel: 'FE 35mm F1.4 GM',
  lensDistortionAmount: 100,
  lensVignetteAmount: 100,
  lensTcaAmount: 100,
  lensDistortionEnabled: true,
  lensVignetteEnabled: true,
  lensTcaEnabled: true,
} as Partial<Adjustments>;
const image = (path: string): SelectedImage => ({
  path,
  width: 6000,
  height: 4000,
  isRaw: true,
  exif: {},
  isReady: true,
  thumbnailUrl: '',
});
const curve = (middle: number) => ({
  ...INITIAL_ADJUSTMENTS.curves,
  luma: [
    { x: 0, y: 0 },
    { x: 128, y: middle },
    { x: 255, y: 255 },
  ],
});
let root: Root | undefined;
let presets: ReturnType<typeof usePresets>;
let actions: ReturnType<typeof useEditorActions>;
let saved: unknown[];
let applied: Array<{ paths: string[]; adjustments: Partial<Adjustments> }>;

beforeEach(() => {
  vi.useFakeTimers();
  useEditorStore.setState(editorInitial, true);
  useLibraryStore.setState(libraryInitial, true);
  useSettingsStore.setState(settingsInitial, true);
  useSettingsStore.setState({ appSettings: { theme: Theme.Dark, lastRootPath: null } });
  usePresetCatalog.setState({ presets: [], isLoading: true, loaded: false });
  saved = [];
  applied = [];
  mockCommand(Invokes.LoadPresets, () => []);
  mockCommand(Invokes.SavePresets, (args) => {
    saved.push(structuredClone(args));
  });
  mockCommand(Invokes.ResetAdjustmentsForPaths, () => undefined);
  mockCommand(Invokes.GetDefaultRawLensAdjustments, () => structuredClone(auto));
  mockCommand(Invokes.ApplyAdjustmentsToPaths, (args) => {
    applied.push(args as (typeof applied)[number]);
  });
  mockCommand(Invokes.LoadMetadata, () => ({ adjustments: useEditorStore.getState().adjustments }));
  vi.spyOn(toast, 'error').mockImplementation(() => 'error');
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  debouncedSetHistory.cancel();
  debouncedSave.cancel();
  useEditorStore.setState(editorInitial, true);
  useLibraryStore.setState(libraryInitial, true);
  useSettingsStore.setState(settingsInitial, true);
  vi.restoreAllMocks();
  vi.useRealTimers();
});

async function mount() {
  function Harness() {
    presets = usePresets(useEditorStore((s) => s.adjustments));
    actions = useEditorActions();
    return null;
  }
  root = createRoot(document.createElement('div'));
  await act(async () => root!.render(createElement(Harness)));
}

async function resetAndCurve() {
  useEditorStore.setState({
    selectedImage: image('/source.ARW'),
    adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 2 },
  });
  await mount();
  await act(async () => {
    actions.handleResetAdjustments(['/source.ARW']);
  });
  expect(useEditorStore.getState().adjustments.lensCorrectionMode).toBe('auto');
  expect(useEditorStore.getState().adjustments.cameraExposureCompensation).toBe(true);
  await act(async () => actions.setAdjustments((before) => ({ ...before, curves: curve(145) })));
}

it('Reset RAW → curve-only Tool create and overwrite capture only curves, even with geometry included', async () => {
  await resetAndCurve();
  let preset!: Preset | null;
  await act(async () => {
    preset = await presets.addPreset('Named curve', null, true, true, 'tool');
  });
  expect(Object.keys(preset!.adjustments)).toEqual(['curves', 'pointCurves', 'parametricCurve', 'curveMode']);
  const target = {
    ...INITIAL_ADJUSTMENTS,
    baselineExposure: false,
    cameraExposureCompensation: false,
    lensCorrectionMode: 'manual',
    cameraLensCorrectionVersion: 0,
  } as Adjustments;
  const mixed = mixAdjustments(preset!.adjustments, 100, target);
  expect(mixed).toMatchObject({
    baselineExposure: false,
    cameraExposureCompensation: false,
    lensCorrectionMode: 'manual',
    cameraLensCorrectionVersion: 0,
    curves: curve(145),
  });
  await act(async () => actions.setAdjustments((before) => ({ ...before, curves: curve(155) })));
  await act(async () => {
    preset = await presets.overwritePreset(preset!.id);
  });
  expect(preset!.adjustments).toEqual({
    curves: curve(155),
    pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
    parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
    curveMode: 'point',
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(501);
  });
  expect(JSON.stringify(saved.at(-1))).not.toContain('cameraLensCorrectionVersion');
  expect(JSON.stringify(saved.at(-1))).not.toContain('cameraExposureCompensation');
});

it('Curve presets capture and overwrite the complete curve state without unrelated edits', async () => {
  await resetAndCurve();
  const parametricCurve = {
    ...INITIAL_ADJUSTMENTS.parametricCurve!,
    luma: { ...INITIAL_ADJUSTMENTS.parametricCurve!.luma, highlights: 40 },
  };
  await act(async () =>
    actions.setAdjustments((before) => ({
      ...before,
      exposure: 3,
      curveMode: 'parametric',
      curves: curve(145),
      pointCurves: curve(135),
      parametricCurve,
    })),
  );
  let preset!: Preset | null;
  await act(async () => {
    preset = await presets.addCurvePreset('Signature curve');
  });
  expect(preset).toMatchObject({ presetType: 'tool', curvePreset: true });
  expect(preset!.adjustments).toEqual({
    curves: curve(145),
    pointCurves: curve(135),
    parametricCurve,
    curveMode: 'parametric',
  });

  await act(async () => actions.setAdjustments((before) => ({ ...before, exposure: 4, curves: curve(155) })));
  await act(async () => {
    preset = await presets.overwritePreset(preset!.id);
  });
  expect(preset!.adjustments).toEqual({
    curves: curve(155),
    pointCurves: curve(135),
    parametricCurve,
    curveMode: 'parametric',
  });
  expect(preset!.adjustments.exposure).toBeUndefined();
});

it('a point curve preset replaces the mode and inactive curves of a parametric target', async () => {
  await resetAndCurve();
  let preset!: Preset | null;
  await act(async () => {
    preset = await presets.addCurvePreset('Point look');
  });
  const target = {
    ...INITIAL_ADJUSTMENTS,
    curveMode: 'parametric',
    curves: curve(90),
    pointCurves: curve(80),
    parametricCurve: {
      ...INITIAL_ADJUSTMENTS.parametricCurve!,
      luma: { ...INITIAL_ADJUSTMENTS.parametricCurve!.luma, highlights: 65 },
    },
  } as Adjustments;
  const mixed = mixAdjustments(preset!.adjustments, 100, target);
  expect(mixed).toMatchObject({
    curveMode: 'point',
    curves: curve(145),
    pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
    parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
  });
});

it('a curve preset captures RGB-only changes with the complete curve state', async () => {
  await resetAndCurve();
  const rgbCurves = {
    ...INITIAL_ADJUSTMENTS.curves,
    red: [
      { x: 0, y: 0 },
      { x: 128, y: 150 },
      { x: 255, y: 255 },
    ],
  };
  await act(async () => actions.setAdjustments((before) => ({ ...before, curves: rgbCurves })));
  let preset!: Preset | null;
  await act(async () => {
    preset = await presets.addCurvePreset('RGB look');
  });
  expect(preset!.adjustments).toEqual({
    curves: rgbCurves,
    pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
    parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
    curveMode: 'point',
  });
});

it('refuses to save an empty curve capture', async () => {
  useEditorStore.setState({
    selectedImage: image('/linear.ARW'),
    adjustments: { ...INITIAL_ADJUSTMENTS },
  });
  await mount();
  let preset: Preset | null = null;
  await act(async () => {
    preset = await presets.addCurvePreset('Empty look');
  });
  expect(preset).toBeNull();
  expect(presets.presets).toHaveLength(0);
});

it('Merge uses the copied source defaults after navigation/settings changes and preserves a differently configured target', async () => {
  await resetAndCurve();
  await act(async () => {
    await actions.handleCopyAdjustments();
  });
  const sourceClipboard = useEditorStore.getState().copiedAdjustments!;
  expect(sourceClipboard.lensCorrectionMode).toBe('auto');
  expect(sourceClipboard.cameraExposureCompensation).toBe(true);
  const target = {
    ...INITIAL_ADJUSTMENTS,
    baselineExposure: false,
    cameraExposureCompensation: false,
    lensCorrectionMode: 'manual',
    cameraLensCorrectionVersion: 0,
  } as Adjustments;
  await act(async () => {
    useEditorStore.setState({ selectedImage: image('/target.ARW'), adjustments: target });
    useSettingsStore.setState({
      appSettings: {
        theme: Theme.Dark,
        lastRootPath: null,
        autoLensCorrectionOnNewRaws: false,
        copyPasteSettings: {
          mode: PasteMode.Merge,
          includedAdjustments: [
            'curves',
            'lensCorrectionMode',
            'lensMaker',
            'lensModel',
            'lensVignetteAmount',
            'exposure',
          ],
          autoSync: false,
          knownAdjustments: [],
        },
      },
    });
    actions.handlePasteAdjustments(['/target.ARW']);
  });
  expect(applied.at(-1)?.adjustments).toEqual({
    curves: curve(145),
    pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
    parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
    curveMode: 'point',
  });
  expect(useEditorStore.getState().adjustments).toMatchObject({
    baselineExposure: false,
    cameraExposureCompensation: false,
    lensCorrectionMode: 'manual',
    cameraLensCorrectionVersion: 0,
  });
});

it.each([0, 1.5])(
  'default-off manual source Merge retains target lens parameters and fixed source ratio%s',
  async (aspectRatio) => {
    mockCommand(Invokes.GetDefaultRawLensAdjustments, () => ({}));
    useEditorStore.setState({
      selectedImage: image('/manual.ARW'),
      adjustments: {
        ...INITIAL_ADJUSTMENTS,
        aspectRatio,
        curves: curve(145),
      },
    });
    await mount();
    await act(async () => {
      await actions.handleCopyAdjustments();
    });
    await act(async () => {
      useEditorStore.setState({
        selectedImage: image('/target.ARW'),
        adjustments: {
          ...INITIAL_ADJUSTMENTS,
          lensCorrectionMode: 'auto',
          cameraLensCorrectionVersion: 1,
          lensDistortionParams: {
            k1: 0.25,
            k2: 0,
            k3: 0,
            model: 0,
            tca_vr: 1,
            tca_vb: 1,
            vig_k1: 0,
            vig_k2: 0,
            vig_k3: 0,
          },
        } as Adjustments,
      });
      actions.handlePasteAdjustments(['/target.ARW']);
    });
    expect(applied.at(-1)?.adjustments).toEqual({
      curves: curve(145),
      pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
      parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
      curveMode: 'point',
      ...(aspectRatio === 0 ? {} : { aspectRatio }),
    });
    expect(useEditorStore.getState().adjustments.lensDistortionParams?.k1).toBe(0.25);
  },
);

it('intentional lens amount and zero-Exposure policy edits retain their policy interpretation', async () => {
  await resetAndCurve();
  await act(async () =>
    actions.setAdjustments((before) => ({
      ...before,
      lensVignetteAmount: 42,
      exposure: 0,
      baselineExposure: false,
      cameraExposureCompensation: false,
    })),
  );
  let preset!: Preset | null;
  await act(async () => {
    preset = await presets.addPreset('Intentional lens and exposure', null, false, true, 'tool');
  });
  expect(preset!.adjustments).toMatchObject({
    lensVignetteAmount: 42,
    lensCorrectionMode: 'auto',
    cameraLensCorrectionVersion: 1,
    exposure: 0,
    baselineExposure: false,
    cameraExposureCompensation: false,
  });
});

it('full Style and Replace retain full-copy policies', async () => {
  await resetAndCurve();
  let preset!: Preset | null;
  await act(async () => {
    preset = await presets.addPreset('Full style', null, true, true, 'style');
  });
  expect(preset!.adjustments).toMatchObject({
    lensCorrectionMode: 'auto',
    cameraLensCorrectionVersion: 1,
    exposure: 0,
    baselineExposure: true,
    cameraExposureCompensation: true,
  });
  await act(async () => {
    await actions.handleCopyAdjustments();
  });
  await act(async () => {
    useEditorStore.setState({
      selectedImage: image('/other.ARW'),
      adjustments: {
        ...INITIAL_ADJUSTMENTS,
        lensCorrectionMode: 'manual',
        baselineExposure: false,
        cameraExposureCompensation: false,
      },
    });
    useSettingsStore.setState({
      appSettings: {
        theme: Theme.Dark,
        lastRootPath: null,
        copyPasteSettings: {
          mode: PasteMode.Replace,
          includedAdjustments: ['lensCorrectionMode', 'exposure'],
          knownAdjustments: [],
          autoSync: false,
        },
      },
    });
    actions.handlePasteAdjustments(['/other.ARW']);
  });
  expect(applied.at(-1)?.adjustments).toEqual({
    lensCorrectionMode: 'auto',
    cameraLensCorrectionVersion: 1,
    exposure: 0,
    baselineExposure: true,
    cameraExposureCompensation: true,
  });
});

it.each([true, false, undefined])(
  'saved Configure preserves policy %s and legacy omission independently of the open photo',
  async (policy) => {
    const adjustments: Partial<Adjustments> = { exposure: 0, curves: curve(150) };
    if (policy !== undefined) {
      adjustments.baselineExposure = policy;
      adjustments.cameraExposureCompensation = policy;
    }
    mockCommand(Invokes.LoadPresets, () => [
      {
        preset: {
          id: 'saved',
          name: 'Saved',
          presetType: 'style',
          includeMasks: false,
          includeCropTransform: false,
          adjustments,
        },
      },
    ]);
    await resetAndCurve();
    await act(async () =>
      actions.setAdjustments((before) => ({
        ...before,
        baselineExposure: policy !== true,
        cameraExposureCompensation: policy !== true,
      })),
    );
    for (const type of ['style', 'tool', 'tool', 'style'] as const) {
      let preset: Preset | null = null;
      await act(async () => {
        preset = presets.configurePreset('saved', 'Renamed', false, false, type);
      });
      for (const key of ['baselineExposure', 'cameraExposureCompensation']) {
        expect(preset!.adjustments[key]).toBe(policy);
        expect(Object.prototype.hasOwnProperty.call(preset!.adjustments, key)).toBe(policy !== undefined);
      }
    }
  },
);

it('failed or stale Tool resolver responses never save a partial preset', async () => {
  await resetAndCurve();
  mockCommand(Invokes.GetDefaultRawLensAdjustments, () => {
    throw new Error('resolver unavailable');
  });
  let result: Preset | null = null;
  await act(async () => {
    result = await presets.addPreset('Unavailable', null, false, true, 'tool');
  });
  expect(result).toBeNull();
  expect(presets.presets).toHaveLength(0);
  expect(toast.error).toHaveBeenCalled();
  let finish!: (value: Partial<Adjustments>) => void;
  mockCommand(
    Invokes.GetDefaultRawLensAdjustments,
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  let pending!: Promise<Preset | null>;
  await act(async () => {
    pending = presets.addPreset('Late', null, false, true, 'tool');
  });
  await act(async () => {
    useEditorStore.setState({ selectedImage: image('/navigated.ARW') });
    finish(auto);
    result = await pending;
  });
  expect(result).toBeNull();
  expect(presets.presets).toHaveLength(0);
});

it('copy failures, stale edits and out-of-order resolver replies retain the previous clipboard', async () => {
  await resetAndCurve();
  await act(async () => {
    await actions.handleCopyAdjustments();
  });
  const prior = useEditorStore.getState().copiedAdjustments;
  mockCommand(Invokes.GetDefaultRawLensAdjustments, () => {
    throw new Error('lookup unavailable');
  });
  await act(async () => {
    await actions.handleCopyAdjustments();
  });
  expect(useEditorStore.getState().copiedAdjustments).toBe(prior);
  const finishes: Array<(value: Partial<Adjustments>) => void> = [];
  mockCommand(Invokes.GetDefaultRawLensAdjustments, () => new Promise((resolve) => finishes.push(resolve)));
  let first!: Promise<void>;
  let second!: Promise<void>;
  await act(async () => {
    first = actions.handleCopyAdjustments();
    second = actions.handleCopyAdjustments();
  });
  await act(async () => {
    finishes[1](auto);
    await second;
  });
  const latest = useEditorStore.getState().copiedAdjustments;
  await act(async () => {
    finishes[0](auto);
    await first;
  });
  expect(useEditorStore.getState().copiedAdjustments).toBe(latest);
  let stale!: Promise<void>;
  await act(async () => {
    stale = actions.handleCopyAdjustments();
  });
  await act(async () => {
    actions.setAdjustments((before) => ({ ...before, curves: curve(170) }));
    finishes[2](auto);
    await stale;
  });
  expect(useEditorStore.getState().copiedAdjustments).toBe(latest);
});

it('Tool creation finishing after unmount cannot write presets', async () => {
  await resetAndCurve();
  let finish!: (value: Partial<Adjustments>) => void;
  mockCommand(
    Invokes.GetDefaultRawLensAdjustments,
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  let pending!: Promise<Preset | null>;
  await act(async () => {
    pending = presets.addPreset('Closed', null, false, true, 'tool');
  });
  await act(async () => {
    root!.unmount();
    root = undefined;
  });
  let result: Preset | null = null;
  await act(async () => {
    finish(auto);
    result = await pending;
    await vi.advanceTimersByTimeAsync(501);
  });
  expect(result).toBeNull();
  expect(saved).toHaveLength(0);
});

it.each([null, { is_null: true }])(
  'unopened Library source %j copies resolved fresh defaults; Merge preserves target and Replace copies full policy',
  async (metadata) => {
    useEditorStore.setState({ selectedImage: null, adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 9 } });
    useLibraryStore.setState({ libraryActivePath: '/source.ARW', multiSelectedPaths: ['/source.ARW'] });
    mockCommand(Invokes.LoadMetadata, () => ({ adjustments: metadata }));
    await mount();
    await act(async () => {
      await actions.handleCopyAdjustments();
    });
    const copied = useEditorStore.getState().copiedAdjustments!;
    expect(copied).toMatchObject({ ...auto, exposure: 0, baselineExposure: true, cameraExposureCompensation: true });
    const target = {
      ...INITIAL_ADJUSTMENTS,
      ...auto,
      lensVignetteAmount: 42,
      baselineExposure: false,
      cameraExposureCompensation: false,
    } as Adjustments;
    await act(async () => {
      useEditorStore.setState({ selectedImage: image('/target.ARW'), adjustments: target });
      actions.handlePasteAdjustments(['/target.ARW']);
    });
    expect(applied).toHaveLength(0);
    expect(useEditorStore.getState().adjustments).toBe(target);
    await act(async () => {
      useSettingsStore.setState({
        appSettings: {
          theme: Theme.Dark,
          lastRootPath: null,
          copyPasteSettings: {
            mode: PasteMode.Replace,
            includedAdjustments: ['exposure', 'lensCorrectionMode', 'lensVignetteAmount'],
            knownAdjustments: [],
            autoSync: false,
          },
        },
      });
      actions.handlePasteAdjustments(['/target.ARW']);
    });
    expect(applied.at(-1)?.adjustments).toEqual({
      exposure: 0,
      baselineExposure: true,
      cameraExposureCompensation: true,
      lensCorrectionMode: 'auto',
      cameraLensCorrectionVersion: 1,
      lensVignetteAmount: 100,
    });
  },
);

it.each([{}, { lensCorrectionMode: 'manual', baselineExposure: false, cameraExposureCompensation: false }])(
  'unopened Library saved object %j retains legacy interpretation instead of fresh defaults',
  async (metadata) => {
    useEditorStore.setState({ selectedImage: null });
    useLibraryStore.setState({ libraryActivePath: '/saved.ARW', multiSelectedPaths: ['/saved.ARW'] });
    mockCommand(Invokes.LoadMetadata, () => ({ adjustments: metadata }));
    await mount();
    await act(async () => {
      await actions.handleCopyAdjustments('/saved.ARW');
    });
    const copied = useEditorStore.getState().copiedAdjustments!;
    expect(copied.lensCorrectionMode).toBe('manual');
    expect(copied.cameraLensCorrectionVersion).toBeUndefined();
    expect(copied.baselineExposure).toBe(false);
    await act(async () => {
      useEditorStore.setState({
        selectedImage: image('/target.ARW'),
        adjustments: { ...INITIAL_ADJUSTMENTS, ...auto } as Adjustments,
      });
      actions.handlePasteAdjustments(['/target.ARW']);
    });
    expect(applied.at(-1)?.adjustments.lensCorrectionMode).toBe('manual');
    expect(useEditorStore.getState().adjustments.lensCorrectionMode).toBe('manual');
    expect(useEditorStore.getState().adjustments.cameraLensCorrectionVersion ?? 0).toBe(0);
  },
);
