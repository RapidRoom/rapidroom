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
import {
  ImageFile,
  LibraryViewMode,
  SelectedImage,
  Theme,
  ImageFlag,
  FlagStatus,
  RawStatus,
  Invokes,
} from '../components/ui/AppProperties';

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
  useLibraryStore.setState({
    multiSelectedPaths: [path],
    selectionAnchorPath: path,
  });
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
    useEditorStore.getState().dispatchReferenceView({
      image: { label: 'ref.raw', path: '/photos/ref.raw' },
      type: 'set-reference',
    });
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
      filterCriteria: {
        colors: [],
        rating: 0,
        rawStatus: RawStatus.All,
        flagStatus: FlagStatus.ExcludeRejected,
      },
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
    useLibraryStore.setState({
      libraryActivePath: '/b.raw',
      multiSelectedPaths: ['/b.raw'],
    });
    await act(async () => finish());
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    expect(useUIStore.getState().imageSelectHandler).not.toHaveBeenCalled();
    await unmount();
  });
  it('advances after a saved color label excludes the active photo, but keeps a refused edit in place', async () => {
    filteredSelection();
    useLibraryStore.setState({
      filterCriteria: {
        colors: ['none'],
        rating: 0,
        rawStatus: RawStatus.All,
        flagStatus: FlagStatus.ExcludeRejected,
      },
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
      imageList: state.imageList.map((image) => ({
        ...image,
        tags: ['user:keep'],
      })),
      searchCriteria: { tags: ['keep'], text: '', mode: 'AND' },
    }));
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleTagsChanged(['/a.raw'], []);
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    await unmount();
  });
  it('does not navigate because of a later unsaved reject when an earlier pick saves', async () => {
    filteredSelection();
    let finishPick!: () => void;
    let failReject!: (error: Error) => void;
    mockCommand(Invokes.SetFlagForPaths, (args) =>
      args?.flag === ImageFlag.Pick
        ? new Promise<void>((resolve) => {
            finishPick = resolve;
          })
        : new Promise<void>((_, reject) => {
            failReject = reject;
          }),
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetFlag(ImageFlag.Pick);
    actions.handleSetFlag(ImageFlag.Reject);
    await act(async () => finishPick());
    const whileRejectIsPending = useLibraryStore.getState().libraryActivePath;
    await act(async () => failReject(new Error('write refused')));
    expect(whileRejectIsPending).toBe('/a.raw');
    expect(useLibraryStore.getState().libraryActivePath).toBe('/a.raw');
    expect(useLibraryStore.getState().imageList[0].flag).toBe(ImageFlag.Pick);
    await unmount();
  });
  it('uses the pre-write order when a failed reject is followed by a saved reject', async () => {
    filteredSelection();
    let failFirst!: (error: Error) => void;
    let finishSecond!: () => void;
    let count = 0;
    mockCommand(Invokes.SetFlagForPaths, () =>
      ++count === 1
        ? new Promise<void>((_, reject) => {
            failFirst = reject;
          })
        : new Promise<void>((resolve) => {
            finishSecond = resolve;
          }),
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetFlag(ImageFlag.Reject);
    actions.handleSetFlag(ImageFlag.Reject);
    await act(async () => failFirst(new Error('write refused')));
    await act(async () => finishSecond());
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
    await unmount();
  });
  it('waits for pending flag rollback before advancing after a saved color label', async () => {
    filteredSelection();
    useLibraryStore.setState({
      filterCriteria: {
        colors: ['none'],
        rating: 0,
        rawStatus: RawStatus.All,
        flagStatus: FlagStatus.ExcludeRejected,
      },
    });
    let fail!: (error: Error) => void;
    mockCommand(
      Invokes.SetFlagForPaths,
      () =>
        new Promise<void>((_, reject) => {
          fail = reject;
        }),
    );
    mockCommand(Invokes.SetColorLabelForPaths, () => undefined);
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetFlag(ImageFlag.Reject, ['/b.raw']);
    await act(async () => actions.handleSetColorLabel('red', ['/a.raw']));
    const whileFlagIsPending = useLibraryStore.getState().libraryActivePath;
    await act(async () => fail(new Error('write refused')));
    expect(whileFlagIsPending).toBe('/a.raw');
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
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
