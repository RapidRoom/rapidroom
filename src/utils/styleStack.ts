import type { Preset } from '../components/ui/AppProperties';
import { normalizeLoadedAdjustments, type Adjustments } from './adjustments';
import { sameAdjustmentValue } from './editHistory';
import { mixPresetAdjustments } from './presetAmount';

export const STYLE_STACK_LIMIT = 3;
export const STYLE_STACK_LIMIT_MESSAGE = 'Up to 3 Styles. Remove one first.';

export interface StyleEntry {
  presetId: string;
  name: string;
  amount: number;
  adjustments: Partial<Adjustments>;
  bakedIn?: boolean;
}

type Segment = string | { id: string; fallback: unknown };
export interface ManualOverride {
  path: Segment[];
  value?: unknown;
  remove?: boolean;
  order?: string[];
}

export interface StyleStack {
  version: 1;
  base: Adjustments;
  entries: StyleEntry[];
  overrides: ManualOverride[];
  result: Adjustments;
}

const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
const identified = (v: unknown[]): v is Array<Record<string, unknown> & { id: string }> =>
  v.every((item): item is Record<string, unknown> & { id: string } => object(item) && typeof item.id === 'string') &&
  new Set(v.map((item) => item.id)).size === v.length;

function differences(before: unknown, after: unknown, path: Segment[] = []): ManualOverride[] {
  if (sameAdjustmentValue(before, after)) return [];
  if (Array.isArray(before) && Array.isArray(after) && identified(before) && identified(after)) {
    const old = new Map(before.map((item) => [item.id, item]));
    const next = new Map(after.map((item) => [item.id, item]));
    // Masks and their components use IDs, so deleting a preceding Style cannot
    // redirect an edit to a different mask. An edited removed beam becomes user-owned.
    return [
      ...before
        .filter((item) => !next.has(item.id))
        .map((item) => ({ path: [...path, { id: item.id, fallback: item }], remove: true })),
      ...after.flatMap((item) => differences(old.get(item.id), item, [...path, { id: item.id, fallback: item }])),
      ...(before.map((item) => item.id).join('\0') !== after.map((item) => item.id).join('\0')
        ? [{ path, order: after.map((item) => item.id) }]
        : []),
    ];
  }
  if (object(before) && object(after)) {
    return [...new Set([...Object.keys(before), ...Object.keys(after)])].flatMap((key) =>
      Object.hasOwn(after, key)
        ? differences(before[key], after[key], [...path, key])
        : [{ path: [...path, key], remove: true }],
    );
  }
  return [{ path, value: after }];
}

function applyOverride(value: unknown, override: ManualOverride, index = 0): unknown {
  if (index === override.path.length) {
    if (override.order && Array.isArray(value)) {
      const order = override.order;
      const rank = (item: unknown) =>
        object(item) && typeof item.id === 'string' && order.includes(item.id) ? order.indexOf(item.id) : order.length;
      return [...value].sort((a, b) => rank(a) - rank(b));
    }
    return override.value;
  }
  const segment = override.path[index];
  const last = index === override.path.length - 1;
  if (typeof segment !== 'string') {
    const items = Array.isArray(value) ? [...value] : [];
    const found = items.findIndex((item) => object(item) && item.id === segment.id);
    if (last && override.remove) return items.filter((_, i) => i !== found);
    const next = applyOverride(found < 0 ? segment.fallback : items[found], override, index + 1);
    if (found < 0) items.push(next);
    else items[found] = next;
    return items;
  }
  const entries = new Map(Object.entries(object(value) ? value : {}));
  if (last && override.remove) entries.delete(segment);
  else entries.set(segment, applyOverride(entries.get(segment), override, index + 1));
  return Object.fromEntries(entries);
}

const pathKey = (path: Segment[]) => JSON.stringify(path.map((s) => (typeof s === 'string' ? s : { id: s.id })));

export function keepManualStyleEdits(stack: StyleStack, current: Adjustments): StyleStack {
  const changes = differences(stack.result, current);
  if (!changes.length) return stack;
  let overrides = stack.overrides;
  for (const change of changes) {
    const key = pathKey(change.path);
    // Coalesce slider drags and discard older descendants of a replacement.
    overrides = [
      ...overrides.filter((old) => {
        const oldKey = pathKey(old.path);
        return (
          !(oldKey === key && !!old.order === !!change.order) &&
          !(
            !change.order &&
            old.path.length > change.path.length &&
            pathKey(old.path.slice(0, change.path.length)) === key
          )
        );
      }),
      change,
    ];
  }
  return { ...stack, overrides, result: current };
}

