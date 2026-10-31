import type { Adjustments } from './adjustments';

export interface HistoryDetails {
  actor: 'user' | 'assistant' | 'system' | 'imported';
  id?: string;
  display?: { name: string; delta: string; value: string };
  label: string;
  changedKeys: string[];
  timestamp: number | null;
  source?: 'lightroom-catalog' | 'xmp';
}

export function sameAdjustmentValue(before: unknown, after: unknown): boolean {
  if (before === after) return true;
  if (!before || !after || typeof before !== 'object' || typeof after !== 'object') return false;
  if (Array.isArray(before) || Array.isArray(after)) {
    return (
      Array.isArray(before) &&
      Array.isArray(after) &&
      before.length === after.length &&
      before.every((value, index) => sameAdjustmentValue(value, after[index]))
    );
  }
  const left = before as Record<string, unknown>;
  const right = after as Record<string, unknown>;
  return (
    Object.keys(left).length === Object.keys(right).length &&
    Object.keys(left).every((key) => Object.hasOwn(right, key) && sameAdjustmentValue(left[key], right[key]))
  );
}

const title = (key: string) => key.replace(/([A-Z])/g, ' $1').replace(/^./, (letter) => letter.toUpperCase());
const number = (value: number) => `${value > 0 ? '+' : ''}${Number(value.toFixed(4))}`;
const transition = (before: unknown, after: number) =>
  `${typeof before === 'number' ? number(before) : '—'} → ${number(after)}`;
const channelName = (key: string) => title(key === 'luma' ? 'luma' : key.replace(/s$/, ''));

function nestedChanges(before: unknown, after: unknown, prefix: string): string[] {
  if (!after || typeof after !== 'object' || Array.isArray(after)) return [prefix];
  const old = (before ?? {}) as Record<string, unknown>;
  return Object.entries(after).flatMap(([key, value]) => {
    if (sameAdjustmentValue(old[key], value)) return [];
    const path = `${prefix}${prefix ? ': ' : ''}${title(key)}`;
    return typeof value === 'number'
      ? [`${path} ${transition(old[key], value)}`]
      : typeof value === 'boolean'
        ? [`${path} ${value ? 'on' : 'off'}`]
        : value && typeof value === 'object' && !Array.isArray(value)
          ? nestedChanges(old[key], value, path)
          : [path];
  });
}

function maskChanges(before: Adjustments['masks'], after: Adjustments['masks']): string[] {
  const changes: string[] = [];
  for (const old of before ?? [])
    if (!after.some((mask) => mask.id === old.id)) changes.push(`Delete mask: ${old.name || old.id}`);
  for (const mask of after) {
    const old = before?.find((item) => item.id === mask.id);
    const name = mask.name || mask.id;
    if (!old) {
      changes.push(`Add mask: ${name}`);
      continue;
    }
    if (sameAdjustmentValue(old, mask)) continue;
    const local = nestedChanges(old.adjustments, mask.adjustments, name);
    if (!sameAdjustmentValue(old.subMasks, mask.subMasks)) {
      const modified = mask.subMasks.filter(
        (sub) =>
          !sameAdjustmentValue(
            old.subMasks.find((item) => item.id === sub.id),
            sub,
          ),
      );
      const fields = new Set<string>();
      for (const sub of modified) {
        const previous = old.subMasks.find((item) => item.id === sub.id);
        if (!previous) {
          fields.add(`add ${sub.type}`);
          continue;
        }
        const oldParameters = (previous.parameters ?? {}) as Record<string, unknown>;
        const parameters = (sub.parameters ?? {}) as Record<string, unknown>;
        for (const key of Object.keys(parameters))
          if (!sameAdjustmentValue(oldParameters[key], parameters[key]))
            fields.add(
              key === 'maskDataBase64'
                ? 'AI selection'
                : typeof parameters[key] === 'number'
                  ? `${title(key)} ${transition(oldParameters[key], parameters[key])}`
                  : typeof parameters[key] === 'boolean'
                    ? `${title(key)} ${parameters[key] ? 'on' : 'off'}`
                    : title(key),
            );
        for (const key of ['visible', 'invert', 'opacity', 'mode'] as const)
          if (previous[key] !== sub[key])
            fields.add(
              typeof sub[key] === 'number'
                ? `${title(key)} ${transition(previous[key], sub[key])}`
                : typeof sub[key] === 'boolean'
                  ? `${title(key)} ${sub[key] ? 'on' : 'off'}`
                  : title(key),
            );
      }
      if (mask.subMasks.length < old.subMasks.length) fields.add('remove component');
      local.push(`${name}: ${[...fields].join(', ') || 'mask area'}`);
    }
    for (const key of ['opacity', 'visible', 'invert', 'name'] as const)
      if (mask[key] !== old[key]) {
        const value = mask[key];
        local.push(
          `${name}: ${title(key)} ${typeof value === 'number' ? transition(old[key], value) : typeof value === 'boolean' ? (value ? 'on' : 'off') : String(value)}`,
        );
      }
    changes.push(...local);
  }
  return changes;
}

