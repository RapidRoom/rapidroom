// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '../test/tauriMock';
import { useLibraryActions } from './useLibraryActions';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { ImageFile, LibraryViewMode, RawStatus, SelectedImage, Theme } from '../components/ui/AppProperties';
import { useSettingsStore } from '../store/useSettingsStore';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialLibraryState = useLibraryStore.getState();
const initialSettingsState = useSettingsStore.getState();

afterEach(() => {
  useEditorStore.setState(initialEditorState, true);
  useLibraryStore.setState(initialLibraryState, true);
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

describe('useLibraryActions.handleLibraryImageSingleClick recursive range selection', () => {
  it('selects only the contiguous current-folder range in Recursive view', async () => {
    const imageList = ['/photos/current/MG_8355.jpg', '/photos/current/IMG_1821.jpg', '/photos/child/IMG_5000.jpg'].map(
      (path) => ({ path }) as ImageFile,
    );
    const anchor = '/photos/current/MG_8355.jpg';
    useLibraryStore.setState({
      ...initialLibraryState,
      currentFolderPath: '/photos/current',
      imageList,
      multiSelectedPaths: [anchor],
      selectionAnchorPath: anchor,
      sortCriteria: { key: 'name', order: 'asc' },
      filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All },
      searchCriteria: { tags: [], text: '', mode: 'OR' },
    });
    useSettingsStore.setState({
      ...initialSettingsState,
      appSettings: {
        ...(initialSettingsState.appSettings ?? { lastRootPath: null, theme: Theme.Dark }),
        libraryViewMode: LibraryViewMode.Recursive,
      },
    });
    const { actions, unmount } = await mountLibraryActions(vi.fn());

    actions.handleLibraryImageSingleClick('/photos/current/IMG_1821.jpg', { ...click, shiftKey: true }, [
      '/photos/current/IMG_1821.jpg',
      anchor,
      '/photos/child/IMG_5000.jpg',
    ]);

    expect(useLibraryStore.getState().multiSelectedPaths).toEqual([
      '/photos/current/IMG_1821.jpg',
      '/photos/current/MG_8355.jpg',
    ]);
    await unmount();
  });

  it('keeps the globally sorted range in Flat view', async () => {
    const imageList = ['/photos/current/MG_8355.jpg', '/photos/current/IMG_1821.jpg', '/photos/child/IMG_5000.jpg'].map(
      (path) => ({ path }) as ImageFile,
    );
    const anchor = '/photos/current/MG_8355.jpg';
    useLibraryStore.setState({
      ...initialLibraryState,
      currentFolderPath: '/photos/current',
      imageList,
      multiSelectedPaths: [anchor],
      selectionAnchorPath: anchor,
      sortCriteria: { key: 'name', order: 'asc' },
      filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All },
      searchCriteria: { tags: [], text: '', mode: 'OR' },
    });
    useSettingsStore.setState({
      ...initialSettingsState,
      appSettings: {
        ...(initialSettingsState.appSettings ?? { lastRootPath: null, theme: Theme.Dark }),
        libraryViewMode: LibraryViewMode.Flat,
      },
    });
    const { actions, unmount } = await mountLibraryActions(vi.fn());

    actions.handleLibraryImageSingleClick('/photos/current/IMG_1821.jpg', { ...click, shiftKey: true }, [
      '/photos/current/IMG_1821.jpg',
      anchor,
      '/photos/child/IMG_5000.jpg',
    ]);

    expect(useLibraryStore.getState().multiSelectedPaths).toContain('/photos/child/IMG_5000.jpg');
    await unmount();
  });

  it('selects the clicked visible photo and resets a collapsed-folder anchor', async () => {
    const hiddenAnchor = '/photos/closed/b.jpg';
    const clickedPath = '/photos/current/a.jpg';
    useLibraryStore.setState({
      ...initialLibraryState,
      currentFolderPath: '/photos/current',
      imageList: [clickedPath, hiddenAnchor, '/photos/open/c.jpg'].map((path) => ({ path }) as ImageFile),
      multiSelectedPaths: [hiddenAnchor],
      libraryActivePath: hiddenAnchor,
      selectionAnchorPath: hiddenAnchor,
      sortCriteria: { key: 'name', order: 'asc' },
      filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All },
      searchCriteria: { tags: [], text: '', mode: 'OR' },
    });
    useSettingsStore.setState({
      ...initialSettingsState,
      appSettings: {
        ...(initialSettingsState.appSettings ?? { lastRootPath: null, theme: Theme.Dark }),
        libraryViewMode: LibraryViewMode.Recursive,
      },
    });
    const { actions, unmount } = await mountLibraryActions(vi.fn());

    actions.handleLibraryImageSingleClick(clickedPath, { ...click, shiftKey: true }, [
      clickedPath,
      '/photos/open/c.jpg',
    ]);

    expect(useLibraryStore.getState().multiSelectedPaths).toEqual([clickedPath]);
    expect(useLibraryStore.getState().libraryActivePath).toBe(clickedPath);
    expect(useLibraryStore.getState().selectionAnchorPath).toBe(clickedPath);
    await unmount();
  });

  it('keeps existing selections when Ctrl+Shift falls back from a collapsed-folder anchor', async () => {
    const hiddenAnchor = '/photos/closed/b.jpg';
    const alreadySelected = '/photos/open/c.jpg';
    const clickedPath = '/photos/current/a.jpg';
    useLibraryStore.setState({
      ...initialLibraryState,
      currentFolderPath: '/photos/current',
      imageList: [clickedPath, hiddenAnchor, alreadySelected].map((path) => ({ path }) as ImageFile),
      multiSelectedPaths: [hiddenAnchor, alreadySelected],
      libraryActivePath: hiddenAnchor,
      selectionAnchorPath: hiddenAnchor,
      sortCriteria: { key: 'name', order: 'asc' },
      filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All },
      searchCriteria: { tags: [], text: '', mode: 'OR' },
    });
    useSettingsStore.setState({
      ...initialSettingsState,
      appSettings: {
        ...(initialSettingsState.appSettings ?? { lastRootPath: null, theme: Theme.Dark }),
        libraryViewMode: LibraryViewMode.Recursive,
      },
    });
    const { actions, unmount } = await mountLibraryActions(vi.fn());

    actions.handleLibraryImageSingleClick(clickedPath, { ctrlKey: true, metaKey: false, shiftKey: true }, [
      clickedPath,
      alreadySelected,
    ]);

    expect(useLibraryStore.getState().multiSelectedPaths).toEqual([hiddenAnchor, alreadySelected, clickedPath]);
    expect(useLibraryStore.getState().libraryActivePath).toBe(clickedPath);
    expect(useLibraryStore.getState().selectionAnchorPath).toBe(clickedPath);
    await unmount();
  });
});
