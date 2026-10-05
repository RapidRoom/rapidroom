import type { Adjustments } from './adjustments';

export interface HistoryDetails {
  actor: 'user' | 'assistant';
  label: string;
  changedKeys: string[];
  timestamp: number;
}

export function describeHistoryChange(
  before: Adjustments,
  after: Adjustments,
  actor: HistoryDetails['actor'],
): HistoryDetails {
  const changedKeys = Object.keys(after).filter((key) => JSON.stringify(before[key]) !== JSON.stringify(after[key]));
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
