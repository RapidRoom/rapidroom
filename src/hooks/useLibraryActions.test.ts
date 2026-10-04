// @vitest-environment jsdom
import { createElement } from 'react';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it } from 'vitest';
import { invoke, mockCommand } from '../test/tauriMock';
import { useLibraryActions } from './useLibraryActions';
import { useLibraryStore } from '../store/useLibraryStore';
import { useEditorStore } from '../store/useEditorStore';
import { ImageFile, ImageFlag, Invokes } from '../components/ui/AppProperties';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialLibraryState = useLibraryStore.getState();
const initialEditorState = useEditorStore.getState();

afterEach(() => {
  useLibraryStore.setState(initialLibraryState, true);
  useEditorStore.setState(initialEditorState, true);
});

async function mountLibraryActions() {
  let actions!: ReturnType<typeof useLibraryActions>;
  function Harness() {
    actions = useLibraryActions();
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => {
    root.render(createElement(Harness));
  });
  return actions;
}

function library(entries: Record<string, ImageFlag | null>, ratings: Record<string, number> = {}) {
  useLibraryStore.setState({
    imageList: Object.entries(entries).map(([path, flag]) => ({ path, flag, tags: [] }) as unknown as ImageFile),
    imageRatings: ratings,
  });
}

const storedFlags = () => Object.fromEntries(useLibraryStore.getState().imageList.map((i) => [i.path, i.flag]));
const settle = () => act(async () => {});

describe('flag actions in the library store', () => {
  it('sets a flag on the selection right away and saves it', async () => {
    const saved: unknown[] = [];
    mockCommand(Invokes.SetFlagForPaths, (args) => saved.push(args));
    library({ '/a.raw': null, '/b.raw': ImageFlag.Reject, '/c.raw': null });
    useLibraryStore.setState({ multiSelectedPaths: ['/a.raw', '/b.raw'] });
    const actions = await mountLibraryActions();

    actions.handleSetFlag(ImageFlag.Pick);
    expect(storedFlags()).toEqual({ '/a.raw': ImageFlag.Pick, '/b.raw': ImageFlag.Pick, '/c.raw': null });
    await settle();
    expect(saved).toEqual([{ paths: ['/a.raw', '/b.raw'], flag: ImageFlag.Pick }]);

    actions.handleSetFlag(null, ['/b.raw']);
    expect(storedFlags()['/b.raw']).toBeNull();
    await settle();
    expect(saved[1]).toEqual({ paths: ['/b.raw'], flag: null });
  });

  it('puts the old flags back when saving is refused (Card mode)', async () => {
    mockCommand(Invokes.SetFlagForPaths, () => {
      throw new Error('This card is open read-only.');
    });
    library({ '/a.raw': ImageFlag.Pick, '/b.raw': null, '/c.raw': ImageFlag.Reject });
    const actions = await mountLibraryActions();

    actions.handleSetFlag(ImageFlag.Reject, ['/a.raw', '/b.raw']);
    expect(storedFlags()).toEqual({
      '/a.raw': ImageFlag.Reject,
      '/b.raw': ImageFlag.Reject,
      '/c.raw': ImageFlag.Reject,
    });
    await settle();
    expect(storedFlags()).toEqual({ '/a.raw': ImageFlag.Pick, '/b.raw': null, '/c.raw': ImageFlag.Reject });
  });

  it('toggles a flag off when the first selected image already has it', async () => {
    mockCommand(Invokes.SetFlagForPaths, () => undefined);
    library({ '/a.raw': ImageFlag.Pick, '/b.raw': null });
    const actions = await mountLibraryActions();

    actions.handleToggleFlag(ImageFlag.Pick, ['/a.raw', '/b.raw']);
    expect(storedFlags()).toEqual({ '/a.raw': null, '/b.raw': null });
    actions.handleToggleFlag(ImageFlag.Pick, ['/b.raw', '/a.raw']);
    expect(storedFlags()).toEqual({ '/a.raw': ImageFlag.Pick, '/b.raw': ImageFlag.Pick });
    await settle();
  });

  it('giving stars clears a reject but keeps a pick', async () => {
    mockCommand(Invokes.SetRatingForPaths, () => undefined);
    library({ '/r.raw': ImageFlag.Reject, '/p.raw': ImageFlag.Pick });
    const actions = await mountLibraryActions();

    actions.handleRate(3, ['/r.raw', '/p.raw']);
    expect(storedFlags()).toEqual({ '/r.raw': null, '/p.raw': ImageFlag.Pick });
    expect(useLibraryStore.getState().imageRatings).toEqual({ '/r.raw': 3, '/p.raw': 3 });
    await settle();
    expect(invoke).toHaveBeenCalledWith(Invokes.SetRatingForPaths, { paths: ['/r.raw', '/p.raw'], rating: 3 });
  });

  it('clearing stars keeps a reject', async () => {
    mockCommand(Invokes.SetRatingForPaths, () => undefined);
    library({ '/r.raw': ImageFlag.Reject }, { '/r.raw': 2 });
    const actions = await mountLibraryActions();

    actions.handleRate(0, ['/r.raw']);
    expect(storedFlags()).toEqual({ '/r.raw': ImageFlag.Reject });
    await settle();
  });
});