export function composeStyleStack(stack: StyleStack, size: { width: number; height: number }): Adjustments {
  let result = stack.base;
  for (const entry of stack.entries) {
    if (entry.bakedIn) continue;
    const mixed = mixPresetAdjustments(
      { presetType: 'style', adjustments: entry.adjustments },
      entry.amount,
      result,
      size,
    );
    const next = { ...result, ...mixed };
    if (entry.amount === 0 && entry.adjustments.masks?.length) {
      // Keep zero-opacity mask geometry while the Style is present, so a
      // manually moved beam fades all the way to zero rather than reappearing
      // from its saved fallback when the mixer omits an Amount0 addition.
      const full = mixPresetAdjustments({ presetType: 'style', adjustments: entry.adjustments }, 100, result, size);
      next.masks = [
        ...result.masks,
        ...(full.masks ?? []).slice(result.masks.length).map((mask) => ({ ...mask, opacity: 0 })),
      ];
    }
    if (next.masks !== result.masks) {
      const normalized = normalizeLoadedAdjustments(next).masks as Adjustments['masks'];
      next.masks = next.masks.map((mask, index) => (result.masks.includes(mask) ? mask : normalized[index]));
    }
    result = next;
  }
  for (const override of stack.overrides) result = applyOverride(result, override) as Adjustments;
  return result;
}

export function changeStyleStack(
  previous: StyleStack | null,
  current: Adjustments,
  change: { preset: Preset; amount: number } | { remove: string },
  size: { width: number; height: number },
): StyleStack {
  const stack = previous
    ? keepManualStyleEdits(previous, current)
    : { version: 1 as const, base: current, entries: [], overrides: [], result: current };
  let entries = [...stack.entries];
  if ('remove' in change) {
    if (!entries.some((entry) => entry.presetId === change.remove)) throw new Error('Style is not applied');
    entries = entries.filter((entry) => entry.presetId !== change.remove);
  } else {
    const { preset, amount } = change;
    if (!Number.isInteger(amount) || amount < 0 || amount > 200)
      throw new Error('Amount must be an integer from 0 to 200');
    const index = entries.findIndex((entry) => entry.presetId === preset.id);
    if (index < 0 && entries.length === STYLE_STACK_LIMIT) throw new Error(STYLE_STACK_LIMIT_MESSAGE);
    if (index >= 0 && entries[index].bakedIn)
      throw new Error('This earlier Style is baked in; its Amount cannot be changed.');
    const adjustments = index < 0 ? structuredClone(preset.adjustments) : entries[index].adjustments;
    if (index < 0 && adjustments.masks?.length) {
      // Reserve each recipe's mask IDs once. Copied presets can share template
      // IDs; removing an earlier Style must never rename a later edited mask.
      const used = new Set(current.masks.flatMap((mask) => [mask.id, ...mask.subMasks.map((sub) => sub.id)]));
      const reserve = (id: string) => {
        const base = id.startsWith(preset.id + ':') ? id : preset.id + ':' + id;
        let candidate = base;
        for (let suffix = 1; used.has(candidate); suffix++) candidate = base + ':' + suffix;
        used.add(candidate);
        return candidate;
      };
      adjustments.masks = adjustments.masks.map((mask) => ({
        ...mask,
        id: reserve(mask.id),
        subMasks: mask.subMasks.map((sub) => ({ ...sub, id: reserve(sub.id) })),
      }));
    }
    const entry =
      index < 0
        ? { presetId: preset.id, name: preset.name, amount, adjustments }
        : { ...entries[index], name: preset.name, amount };
    if (index < 0) entries.push(entry);
    else entries[index] = entry;
  }
  const overrides = stack.overrides.map((override) => {
    let value: unknown = current;
    const path = override.path.map((segment) => {
      if (typeof segment === 'string') {
        value = object(value) ? value[segment] : undefined;
        return segment;
      }
      const found = Array.isArray(value) ? value.find((item) => object(item) && item.id === segment.id) : undefined;
      value = found ?? segment.fallback;
      return found ? { ...segment, fallback: found } : segment;
    });
    return { ...override, path };
  });
  const next = { ...stack, entries, overrides };
  return { ...next, result: composeStyleStack(next, size) };
}

export function migrateAppliedStyle(
  record: { id: string; name: string; amount: number },
  current: Adjustments,
): StyleStack {
  return {
    version: 1,
    base: current,
    entries: [{ presetId: record.id, name: record.name, amount: record.amount, adjustments: {}, bakedIn: true }],
    overrides: [],
    result: current,
  };
}
