import type { Preset } from '../components/ui/AppProperties';
import type { UserPreset } from '../hooks/usePresets';

export function presetUnavailableReason(preset: Preset, cameraModel?: string): string | null {
  if (preset.unavailableReason) return preset.unavailableReason;
  if (
    preset.cameraModelRestriction &&
    preset.cameraModelRestriction.toLocaleLowerCase() !== cameraModel?.toLocaleLowerCase()
  ) {
    return `Requires camera: ${preset.cameraModelRestriction}`;
  }
  return null;
}

export function filterPresetLibrary(
  items: UserPreset[],
  query: string,
  compatibleOnly: boolean,
  cameraModel?: string,
): UserPreset[] {
  const needle = query.trim().toLocaleLowerCase();
  const matches = (preset: Preset, group = '') =>
    (!compatibleOnly || !presetUnavailableReason(preset, cameraModel)) &&
    (!needle || `${group} ${preset.name}`.toLocaleLowerCase().includes(needle));
  return items.flatMap((item) => {
    if (item.preset) return matches(item.preset) ? [item] : [];
    if (!item.folder) return [];
    const children = item.folder.children.filter((preset: Preset) => matches(preset, item.folder!.name));
    return children.length ? [{ folder: { ...item.folder, children } }] : [];
  });
}
