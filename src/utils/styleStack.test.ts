import { describe, expect, it } from 'vitest';
import type { Preset } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, type Adjustments } from './adjustments';
import { changeStyleStack, keepManualStyleEdits, migrateAppliedStyle } from './styleStack';
import styles from '../../src-tauri/resources/styles/styles-v1.json';

const size = { width: 6000, height: 4000 };
const base = () => structuredClone(INITIAL_ADJUSTMENTS);
const preset = (id: string, adjustments: Partial<Adjustments>): Preset => ({
  id,
  name: id,
  presetType: 'style',
  adjustments,
});
const first = preset('first', { contrast: 40 });
const second = preset('second', { contrast: -20, shadows: 30 });

describe('ordered Style stack with manual edits', () => {
  it('recomputes surviving Styles in order from the original baseline', () => {
    const initial = { ...base(), exposure: 0.7, contrast: 10 };
    const one = changeStyleStack(null, initial, { preset: first, amount: 100 }, size);
    const two = changeStyleStack(one, one.result, { preset: second, amount: 50 }, size);
    expect(two.result.contrast).toBe(10);
    const removed = changeStyleStack(two, two.result, { remove: first.id }, size);
    expect(removed.entries.map((entry) => entry.presetId)).toEqual(['second']);
    expect(removed.result.contrast).toBe(-5);
    expect(removed.result.exposure).toBe(0.7);
    expect(changeStyleStack(removed, removed.result, { remove: second.id }, size).result).toEqual(initial);
    expect(initial.contrast).toBe(10);
  });

  it('adjusts any entry without reordering, accumulating strength or mutating its saved recipe', () => {
    const one = changeStyleStack(null, base(), { preset: first, amount: 100 }, size);
    const two = changeStyleStack(one, one.result, { preset: second, amount: 50 }, size);
    const half = changeStyleStack(two, two.result, { preset: first, amount: 50 }, size);
    expect(half.entries.map((entry) => entry.presetId)).toEqual(['first', 'second']);
    expect(half.result.contrast).toBe(0);
    const restored = changeStyleStack(half, half.result, { preset: first, amount: 100 }, size);
    expect(restored.result).toEqual(two.result);
    const changedCatalog = { ...first, adjustments: { contrast: -90 } };
    expect(changeStyleStack(two, two.result, { preset: changedCatalog, amount: 100 }, size).result).toEqual(two.result);
  });

  it('rejects a fourth Style atomically, including when the first is at Amount0', () => {
    let stack = changeStyleStack(null, base(), { preset: first, amount: 0 }, size);
    for (const id of ['b', 'c'])
      stack = changeStyleStack(stack, stack.result, { preset: preset(id, {}), amount: 100 }, size);
    const saved = structuredClone(stack);
    expect(() => changeStyleStack(stack, stack.result, { preset: preset('fourth', {}), amount: 100 }, size)).toThrow(
      'Up to 3 Styles. Remove one first.',
    );
    expect(stack).toEqual(saved);
  });

  it('keeps manual Exposure and same-key Contrast through removal and later Amount changes', () => {
    const one = changeStyleStack(null, base(), { preset: first, amount: 100 }, size);
    const two = changeStyleStack(one, one.result, { preset: second, amount: 100 }, size);
    const manual = { ...two.result, exposure: 1.25, contrast: 7 };
    const removed = changeStyleStack(two, manual, { remove: first.id }, size);
    expect(removed.result).toMatchObject({ exposure: 1.25, contrast: 7, shadows: 30 });
    expect(changeStyleStack(removed, removed.result, { preset: second, amount: 0 }, size).result).toMatchObject({
      exposure: 1.25,
      contrast: 7,
      shadows: 0,
    });
  });

  it('preserves nested manual keys without freezing untouched grading channels', () => {
    const look = preset('grade', {
      colorGrading: { ...base().colorGrading, shadows: { hue: 200, saturation: 20, luminance: 0 } },
    });
    const one = changeStyleStack(null, base(), { preset: look, amount: 100 }, size);
    const manual = {
      ...one.result,
      colorGrading: { ...one.result.colorGrading, shadows: { ...one.result.colorGrading.shadows, hue: 45 } },
    };
    const half = changeStyleStack(one, manual, { preset: look, amount: 50 }, size);
    expect(half.result.colorGrading.shadows.hue).toBe(45);
    expect(half.result.colorGrading.shadows.saturation).toBe(10);
  });

  it('keeps a manually moved beam editable and fades untouched opacity; removal preserves the edited mask', () => {
    const forest = styles.presets.find((item) => item.preset.id === 'rapidroom-style:forest-gothic')!.preset as Preset;
    const one = changeStyleStack(null, base(), { preset: forest, amount: 100 }, size);
    const manual = structuredClone(one.result);
    manual.masks[0].subMasks[0].parameters!.centerX = (manual.masks[0].subMasks[0].parameters!.centerX ?? 0) + 120;
    const half = changeStyleStack(one, manual, { preset: forest, amount: 50 }, size);
    expect(half.result.masks[0].subMasks[0].parameters!.centerX).toBe(manual.masks[0].subMasks[0].parameters!.centerX);
    expect(half.result.masks[0].opacity).toBe(one.result.masks[0].opacity / 2);
    const zero = changeStyleStack(half, half.result, { preset: forest, amount: 0 }, size);
    expect(zero.result.masks[0].opacity).toBe(0);
    const removed = changeStyleStack(half, half.result, { remove: forest.id }, size);
    expect(removed.result.masks).toHaveLength(1);
    expect(removed.result.masks[0].opacity).toBe(half.result.masks[0].opacity);
    expect(removed.result.masks[0].subMasks[0].parameters!.centerX).toBe(
      manual.masks[0].subMasks[0].parameters!.centerX,
    );
  });

  it('keeps a later edited mask attached to its Style when copied recipes shared template IDs', () => {
    const forest = styles.presets.find((item) => item.preset.id === 'rapidroom-style:forest-gothic')!.preset as Preset;
    const recipe = structuredClone(forest.adjustments);
    recipe.masks![0].id = 'm'.repeat(128);
    recipe.masks![0].subMasks[0].id = 's'.repeat(128);
    const first = { ...forest, id: 'copy-one / 日本', adjustments: recipe };
    const second = { ...forest, id: 'copy-two / 日本', adjustments: recipe };
    const one = changeStyleStack(null, base(), { preset: first, amount: 100 }, size);
    const two = changeStyleStack(one, one.result, { preset: second, amount: 100 }, size);
    for (const mask of two.result.masks) {
      for (const id of [mask.id, ...mask.subMasks.map((sub) => sub.id)]) expect(id).toMatch(/^[A-Za-z0-9_:.-]{1,128}$/);
    }
    const manual = structuredClone(two.result);
    const id = manual.masks[1].id;
    manual.masks[1].subMasks[0].parameters!.centerX = 1234;
    const removed = changeStyleStack(two, manual, { remove: first.id }, size);
    expect(removed.result.masks).toHaveLength(1);
    expect(removed.result.masks[0].id).toBe(id);
    expect(removed.result.masks[0].subMasks[0].parameters!.centerX).toBe(1234);
  });

  it('keeps a manual mask reorder without freezing untouched opacity or resurrecting removed Style masks', () => {
    const forest = styles.presets.find((item) => item.preset.id === 'rapidroom-style:forest-gothic')!.preset as Preset;
    const one = changeStyleStack(null, base(), { preset: { ...forest, id: 'one-mask' }, amount: 100 }, size);
    const two = changeStyleStack(one, one.result, { preset: { ...forest, id: 'two-mask' }, amount: 100 }, size);
    const user = { ...structuredClone(two.result.masks[0]), id: 'user-mask', subMasks: [] };
    const manual = { ...two.result, masks: [user, two.result.masks[1], two.result.masks[0]] };
    const half = changeStyleStack(two, manual, { preset: { ...forest, id: 'one-mask' }, amount: 50 }, size);
    expect(half.result.masks.map((mask) => mask.id)).toEqual(manual.masks.map((mask) => mask.id));
    expect(half.result.masks[2].opacity).toBe(two.result.masks[0].opacity / 2);
    const removed = changeStyleStack(half, half.result, { remove: 'one-mask' }, size);
    expect(removed.result.masks.map((mask) => mask.id)).toEqual(['user-mask', two.result.masks[1].id]);
  });

  it('does not resurrect a manually deleted mask or overwrite a later user mask', () => {
    const forest = styles.presets.find((item) => item.preset.id === 'rapidroom-style:forest-gothic')!.preset as Preset;
    const one = changeStyleStack(null, base(), { preset: forest, amount: 100 }, size);
    const user = structuredClone(one.result.masks[0]);
    user.id = 'user-mask';
    const manual = { ...one.result, masks: [user] };
    const half = changeStyleStack(one, manual, { preset: forest, amount: 50 }, size);
    expect(half.result.masks).toEqual([user]);
    expect(changeStyleStack(half, half.result, { remove: forest.id }, size).result.masks).toEqual([user]);
  });

  it('coalesces manual slider drags and survives a sidecar JSON round trip', () => {
    let stack = changeStyleStack(null, base(), { preset: first, amount: 100 }, size);
    for (let exposure = 0; exposure < 200; exposure++)
      stack = keepManualStyleEdits(stack, { ...stack.result, exposure: exposure / 100 });
    expect(stack.overrides).toHaveLength(1);
    const reopened = JSON.parse(JSON.stringify(stack));
    expect(changeStyleStack(reopened, reopened.result, { preset: first, amount: 0 }, size).result.exposure).toBe(1.99);
  });

  it('loads a legacy marker as one baked-in entry without changing pixels or inventing a baseline', () => {
    const pixels = { ...base(), exposure: 1, contrast: 47 };
    const legacy = migrateAppliedStyle({ id: 'old', name: 'Earlier Style', amount: 60 }, pixels);
    expect(legacy.result).toBe(pixels);
    expect(legacy.entries).toMatchObject([{ presetId: 'old', amount: 60, bakedIn: true }]);
    expect(() =>
      changeStyleStack(legacy, pixels, { preset: preset('old', { contrast: 90 }), amount: 30 }, size),
    ).toThrow('baked in');
    expect(changeStyleStack(legacy, pixels, { remove: 'old' }, size).result).toEqual(pixels);
  });
});
