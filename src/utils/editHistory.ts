import type { Adjustments } from './adjustments';

export interface HistoryDetails {
  actor: 'user' | 'assistant';
  label: string;
  changedKeys: string[];
  timestamp: number;
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

export function describeHistoryChange(
  before: Adjustments,
  after: Adjustments,
  actor: HistoryDetails['actor'],
): HistoryDetails {
  const changedKeys = Object.keys(after).filter((key) => !sameAdjustmentValue(before[key], after[key]));
  const changes = changedKeys.map((key) => {
    const title = key.replace(/([A-Z])/g, ' $1').replace(/^./, (value) => value.toUpperCase());
    const oldValue = before[key];
    const value = after[key];
    if (typeof oldValue === 'number' && typeof value === 'number') {
      const delta = Number((value - oldValue).toFixed(4));
      return `${title} ${delta >= 0 ? '+' : ''}${delta}`;
    }
    return title;
  });
  return {
    actor,
    label: `${actor === 'assistant' ? 'AI: ' : ''}${changes.slice(0, 3).join(', ') || 'Adjustment'}${changes.length > 3 ? ` (+${changes.length - 3} more)` : ''}`,
    changedKeys,
    timestamp: Date.now(),
  };
}
