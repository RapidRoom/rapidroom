import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useEditorStore } from '../store/useEditorStore';
import { INITIAL_ADJUSTMENTS } from './adjustments';
import { commitPresetApplication, preparePresetApplication, usePresetApplication } from './presetApplication';
import { mockCommand } from '../test/tauriMock';
import stylePack from '../../src-tauri/resources/styles/styles-v1.json';
import type { Preset } from '../components/ui/AppProperties';

const image = { path: '/photo.raw', isReady: true, width: 6000, height: 4000, exif: {}, isRaw: true, thumbnailUrl: '' };
const preset: Preset = { id: 'one', name: 'One', presetType: 'style', adjustments: { exposure: 2, contrast: 30 } };
const original = useEditorStore.getState();
const apply = (amount: number) =>
  commitPresetApplication(preparePresetApplication(preset, amount), (adjustments) =>
    useEditorStore.setState({ adjustments }),
  );

beforeEach(() => {
  useEditorStore.setState(original, true);
  usePresetApplication.setState({ application: null });
  mockCommand('save_applied_preset', () => undefined);
  useEditorStore.setState({
    selectedImage: image,
    adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 1, crop: { x: 10, y: 20, width: 2000, height: 3000, unit: 'px' } },
  });
});

describe('shared GUI/MCP preset Amount baseline', () => {
  it('recomputes 100→50→100 and 0/200 from the captured nondefault edit, preserving unrelated crop', () => {
    const base = useEditorStore.getState().adjustments;
    apply(100);
    const full = useEditorStore.getState().adjustments;
    apply(50);
    expect(useEditorStore.getState().adjustments.exposure).toBe(1.5);
    apply(100);
    expect(useEditorStore.getState().adjustments).toEqual(full);
    apply(0);
    expect(useEditorStore.getState().adjustments).toEqual(base);
    apply(200);
    expect(useEditorStore.getState().adjustments.exposure).toBe(3);
    expect(useEditorStore.getState().adjustments.crop).toEqual(base.crop);
    expect(usePresetApplication.getState().application?.base).toBe(base);
  });

  it('tracks a zero/no-op Amount without artificial GUI/MCP history', () => {
    const before = useEditorStore.getState();
    const commit = vi.fn();
    commitPresetApplication(preparePresetApplication(preset, 0), commit);
    expect(commit).not.toHaveBeenCalled();
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(usePresetApplication.getState().application).toMatchObject({ amount: 0, result: before.adjustments });
    apply(50);
    expect(useEditorStore.getState().adjustments.exposure).toBe(1.5);
  });

  it('reapplies an actual additive masked Style without duplicating masks or changing prior mask geometry', () => {
    const forest = stylePack.presets.find((entry) => entry.preset.id === 'rapidroom-style:forest-gothic')!
      .preset as unknown as Preset;
    const prior = structuredClone(forest.adjustments.masks![0]);
    prior.id = 'prior-user-mask';
    useEditorStore.setState({ adjustments: { ...useEditorStore.getState().adjustments, masks: [prior] } });
    const run = (amount: number) =>
      commitPresetApplication(preparePresetApplication(forest, amount), (adjustments) =>
        useEditorStore.setState({ adjustments }),
      );
    run(100);
    const full = structuredClone(useEditorStore.getState().adjustments);
    run(50);
    expect(useEditorStore.getState().adjustments.masks).toHaveLength(2);
    expect(useEditorStore.getState().adjustments.masks[0]).toEqual(prior);
    expect(useEditorStore.getState().adjustments.masks[1].opacity).toBe(full.masks[1].opacity / 2);
    run(100);
    expect(useEditorStore.getState().adjustments).toEqual(full);
  });

  it('invalidates without a mounted panel on a user edit, image switch, undo and reset', () => {
    for (const change of ['edit', 'switch', 'undo', 'reset']) {
      useEditorStore.setState({ selectedImage: image, historyIndex: 1 });
      apply(100);
      if (change === 'edit')
        useEditorStore.setState({ adjustments: { ...useEditorStore.getState().adjustments, shadows: 15 } });
      if (change === 'switch') useEditorStore.setState({ selectedImage: { ...image, path: '/other.raw' } });
      if (change === 'undo') useEditorStore.setState({ historyIndex: 0 });
      if (change === 'reset') useEditorStore.setState({ adjustments: { ...INITIAL_ADJUSTMENTS } });
      expect(usePresetApplication.getState().application).toBeNull();
    }
  });

  it('rejects a stale prepared result or changed preset before touching the marker or edit', () => {
    apply(100);
    const plan = preparePresetApplication(preset, 50);
    useEditorStore.setState({ adjustments: { ...useEditorStore.getState().adjustments, exposure: 0.2 } });
    const before = useEditorStore.getState().adjustments;
    expect(() =>
      commitPresetApplication(plan, () => {
        throw new Error('must not run');
      }),
    ).toThrow('changed');
    expect(useEditorStore.getState().adjustments).toBe(before);
    expect(usePresetApplication.getState().application).toBeNull();
    apply(100);
    expect(() => preparePresetApplication({ ...preset, adjustments: { exposure: 3 } }, 50)).toThrow(
      'definition changed',
    );
  });
});
