// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../test/tauriMock';
import { useUIStore } from '../store/useUIStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useLibraryActions } from './useLibraryActions';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { SelectedImage, ImageFlag, FlagStatus, RawStatus, Invokes } from '../components/ui/AppProperties';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialLibraryState = useLibraryStore.getState();
const initialUIState = useUIStore.getState();
const initialSettingsState = useSettingsStore.getState();

afterEach(() => {
  useEditorStore.setState(initialEditorState, true);
  useLibraryStore.setState(initialLibraryState, true);
  useUIStore.setState(initialUIState, true);
  useSettingsStore.setState(initialSettingsState, true);
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

describe('filtered selection after atomic metadata writes', () => {
  function filteredSelection() {
    useUIStore.setState({ activeView: 'library', imageSelectHandler: vi.fn() });
    useLibraryStore.setState({
      imageList: ['/a.raw', '/b.raw'].map((path, modified) => ({
        path,
        modified,
        is_raw: true,
        is_edited: false,
        is_virtual_copy: false,
        is_cloud_placeholder: false,
        rating: 0,
        exif: null,
        group_id: null,
        tags: [],
        flag: null,
      })) as ReturnType<typeof useLibraryStore.getState>['imageList'],
      imageRatings: {},
      libraryActivePath: '/a.raw',
      multiSelectedPaths: ['/a.raw'],
      selectionAnchorPath: '/a.raw',
      filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All, flagStatus: FlagStatus.ExcludeRejected },
      searchCriteria: { tags: [], text: '', mode: 'AND' },
      sortCriteria: { key: 'name', order: 'asc' },
    });
  }
  it('advances to the next visible image only after the backend saves the reject', async () => {
    filteredSelection();
    let finish!: () => void;
    mockCommand(
      Invokes.SetFlagForPaths,
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetFlag(ImageFlag.Reject);
    expect(useLibraryStore.getState().imageList[0].flag).toBe(ImageFlag.Reject);
    expect(useLibraryStore.getState().libraryActivePath).toBe('/a.raw');
    await act(async () => finish());
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    expect(useUIStore.getState().imageSelectHandler).toHaveBeenCalledWith('/b.raw', false);
    await unmount();
  });
  it('restores a refused flag without advancing away from the photo', async () => {
    filteredSelection();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    let fail!: (error: Error) => void;
    mockCommand(
      Invokes.SetFlagForPaths,
      () =>
        new Promise<void>((_, reject) => {
          fail = reject;
        }),
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetFlag(ImageFlag.Reject);
    await act(async () => fail(new Error('write refused')));
    expect(useLibraryStore.getState().imageList[0].flag).toBeNull();
    expect(useLibraryStore.getState().libraryActivePath).toBe('/a.raw');
    expect(useUIStore.getState().imageSelectHandler).not.toHaveBeenCalled();
    await unmount();
  });
  it('does not override navigation made while a write is pending', async () => {
    filteredSelection();
    let finish!: () => void;
    mockCommand(
      Invokes.SetFlagForPaths,
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetFlag(ImageFlag.Reject);
    useLibraryStore.setState({ libraryActivePath: '/b.raw', multiSelectedPaths: ['/b.raw'] });
    await act(async () => finish());
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    expect(useUIStore.getState().imageSelectHandler).not.toHaveBeenCalled();
    await unmount();
  });
  it('advances after a saved color label excludes the active photo, but keeps a refused edit in place', async () => {
    filteredSelection();
    useLibraryStore.setState({
      filterCriteria: { colors: ['none'], rating: 0, rawStatus: RawStatus.All, flagStatus: FlagStatus.ExcludeRejected },
    });
    let finish!: () => void;
    mockCommand(
      Invokes.SetColorLabelForPaths,
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    const saved = actions.handleSetColorLabel('red');
    expect(useLibraryStore.getState().libraryActivePath).toBe('/a.raw');
    await act(async () => {
      finish();
      await saved;
    });
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    mockCommand(Invokes.SetColorLabelForPaths, () => Promise.reject(new Error('write refused')));
    await act(async () => {
      await actions.handleSetColorLabel('red');
    });
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    expect(useLibraryStore.getState().imageList[1].tags).toEqual([]);
    await unmount();
  });
  it('advances when a saved tag change removes the active photo from the search', async () => {
    filteredSelection();
    useLibraryStore.setState((state) => ({
      imageList: state.imageList.map((image) => ({ ...image, tags: ['user:keep'] })),
      searchCriteria: { tags: ['keep'], text: '', mode: 'AND' },
    }));
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleTagsChanged(['/a.raw'], []);
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    await unmount();
  });
});
