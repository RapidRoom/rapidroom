import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { setDenoiseMark, refreshDenoiseMarks } from './useDenoiseQueueStore';
import { useLibraryStore } from './useLibraryStore';
import { useEditorStore } from './useEditorStore';
import { RawStatus, type ImageFile } from '../components/ui/AppProperties';
import { computeSortedLibrary } from '../hooks/useSortedLibrary';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

const photo = (path: string): ImageFile => ({
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
});

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  useLibraryStore.setState({ multiSelectedPaths: ['/a.ARW', '/b.ARW'], imageList: [photo('/a.ARW'), photo('/b.ARW')] });
  useEditorStore.setState({ selectedImage: null });
  useLibraryStore.setState({ denoiseStatuses: {} });
});

describe('denoise culling marks', () => {
  it('does not let an older folder-status response undo a newer mark refresh', async () => {
    let finishOlder: ((value: Record<string, { marked: boolean; denoised: boolean }>) => void) | undefined;
    vi.mocked(invoke).mockReturnValueOnce(
      new Promise((resolve) => {
        finishOlder = resolve;
      }),
    );
    const older = refreshDenoiseMarks(['/a.ARW']);
    vi.mocked(invoke).mockResolvedValueOnce({ '/a.ARW': { marked: true, denoised: false } });
    await refreshDenoiseMarks(['/a.ARW']);
    finishOlder?.({ '/a.ARW': { marked: false, denoised: false } });
    await older;
    expect(useLibraryStore.getState().denoiseStatuses['/a.ARW'].marked).toBe(true);
  });

  it('serializes rapid repeated shortcut toggles against durable state', async () => {
    let marked = false;
    vi.mocked(invoke).mockImplementation(async (command, payload) => {
      if (command === 'set_denoise_mark') {
        marked = (payload as { marked: boolean }).marked;
        return undefined;
      }
      return Object.fromEntries(['/a.ARW', '/b.ARW'].map((path) => [path, { marked, denoised: false }]));
    });
    await Promise.all([setDenoiseMark(), setDenoiseMark()]);
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === 'set_denoise_mark')
        .map(([, args]) => (args as { marked: boolean }).marked),
    ).toEqual([true, false]);
    expect(useLibraryStore.getState().denoiseStatuses['/a.ARW'].marked).toBe(false);
  });

  it('filters marked photos and denoised photos without changing colour/flag semantics', () => {
    const images = [
      { ...photo('/a.ARW'), denoise_marked: true },
      { ...photo('/b.ARW'), denoised: true },
      photo('/c.ARW'),
    ];
    const filter = (denoiseStatus: 'marked' | 'done') =>
      computeSortedLibrary(
        {
          imageList: images,
          denoiseStatuses: { '/a.ARW': { marked: true, denoised: false }, '/b.ARW': { marked: false, denoised: true } },
          imageRatings: {},
          filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All, denoiseStatus },
          searchCriteria: { tags: [], text: '', mode: 'AND' },
          sortCriteria: { key: 'name', order: 'asc' },
        },
        { appSettings: {} },
      ).map((image) => image.path);
    expect(filter('marked')).toEqual(['/a.ARW']);
    expect(filter('done')).toEqual(['/b.ARW']);
    expect(images.every((image) => image.flag === null && image.tags?.length === 0)).toBe(true);
  });
});
