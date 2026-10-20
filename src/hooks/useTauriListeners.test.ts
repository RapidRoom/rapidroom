// @vitest-environment jsdom
import { createElement } from 'react';
import { act } from 'react';
import { showInstagramExportComplete } from '../utils/instagramExportToast';
vi.mock('../utils/instagramExportToast', () => ({ showInstagramExportComplete: vi.fn() }));
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { emit, listen, listenerCount } from '../test/tauriMock';
import { useTauriListeners } from './useTauriListeners';
import { useProcessStore } from '../store/useProcessStore';
import { useEditorStore } from '../store/useEditorStore';
import { Status } from '../components/ui/ExportImportProperties';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const initialProcessState = useProcessStore.getState();

afterEach(() => {
  useProcessStore.setState(initialProcessState, true);
  useEditorStore.setState(initialEditorState, true);
});

async function mountListeners() {
  const props = {
    refreshAllFolderTrees: vi.fn(),
    handleSelectSubfolder: vi.fn(),
    refreshImageList: vi.fn(),
    markGenerated: vi.fn(),
  };
  function Harness() {
    useTauriListeners(props);
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => {
    root.render(createElement(Harness));
  });
  return { props, unmount: () => act(async () => root.unmount()) };
}

describe('useTauriListeners', () => {
  it('handles backend events while mounted', async () => {
    const { props, unmount } = await mountListeners();

    await act(() => emit('export-complete'));

    expect(useProcessStore.getState().exportState.status).toBe(Status.Success);
    expect(props.refreshAllFolderTrees).toHaveBeenCalledTimes(1);
    await unmount();
  });

  it('removes every listener on unmount and ignores later events', async () => {
    const { props, unmount } = await mountListeners();
    expect(listen).toHaveBeenCalled();
    expect(listenerCount()).toBe(listen.mock.calls.length);

    await unmount();

    expect(listenerCount()).toBe(0);
    await act(() => emit('export-complete'));
    expect(useProcessStore.getState().exportState.status).toBe(initialProcessState.exportState.status);
    expect(props.refreshAllFolderTrees).not.toHaveBeenCalled();
  });
});

it('offers the actual Instagram folder only on successful backend completion', async () => {
  vi.mocked(showInstagramExportComplete).mockClear();
  const { unmount } = await mountListeners();
  await act(() => emit('batch-export-progress', { current: 1, total: 1 }));
  await act(() => emit('export-error', 'write failed'));
  await act(() => emit('export-cancelled'));
  await act(() => emit('export-complete'));
  expect(showInstagramExportComplete).not.toHaveBeenCalled();
  await act(() => emit('export-complete', { instagramOutputFolder: '/Pictures/Instagram/2026-10-16' }));
  expect(showInstagramExportComplete).toHaveBeenCalledExactlyOnceWith('/Pictures/Instagram/2026-10-16');
  await unmount();
});

it('ignores queued analytics and frame events from a previous visit to the same photo', async () => {
  const { unmount } = await mountListeners();
  const selected = (path: string, loadGeneration: number) => ({
    path,
    loadGeneration,
    isReady: true,
    isRaw: true,
    exif: {},
    width: 12,
    height: 8,
    thumbnailUrl: '',
  });
  try {
    await act(async () => useEditorStore.setState({ selectedImage: selected('/photos/A.ARW', 1) }));
    await act(async () => useEditorStore.setState({ selectedImage: selected('/photos/B.ARW', 2) }));
    await act(async () =>
      useEditorStore.setState({
        selectedImage: selected('/photos/A.ARW', 3),
        histogram: null,
        hasRenderedFirstFrame: false,
      }),
    );
    const histogram = { red: [1], green: [2], blue: [3], luma: [4] };
    await act(() => emit('analytics-update', { path: '/photos/A.ARW', loadGeneration: 1, histogram }));
    await act(() => emit('wgpu-frame-ready', { path: '/photos/A.ARW', loadGeneration: 1 }));
    expect(useEditorStore.getState().histogram).toBeNull();
    expect(useEditorStore.getState().hasRenderedFirstFrame).toBe(false);
    await act(() => emit('analytics-update', { path: '/photos/A.ARW', loadGeneration: 3, histogram }));
    await act(() => emit('wgpu-frame-ready', { path: '/photos/A.ARW', loadGeneration: 3 }));
    expect(useEditorStore.getState().histogram).toEqual(histogram);
    expect(useEditorStore.getState().hasRenderedFirstFrame).toBe(true);
  } finally {
    await unmount();
  }
});