function cropLabel(after: Adjustments): string {
  if (!after.crop) return 'Crop cleared';
  const ratio = after.aspectRatio || after.crop.width / after.crop.height;
  const standard = [
    [1, 1],
    [3, 4],
    [4, 3],
    [2, 3],
    [3, 2],
    [4, 5],
    [5, 4],
    [9, 16],
    [16, 9],
    [2, 1],
  ].find(([w, h]) => Math.abs(ratio - w / h) < 0.0001);
  return `Crop ${standard ? standard.join(':') : `${Math.round(after.crop.width)} × ${Math.round(after.crop.height)}`}`;
}

export function describeHistoryChange(
  before: Adjustments,
  after: Adjustments,
  actor: HistoryDetails['actor'],
): HistoryDetails {
  const changedKeys = Object.keys(after).filter((key) => !sameAdjustmentValue(before[key], after[key]));
  const changes = changedKeys.flatMap((key): string[] => {
    if (key === 'activeTools' || key === 'sectionVisibility') return [];
    if (key === 'masks') return maskChanges(before.masks, after.masks);
    if (key === 'crop' || key === 'aspectRatio')
      return key === 'aspectRatio' && changedKeys.includes('crop') ? [] : [cropLabel(after)];
    if (['curves', 'pointCurves', 'parametricCurve'].includes(key)) {
      if (key === 'curves' && (changedKeys.includes('pointCurves') || changedKeys.includes('parametricCurve')))
        return [];
      if (!after[key] || typeof after[key] !== 'object') return ['Tone curve'];
      const previous = before[key] as Record<string, unknown> | undefined;
      return Object.entries(after[key] as Record<string, unknown>)
        .filter(([channel, value]) => !sameAdjustmentValue(previous?.[channel], value))
        .map(([channel]) => `Tone curve (${title(channel)})`);
    }
    if (key === 'hsl') {
      return Object.entries(after.hsl).flatMap(([channel, value]) =>
        nestedChanges(before.hsl[channel], value, `HSL: ${channelName(channel)}`),
      );
    }
    if (key === 'colorGrading' || key === 'colorCalibration') return nestedChanges(before[key], after[key], title(key));
    const value = after[key];
    if (typeof value === 'number') return [`${title(key)} ${transition(before[key], value)}`];
    if (typeof value === 'boolean') return [`${title(key)} ${value ? 'on' : 'off'}`];
    return [title(key)];
  });
  const distinct = [...new Set(changes)];
  const numericKey = changedKeys.length === 1 ? changedKeys[0] : undefined;
  const numericBefore = numericKey ? before[numericKey] : undefined;
  const numericAfter = numericKey ? after[numericKey] : undefined;
  return {
    actor,
    ...(numericKey && typeof numericBefore === 'number' && typeof numericAfter === 'number'
      ? {
          display: {
            name: title(numericKey),
            delta: number(numericAfter - numericBefore),
            value: number(numericAfter),
          },
        }
      : {}),
    label: `${actor === 'assistant' ? 'AI: ' : ''}${distinct.slice(0, 6).join(', ') || 'Adjustment'}${distinct.length > 6 ? ` (+${distinct.length - 6} more)` : ''}`,
    changedKeys,
    timestamp: Date.now(),
  };
}

export function historyMenuPlacement(
  anchor: { left: number; right: number; top: number; bottom: number },
  viewportWidth: number,
  viewportHeight: number,
) {
  const width = Math.max(1, Math.min(384, viewportWidth * 0.9, viewportWidth - 16));
  const maxHeight = Math.max(1, Math.min(320, viewportHeight - 16));
  const left = Math.max(8, Math.min(anchor.right - width, viewportWidth - width - 8));
  const below = viewportHeight - anchor.bottom - 12;
  const top = below >= Math.min(160, maxHeight) ? anchor.bottom + 12 : Math.max(8, anchor.top - maxHeight - 12);
  return { left, top, width, maxHeight: Math.min(maxHeight, Math.max(1, viewportHeight - top - 8)) };
}
