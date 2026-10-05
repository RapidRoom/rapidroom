import { beforeEach, describe, expect, it } from 'vitest';
import { useEditorStore } from '../store/useEditorStore';
import { INITIAL_ADJUSTMENTS } from './adjustments';
import { describeHistoryChange } from './editHistory';

beforeEach(() => useEditorStore.getState().resetHistory(INITIAL_ADJUSTMENTS));

describe('labelled editor history', () => {
  it('retains AI labels across undo/redo and drops the abandoned redo branch', () => {
    const first = { ...INITIAL_ADJUSTMENTS, exposure: 0.4, highlights: -30 };
    const second = { ...first, contrast: 10 };
    const details = describeHistoryChange(INITIAL_ADJUSTMENTS, first, 'assistant');
    expect(details.label).toBe('AI: Exposure +0.4, Highlights -30');
    const store = useEditorStore.getState();
    store.pushHistory(first, details);
    store.pushHistory(second, describeHistoryChange(first, second, 'assistant'));
    store.undo();
    expect(useEditorStore.getState().adjustments).toEqual(first);
    store.redo();
    expect(useEditorStore.getState().historyDetails[1]).toEqual(details);
    store.undo();
    store.pushHistory({ ...first, contrast: 20 });
    expect(useEditorStore.getState().historyDetails).toEqual([null, details, null]);
    expect(useEditorStore.getState().history).toHaveLength(3);
  });

  it('keeps metadata aligned when old history is trimmed, and clears it on photo reset', () => {
    for (let index = 1; index <= 60; index++) {
      const next = { ...INITIAL_ADJUSTMENTS, exposure: index / 100 };
      const store = useEditorStore.getState();
      store.pushHistory(next, describeHistoryChange(store.history.at(-1)!, next, 'assistant'));
    }
    const store = useEditorStore.getState();
    expect(store.history).toHaveLength(50);
    expect(store.historyDetails).toHaveLength(50);
    expect(store.history[0].exposure).toBe(0.11);
    expect(store.historyDetails.every((details) => details?.changedKeys.includes('exposure'))).toBe(true);
    store.resetHistory(INITIAL_ADJUSTMENTS);
    expect(useEditorStore.getState().historyDetails).toEqual([null]);
  });
});
