import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useEditorStore } from '../store/useEditorStore';
import { INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from './adjustments';
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

  it('tracks Amount0 as a real stack entry with unchanged pixels', () => {
    const before = useEditorStore.getState();
    const commit = vi.fn((adjustments) => useEditorStore.setState({ adjustments }));
    commitPresetApplication(preparePresetApplication(preset, 0), commit);
    expect(commit).toHaveBeenCalledOnce();
    expect(useEditorStore.getState().adjustments).toEqual(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(usePresetApplication.getState().application).toMatchObject({
      amount: 0,
      stackChanged: true,
      stack: { entries: [{ presetId: preset.id, amount: 0 }] },
    });
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

  it('keeps the Amount baseline across favorite and display-name changes', () => {
    const baseline = useEditorStore.getState().adjustments;
    apply(100);
    const renamed = { ...preset, favorite: true, name: 'Renamed Style' };
    commitPresetApplication(preparePresetApplication(renamed, 50), (adjustments) =>
      useEditorStore.setState({ adjustments }),
    );
    expect(useEditorStore.getState().adjustments.exposure).toBe(1.5);
    expect(usePresetApplication.getState().application?.base).toBe(baseline);
    expect(usePresetApplication.getState().application?.preset.name).toBe('Renamed Style');
  });

  it('keeps actual Dark Forest masks exact through the ordinary copy/original save-load path', () => {
    const forest = stylePack.presets.find((entry) => entry.preset.id === 'rapidroom-style:forest-gothic')!
      .preset as unknown as Preset;
    const plan = preparePresetApplication(forest, 100);
    commitPresetApplication(plan, (adjustments) => useEditorStore.setState({ adjustments }));
    const draft = useEditorStore.getState().adjustments;
    const reopened = normalizeLoadedAdjustments(JSON.parse(JSON.stringify(draft)));
    expect(reopened.masks).toEqual(draft.masks);
    expect(draft.masks[0].id).toBe('rapidroom-style:forest-gothic:beam');
    expect(draft.masks[0].subMasks[0].parameters?.centerX).toBe(6000 * 0.23);
    expect(draft.masks[0].adjustments.exposure).toBe(forest.adjustments.masks![0].adjustments.exposure);
    expect(draft.crop).toBe(plan.application.base.crop);
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

  it('rejects a stale prepared result and retains the applied recipe when its catalog definition changes', () => {
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
    const changed = preparePresetApplication({ ...preset, adjustments: { exposure: 3 } }, 50);
    expect(changed.application.stack?.entries[0].adjustments).toEqual(preset.adjustments);
  });
});
