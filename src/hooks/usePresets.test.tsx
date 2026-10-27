// @vitest-environment jsdom
import { createElement, act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { usePresets } from './usePresets';
import { mockCommand } from '../test/tauriMock';
import { Invokes, type Preset } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from '../utils/adjustments';
import { mixAdjustments } from '../utils/presetAmount';
import { usePresetCatalog } from '../store/usePresetCatalog';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: ReturnType<typeof createRoot> | undefined;
beforeEach(() => {
  vi.useFakeTimers();
  usePresetCatalog.setState({ presets: [], isLoading: true, loaded: false });
});
afterEach(async () => {
  await act(async () => {
    await vi.runOnlyPendingTimersAsync();
    root?.unmount();
  });
  root = undefined;
  vi.useRealTimers();
});

async function mount(preset: Preset) {
  mockCommand(Invokes.LoadPresets, () => [{ preset }]);
  mockCommand(Invokes.SavePresets, () => undefined);
  const before = normalizeLoadedAdjustments({ exposure: 0 } as typeof INITIAL_ADJUSTMENTS);
  let hook!: ReturnType<typeof usePresets>;
  function Harness() {
    hook = usePresets(before);
    return null;
  }
  root = createRoot(document.createElement('div'));
  await act(async () => root!.render(createElement(Harness)));
  return { hook: () => hook, before };
}

it.each(['style', 'tool'] as const)(
  'configuring a legacy %s preset preserves its omitted eligibility and applied exposure',
  async (presetType) => {
    const preset: Preset = { id: 'legacy', name: 'Old name', adjustments: { exposure: 1 }, presetType };
    const { hook, before } = await mount(preset);
    const original = mixAdjustments(preset.adjustments, 100, before);
    let renamed!: Preset;
    await act(async () => {
      renamed = hook().configurePreset('legacy', 'New name', false, false, presetType)!;
    });
    expect(renamed.adjustments).toEqual(preset.adjustments);
    expect(mixAdjustments(renamed.adjustments, 100, before)).toEqual(original);
  },
);

it.each(['style', 'tool'] as const)('converting a legacy %s preset preserves omitted policy', async (presetType) => {
  const preset: Preset = { id: 'legacy', name: 'Legacy', adjustments: { exposure: 1 }, presetType };
  const { hook, before } = await mount(preset);
  const original = mixAdjustments(preset.adjustments, 100, before);
  let converted!: Preset;
  await act(async () => {
    converted = hook().configurePreset('legacy', 'Converted', false, false, presetType === 'style' ? 'tool' : 'style')!;
  });
  expect(converted.adjustments.baselineExposure).toBeUndefined();
  expect(converted.adjustments.cameraExposureCompensation).toBeUndefined();
  const applied = mixAdjustments(converted.adjustments, 100, before);
  expect(applied.baselineExposure).toBe(original.baselineExposure);
  expect(applied.cameraExposureCompensation).toBe(original.cameraExposureCompensation);
  expect(applied.exposure).toBe(original.exposure);
});

it.each([
  { baselineExposure: false, cameraExposureCompensation: false },
  { baselineExposure: true, cameraExposureCompensation: false },
  { baselineExposure: true, cameraExposureCompensation: true },
])('style/tool conversion retains explicitly saved policy %j even when zero Exposure is filtered', async (policy) => {
  const preset: Preset = {
    id: 'saved',
    name: 'Saved style',
    presetType: 'style',
    adjustments: { exposure: 0, ...policy },
  };
  const { hook, before } = await mount(preset);
  const original = mixAdjustments(preset.adjustments, 100, before);
  let tool!: Preset;
  await act(async () => {
    tool = hook().configurePreset('saved', 'Tool', false, false, 'tool')!;
  });
  expect(tool.adjustments).toEqual(policy);
  const appliedTool = mixAdjustments(tool.adjustments, 100, before);
  expect(appliedTool.baselineExposure).toBe(original.baselineExposure);
  expect(appliedTool.cameraExposureCompensation).toBe(original.cameraExposureCompensation);
  let style!: Preset;
  await act(async () => {
    style = hook().configurePreset('saved', 'Style again', false, false, 'style')!;
  });
  const reapplied = mixAdjustments(style.adjustments, 100, before);
  expect(reapplied.baselineExposure).toBe(original.baselineExposure);
  expect(reapplied.cameraExposureCompensation).toBe(original.cameraExposureCompensation);
  expect(reapplied.exposure).toBe(original.exposure);
});
