import { beforeEach, describe, expect, it } from 'vitest';
import { mockCommand } from '../test/tauriMock';
import { loadPresetCatalog, setCatalogPresets, usePresetCatalog } from '../store/usePresetCatalog';
import { useEditorStore } from '../store/useEditorStore';
import { listMcpPresets, resolveMcpPreset } from './mcpPresets';

beforeEach(() => {
  usePresetCatalog.setState({ presets: [], loaded: false, isLoading: true });
  useEditorStore.setState({ selectedImage: null });
});

describe('live preset catalog discovery and exact selectors', () => {
  it('initializes once with panel closed and uses opaque selectable IDs for legacy paths/hostile names', async () => {
    mockCommand('load_presets', () => [
      {
        folder: {
          id: 'legacy/group',
          name: 'Group',
          children: [{ id: '../legacy/sk-secret', name: '☂ secret=hidden', adjustments: {} }],
        },
      },
    ]);
    const page = await listMcpPresets({});
    expect(page.total).toBe(1);
    expect(page.presets[0]).toMatchObject({ availability: 'unknown', name: '☂ secret=hidden' });
    expect(page.presets[0].id).toMatch(/^preset:[0-9a-f]{64}$/);
    expect(page.presets[0].groupId).toMatch(/^group:[0-9a-f]{64}$/);
    const match = await resolveMcpPreset({ presetId: page.presets[0].id });
    expect(match.preset.id).toBe('../legacy/sk-secret');
    setCatalogPresets([]);
    await expect(resolveMcpPreset({ presetId: page.presets[0].id })).rejects.toThrow('not found');
  });

  it('preserves pending GUI changes when initialization finishes late', async () => {
    let finish!: (value: unknown) => void;
    mockCommand(
      'load_presets',
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const pending = loadPresetCatalog();
    await Promise.resolve();
    await Promise.resolve();
    setCatalogPresets([{ preset: { id: 'current', name: 'Renamed in GUI', adjustments: {} } }]);
    finish([{ preset: { id: 'stale', name: 'Old disk name', adjustments: {} } }]);
    await pending;
    expect((await listMcpPresets({})).presets[0].name).toBe('Renamed in GUI');
  });

  it('refuses duplicate names/IDs and never substitutes a missing preset', async () => {
    setCatalogPresets([
      { preset: { id: 'a', name: 'Same', adjustments: {} } },
      { preset: { id: 'b', name: 'Same', adjustments: {} } },
    ]);
    await expect(resolveMcpPreset({ name: 'Same' })).rejects.toThrow('Ambiguous');
    await expect(resolveMcpPreset({ name: 'same' })).rejects.toThrow('not found');
    setCatalogPresets([
      { preset: { id: 'a', name: 'One', adjustments: {} } },
      { preset: { id: 'a', name: 'Two', adjustments: {} } },
    ]);
    await expect(resolveMcpPreset({ name: 'One' })).rejects.toThrow('Duplicate');
  });
});
