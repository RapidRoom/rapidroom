// @vitest-environment jsdom
import { act, createElement, type ReactElement } from 'react';
import { toast } from 'react-toastify';
vi.mock('react-toastify', () => ({
  toast: Object.assign(vi.fn(), { isActive: vi.fn(() => false), update: vi.fn(), dismiss: vi.fn(), error: vi.fn() }),
}));
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
      wipePosition: 50,
      wipeSwapped: false,
      wipeReference: false,
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
  it('offers Undo only after a saved filter exit and restores through the queue', async () => {
    filteredSelection();
    vi.mocked(toast).mockClear();
    let finish!: () => void;
    let call = 0;
    mockCommand(Invokes.SetFlagForPaths, () =>
      ++call === 1
        ? new Promise<void>((resolve) => {
            finish = resolve;
          })
        : undefined,
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetFlag(ImageFlag.Reject);
    expect(toast).not.toHaveBeenCalled();
    await act(async () => finish());
    const notice = vi.mocked(toast).mock.calls.at(-1)![0] as ReactElement<{ onUndo(): void }>;
    expect(notice).toBeTruthy();
    await act(async () => notice.props.onUndo());
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SetFlagForPaths, { paths: ['/a.raw'], flag: null });
    expect(useLibraryStore.getState().libraryActivePath).toBe('/a.raw');
    expect(useLibraryStore.getState().imageList[0].flag).toBeNull();
    await unmount();
  });

  it('waits for a genuinely pending write before applying Undo', async () => {
    filteredSelection();
    vi.mocked(toast).mockClear();
    let finishPick!: () => void;
    mockCommand(Invokes.SetFlagForPaths, (args) =>
      args?.flag === ImageFlag.Pick
        ? new Promise<void>((resolve) => {
            finishPick = resolve;
          })
        : undefined,
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    await act(async () => actions.handleSetFlag(ImageFlag.Reject));
    const notice = vi.mocked(toast).mock.calls.at(-1)![0] as ReactElement<{ onUndo(): void }>;
    actions.handleSetFlag(ImageFlag.Pick, ['/b.raw']);
    const writes = invoke.mock.calls.length;
    await act(async () => notice.props.onUndo());
    expect(invoke.mock.calls).toHaveLength(writes);
    expect(useLibraryStore.getState().imageList[0].flag).toBe(ImageFlag.Reject);
    await act(async () => finishPick());
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SetFlagForPaths, { paths: ['/a.raw'], flag: null });
    expect(useLibraryStore.getState().imageList[1].flag).toBe(ImageFlag.Pick);
    expect(useLibraryStore.getState().libraryActivePath).toBe('/a.raw');
    await unmount();
  });

  it('undoes a colour label without removing user tags', async () => {
    filteredSelection();
    vi.mocked(toast).mockClear();
    useLibraryStore.setState((state) => ({
      imageList: state.imageList.map((image) =>
        image.path === '/a.raw' ? { ...image, tags: ['user:x', 'color:blue'] } : image,
      ),
      filterCriteria: { colors: ['blue'], rating: 0, rawStatus: RawStatus.All },
    }));
    mockCommand(Invokes.SetColorLabelForPaths, () => undefined);
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    await act(async () => actions.handleSetColorLabel('red'));
    const notice = vi.mocked(toast).mock.calls.at(-1)![0] as ReactElement<{ onUndo(): void }>;
    await act(async () => notice.props.onUndo());
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SetColorLabelForPaths, { paths: ['/a.raw'], color: 'blue' });
    expect(useLibraryStore.getState().imageList[0].tags).toEqual(['user:x', 'color:blue']);
    expect(useLibraryStore.getState().libraryActivePath).toBe('/a.raw');
    await unmount();
  });

  it.each(['folder', 'view'] as const)('dismisses and invalidates Undo after a %s switch', async (navigation) => {
    filteredSelection();
    vi.mocked(toast).mockClear();
    vi.mocked(toast.dismiss).mockClear();
    mockCommand(Invokes.SetFlagForPaths, () => undefined);
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    await act(async () => actions.handleSetFlag(ImageFlag.Reject));
    const notice = vi.mocked(toast).mock.calls.at(-1)![0] as ReactElement<{ onUndo(): void }>;
    const writes = invoke.mock.calls.length;
    await act(async () => {
      if (navigation === 'view') useUIStore.setState({ activeView: 'editor' });
      else
        useLibraryStore.setState((state) => ({
          currentFolderPath: '/other',
          imageList: [{ ...state.imageList[0], path: '/other/photo.raw', flag: null }],
          imageRatings: { '/other/photo.raw': 2 },
          libraryActivePath: '/other/photo.raw',
        }));
    });
    expect(toast.dismiss).toHaveBeenCalledWith('filter-exit');
    const state = useLibraryStore.getState();
    await act(async () => notice.props.onUndo());
    expect(invoke.mock.calls).toHaveLength(writes);
    expect(useLibraryStore.getState()).toBe(state);
    await unmount();
  });

  it('attributes a filter-exit toast to the hiding write, preserving an earlier colour change', async () => {
    filteredSelection();
    vi.mocked(toast).mockClear();
    useLibraryStore.setState({
      imageRatings: { '/a.raw': 3, '/b.raw': 3 },
      filterCriteria: { colors: [], rating: 3, ratingOperator: 'eq', rawStatus: RawStatus.All },
    });
    let finishColour!: () => void;
    let colours = 0;
    mockCommand(Invokes.SetColorLabelForPaths, () =>
      ++colours === 1
        ? new Promise<void>((resolve) => {
            finishColour = resolve;
          })
        : undefined,
    );
    mockCommand(Invokes.SetRatingForPaths, () => undefined);
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    actions.handleSetColorLabel('red');
    actions.handleRate(4, ['/a.raw']);
    await act(async () => finishColour());
    expect(toast).toHaveBeenCalledTimes(1);
    const notice = vi.mocked(toast).mock.calls[0][0] as ReactElement<{ onUndo(): void }>;
    await act(async () => notice.props.onUndo());
    expect(colours).toBe(1);
    expect(useLibraryStore.getState().imageList[0].tags).toEqual(['color:red']);
    expect(useLibraryStore.getState().imageRatings['/a.raw']).toBe(3);
    await unmount();
  });

  it('restores a rated reject on disk after undoing a rating filter exit', async () => {
    filteredSelection();
    vi.mocked(toast).mockClear();
    useLibraryStore.setState((state) => ({
      imageRatings: { '/a.raw': 3 },
      imageList: state.imageList.map((image) =>
        image.path === '/a.raw' ? { ...image, flag: ImageFlag.Reject } : image,
      ),
      filterCriteria: { colors: [], rating: 3, ratingOperator: 'eq', rawStatus: RawStatus.All },
    }));
    let rating = 3;
    let flag: ImageFlag | null = ImageFlag.Reject;
    mockCommand(Invokes.SetRatingForPaths, (args) => {
      rating = args!.rating as number;
      if (rating > 0) flag = null;
    });
    mockCommand(Invokes.SetFlagForPaths, (args) => {
      flag = args!.flag as ImageFlag | null;
    });
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    await act(async () => actions.handleRate(4));
    const notice = vi.mocked(toast).mock.calls.at(-1)![0] as ReactElement<{ onUndo(): void }>;
    await act(async () => notice.props.onUndo());
    expect(rating).toBe(3);
    expect(flag).toBe(ImageFlag.Reject);
    expect(useLibraryStore.getState().imageList[0].flag).toBe(ImageFlag.Reject);
    expect(invoke.mock.calls.slice(-2)).toEqual([
      [Invokes.SetRatingForPaths, { paths: ['/a.raw'], rating: 3 }],
      [Invokes.SetFlagForPaths, { paths: ['/a.raw'], flag: ImageFlag.Reject }],
    ]);
    await unmount();
  });

  it('restores a refused flag without advancing away from the photo', async () => {
    filteredSelection();
    vi.mocked(toast).mockClear();
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
    expect(toast).not.toHaveBeenCalled();
    expect(useUIStore.getState().imageSelectHandler).not.toHaveBeenCalled();
    await unmount();
  });
  it('rolls back a refused Undo without selecting a still-hidden photo', async () => {
    filteredSelection();
    vi.mocked(toast).mockClear();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    mockCommand(Invokes.SetFlagForPaths, (args) =>
      args?.flag === ImageFlag.Reject ? undefined : Promise.reject(new Error('undo refused')),
    );
    const { actions, unmount } = await mountLibraryActions(vi.fn());
    await act(async () => actions.handleSetFlag(ImageFlag.Reject));
    const notice = vi.mocked(toast).mock.calls.at(-1)![0] as ReactElement<{ onUndo(): void }>;
    await act(async () => notice.props.onUndo());
    expect(useLibraryStore.getState().imageList[0].flag).toBe(ImageFlag.Reject);
    expect(useLibraryStore.getState().libraryActivePath).toBe('/b.raw');
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
