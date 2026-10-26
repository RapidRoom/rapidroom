import { usePresetCatalog, loadPresetCatalog } from '../store/usePresetCatalog';
import type { Preset } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { ADJUSTMENT_GROUPS } from './adjustments';
import { presetUnavailableReason } from './presetBrowser';

export interface PresetSelector {
  presetId?: string;
  name?: string;
  groupId?: string;
  amount?: number;
  query?: string;
  presetType?: 'style' | 'tool';
  offset?: number;
  limit?: number;
}

// Catalog selectors are stable, opaque handles, not filesystem paths. This also
// keeps legacy slash IDs and hostile/secret-looking imported IDs selectable.
export async function presetCatalogSelector(prefix: string, id: string): Promise<string> {
  const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(id));
  return `${prefix}:${Array.from(new Uint8Array(hash), (b) => b.toString(16).padStart(2, '0')).join('')}`;
}

export async function presetCatalogEntries() {
  await loadPresetCatalog();
  const snapshot = usePresetCatalog.getState().presets;
  const entries = await Promise.all(
    snapshot.flatMap((item) => {
      const children = item.preset ? [item.preset] : (item.folder?.children ?? []);
      return children.map(async (preset) => ({
        preset,
        id: await presetCatalogSelector('preset', preset.id),
        groupId: item.folder ? await presetCatalogSelector('group', item.folder.id ?? '') : null,
        groupName: item.folder?.name ?? null,
      }));
    }),
  );
  if (usePresetCatalog.getState().presets !== snapshot) throw new Error('Preset catalog changed during lookup; retry');
  return { entries, snapshot };
}

export async function listMcpPresets(options: PresetSelector) {
  const { entries } = await presetCatalogEntries();
  const image = useEditorStore.getState().selectedImage;
  const camera = image?.isReady ? image.exif?.Model : undefined;
  const query = options.query?.toLocaleLowerCase() ?? '';
  const matches = entries.filter(
    ({ preset, groupId, groupName }) =>
      (!options.groupId || options.groupId === groupId) &&
      (!options.presetType || (preset.presetType ?? 'style') === options.presetType) &&
      (!query || `${preset.name} ${preset.description ?? ''} ${groupName ?? ''}`.toLocaleLowerCase().includes(query)),
  );
  const offset = options.offset ?? 0;
  const limit = options.limit ?? 10;
  return {
    imagePath: image?.isReady ? image.path : null,
    cameraModel: camera ?? null,
    total: matches.length,
    offset,
    hasMore: offset + limit < matches.length,
    amountRange: { minimum: 0, maximum: 200, default: 100 },
    presets: matches.slice(offset, offset + limit).map(({ preset, id, groupId, groupName }) => ({
      id,
      name: preset.name.slice(0, 128),
      description: preset.description?.slice(0, 256) ?? null,
      groupId,
      groupName: groupName?.slice(0, 128) ?? null,
      presetType: preset.presetType ?? 'style',
      favorite: preset.favorite ?? false,
      includesMasks: preset.includeMasks ?? Boolean(preset.adjustments?.masks?.length),
      includesGeometry:
        preset.includeCropTransform ??
        ADJUSTMENT_GROUPS.geometry.some((g) => g.keys.some((key) => preset.adjustments?.[key] !== undefined)),
      availability: image?.isReady
        ? presetUnavailableReason(preset, camera)
          ? 'unavailable'
          : 'available'
        : 'unknown',
      reason: image?.isReady ? presetUnavailableReason(preset, camera) : 'Camera compatibility requires a loaded image',
    })),
  };
}

export async function resolveMcpPreset(options: PresetSelector): Promise<{
  preset: Preset;
  id: string;
  groupId: string | null;
  snapshot: ReturnType<typeof usePresetCatalog.getState>['presets'];
}> {
  const { entries, snapshot } = await presetCatalogEntries();
  const matches = entries.filter(
    (entry) =>
      (options.presetId ? entry.id === options.presetId : entry.preset.name === options.name) &&
      (!options.groupId || entry.groupId === options.groupId),
  );
  if (matches.length !== 1)
    throw new Error(
      matches.length
        ? `Ambiguous preset selector; candidate IDs: ${matches
            .slice(0, 20)
            .map((entry) => entry.id)
            .join(', ')}`
        : 'Preset not found in the live catalog',
    );
  const selected = matches[0];
  if (entries.filter((entry) => entry.preset.id === selected.preset.id).length !== 1)
    throw new Error('Duplicate preset IDs in the catalog; repair the library before applying');
  return { ...selected, snapshot };
}
