// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '../test/tauriMock';
import { useLibraryActions } from './useLibraryActions';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { SelectedImage } from '../components/ui/AppProperties';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialLibraryState = useLibraryStore.getState();

afterEach(() => {
  useEditorStore.setState(initialEditorState, true);
  useLibraryStore.setState(initialLibraryState, true);
});

async function mountLibraryActions(handleImageSelect: (path: string) => void) {
  let actions!: ReturnType<typeof useLibraryActions>;
  function Harness() {
    actions = useLibraryActions(handleImageSelect);
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => {
    root.render(createElement(Harness));
  });
  return { actions, unmount: () => act(async () => root.unmount()) };
}

const click = { ctrlKey: false, metaKey: false, shiftKey: false };

function openEditorOn(path: string) {
  useEditorStore.setState({ selectedImage: { path } as SelectedImage });
  useLibraryStore.setState({ multiSelectedPaths: [path], selectionAnchorPath: path });
}

describe('useLibraryActions.handleImageClick with Reference View', () => {
  it('pins a filmstrip click as the reference without changing the active image or selection', async () => {
    openEditorOn('/photos/active.raw');
    useEditorStore.getState().dispatchReferenceView({ type: 'enter' });
    const handleImageSelect = vi.fn();
    const { actions, unmount } = await mountLibraryActions(handleImageSelect);

    actions.handleImageClick('/photos/other.raw?vc=v1', click);

    expect(useEditorStore.getState().referenceView).toEqual({
      isChooserOpen: false,
      mode: 'side-by-side',
      reference: { label: 'other.raw (VC)', path: '/photos/other.raw?vc=v1' },
    });
    expect(handleImageSelect).not.toHaveBeenCalled();
    expect(useEditorStore.getState().selectedImage?.path).toBe('/photos/active.raw');
    expect(useLibraryStore.getState().multiSelectedPaths).toEqual(['/photos/active.raw']);
    expect(invoke).not.toHaveBeenCalled();
    await unmount();
  });

  it('ignores the active image while choosing', async () => {
    openEditorOn('/photos/active.raw');
    useEditorStore.getState().dispatchReferenceView({ type: 'enter' });
    const handleImageSelect = vi.fn();
    const { actions, unmount } = await mountLibraryActions(handleImageSelect);

    actions.handleImageClick('/photos/active.raw', click);

    expect(useEditorStore.getState().referenceView.reference).toBeNull();
    expect(useEditorStore.getState().referenceView.isChooserOpen).toBe(true);
    expect(handleImageSelect).not.toHaveBeenCalled();
    await unmount();
  });

  it('navigates normally once a reference is pinned', async () => {
    openEditorOn('/photos/active.raw');
    useEditorStore
      .getState()
      .dispatchReferenceView({ image: { label: 'ref.raw', path: '/photos/ref.raw' }, type: 'set-reference' });
    const handleImageSelect = vi.fn();
    const { actions, unmount } = await mountLibraryActions(handleImageSelect);

    actions.handleImageClick('/photos/next.raw', click);

    expect(handleImageSelect).toHaveBeenCalledWith('/photos/next.raw');
    expect(useLibraryStore.getState().multiSelectedPaths).toEqual(['/photos/next.raw']);
    expect(useEditorStore.getState().referenceView.reference?.path).toBe('/photos/ref.raw');
    await unmount();
  });
});
