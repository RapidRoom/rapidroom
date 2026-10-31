import { v4 as uuid } from 'uuid';
import type { Adjustments } from './adjustments';
import { sameAdjustmentValue, type HistoryDetails } from './editHistory';

export interface HistorySnapshot {
  id: string;
  name: string;
  createdAt: number;
  adjustments: Adjustments;
}
export interface SavedHistory {
  version: number;
  currentStepId: string;
  compactedSteps: number;
  steps: Array<HistoryDetails & { id: string; adjustments: Adjustments }>;
  snapshots: HistorySnapshot[];
}
export interface HistoryRead {
  history: SavedHistory | null;
  warning: string | null;
  writable: boolean;
}

// Bound by UTF-8 bytes, without splitting a Unicode scalar value.
function boundedLabel(label: string): string {
  let result = '';
  const encoder = new TextEncoder();
  for (const character of label) {
    if (encoder.encode(result + character).length > 240) break;
    result += character;
  }
  return result;
}

export function serializeHistory(
  state: {
    history: Adjustments[];
    historyDetails: Array<HistoryDetails | null>;
    historyIndex: number;
    historySnapshots: HistorySnapshot[];
    historyCompacted: number;
  },
  adjustments: Adjustments,
): SavedHistory | null {
  if (!sameAdjustmentValue(state.history[state.historyIndex], adjustments)) return null;
  const steps = state.history.map((value, index) => {
    const details = state.historyDetails[index];
    return {
      id: details?.id ?? uuid(),
      label: boundedLabel(details?.label ?? 'Initial state'),
      actor: details?.actor ?? 'system',
      timestamp: details ? details.timestamp : Date.now(),
      changedKeys: details?.changedKeys ?? [],
      ...(details?.display ? { display: details.display } : {}),
      ...(details?.source ? { source: details.source } : {}),
      adjustments: value,
    };
  });
  return {
    version: 1,
    currentStepId: steps[state.historyIndex].id,
    compactedSteps: state.historyCompacted,
    steps,
    snapshots: state.historySnapshots,
  };
}
