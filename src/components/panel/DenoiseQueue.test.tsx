// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import DenoiseQueue, { DenoiseBadge } from './DenoiseQueue';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useDenoiseQueueStore } from '../../store/useDenoiseQueueStore';
import { computeSortedLibrary } from '../../hooks/useSortedLibrary';
import { RawStatus, type ImageFile } from '../ui/AppProperties';
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'linux' }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
afterEach(() => vi.useRealTimers());
it('keeps badges and filters coherent across same-path backend refresh and then reads external mark changes', async () => {
  vi.useFakeTimers();
  const path = '/photos/a.ARW';
  const photo: ImageFile = {
    path,
    is_raw: true,
    is_edited: false,
    flag: null,
    tags: [],
    exif: null,
    is_virtual_copy: false,
    is_cloud_placeholder: false,
    group_id: null,
    modified: 0,
    rating: 0,
  };
  mocks.invoke.mockResolvedValue({ [path]: { marked: true, denoised: false } });
  useLibraryStore.setState({
    imageList: [photo],
    denoiseStatuses: {},
    imageRatings: {},
    currentFolderPath: '/photos',
    filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All, denoiseStatus: 'marked' },
    searchCriteria: { tags: [], text: '', mode: 'AND' },
    sortCriteria: { key: 'name', order: 'asc' },
  });
  useDenoiseQueueStore.setState({ setupOpen: false, jobId: null, starting: false, summary: null });
  const host = document.createElement('div');
  document.body.append(host);
  const root = createRoot(host);
  const visible = () =>
    computeSortedLibrary(useLibraryStore.getState(), { appSettings: {} }).map((image) => image.path);
  try {
    await act(async () =>
      root.render(
        <>
          <DenoiseQueue />
          <DenoiseBadge path={path} />
        </>,
      ),
    );
    await act(async () => vi.advanceTimersByTimeAsync(100));
    expect(visible()).toEqual([path]);
    await act(async () => useLibraryStore.getState().setLibrary({ imageList: [{ ...photo }] }));
    expect(host.querySelector('[title="denoiseQueue.marked"]')).toBeTruthy();
    expect(visible()).toEqual([path]);
    await act(async () => vi.advanceTimersByTimeAsync(100));
    expect(visible()).toEqual([path]);
    mocks.invoke.mockResolvedValue({ [path]: { marked: false, denoised: false } });
    await act(async () => useLibraryStore.getState().setLibrary({ imageList: [{ ...photo }] }));
    await act(async () => vi.advanceTimersByTimeAsync(100));
    expect(visible()).toEqual([]);
    expect(host.querySelector('[title="denoiseQueue.marked"]')).toBeNull();
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});
