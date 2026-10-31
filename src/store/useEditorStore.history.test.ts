import { afterEach, describe, expect, it } from 'vitest';
import type { SelectedImage } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS, type Adjustments } from '../utils/adjustments';
import { useEditorStore } from './useEditorStore';

const initialEditorState = useEditorStore.getState();

afterEach(() => {
  useEditorStore.setState(initialEditorState, true);
});

function openPhoto(path: string, adjustments: Adjustments) {
  useEditorStore.getState().setEditor({ selectedImage: { path } as SelectedImage });
  useEditorStore.getState().restoreHistory(adjustments);
}

describe('per-photo edit history', () => {
  it('does not save a stack under a different photo while that photo is loading', () => {
    const edited = { ...INITIAL_ADJUSTMENTS, exposure: 1 };
    openPhoto('/photos/loaded.NEF', INITIAL_ADJUSTMENTS);
    useEditorStore.getState().pushHistory(edited, 'Loaded edit');
    useEditorStore.getState().saveHistory();

    useEditorStore.getState().setEditor({ selectedImage: { path: '/photos/loading.NEF' } as SelectedImage });
    useEditorStore.getState().saveHistory();
    expect(useEditorStore.getState().photoHistories.has('/photos/loading.NEF')).toBe(false);

    openPhoto('/photos/loaded.NEF', edited);
    expect(useEditorStore.getState().historyDetails[1]?.label).toBe('Loaded edit');
  });

  it('keeps virtual copies under separate exact-path keys', () => {
    const originalPath = '/photos/a.NEF';
    const copyPath = '/photos/a.NEF?vc=copy';
    const originalEdit = { ...INITIAL_ADJUSTMENTS, exposure: 1 };
    const copyEdit = { ...INITIAL_ADJUSTMENTS, exposure: -1 };

    openPhoto(originalPath, INITIAL_ADJUSTMENTS);
    useEditorStore.getState().pushHistory(originalEdit, 'Original edit');
    useEditorStore.getState().saveHistory();
    openPhoto(copyPath, INITIAL_ADJUSTMENTS);
    useEditorStore.getState().pushHistory(copyEdit, 'Copy edit');
    useEditorStore.getState().saveHistory();

    openPhoto(originalPath, originalEdit);
    expect(useEditorStore.getState().historyDetails[1]?.label).toBe('Original edit');
    openPhoto(copyPath, copyEdit);
    expect(useEditorStore.getState().historyDetails[1]?.label).toBe('Copy edit');
  });

  it('keeps the 50 most recently used photos and evicts the oldest', () => {
    const edited = (exposure: number) => ({ ...INITIAL_ADJUSTMENTS, exposure });
    for (let index = 0; index < 50; index += 1) {
      openPhoto(`/photos/${index}.NEF`, INITIAL_ADJUSTMENTS);
      useEditorStore.getState().pushHistory(edited(index + 1), `Edit ${index}`);
      useEditorStore.getState().saveHistory();
    }

    openPhoto('/photos/0.NEF', edited(1));
    expect(useEditorStore.getState().history).toHaveLength(2);
    useEditorStore.getState().saveHistory();
    openPhoto('/photos/50.NEF', INITIAL_ADJUSTMENTS);
    useEditorStore.getState().pushHistory(edited(51), 'Edit 50');
    useEditorStore.getState().saveHistory();

    expect(useEditorStore.getState().photoHistories.size).toBe(50);
    expect(useEditorStore.getState().photoHistories.has('/photos/0.NEF')).toBe(true);
    expect(useEditorStore.getState().photoHistories.has('/photos/1.NEF')).toBe(false);
    openPhoto('/photos/1.NEF', edited(2));
    expect(useEditorStore.getState().history).toEqual([edited(2)]);
    expect(useEditorStore.getState().historyIndex).toBe(0);
  });
});
