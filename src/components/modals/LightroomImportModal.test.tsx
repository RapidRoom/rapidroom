// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../../test/tauriMock';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { AppSettings, Invokes } from '../ui/AppProperties';
import LightroomImportModal, { LightroomImportPreview } from './LightroomImportModal';

const pickFolder = vi.hoisted(() => vi.fn());

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: pickFolder }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const CATALOG = '/Catalogs/Lightroom Catalog.lrcat';
const initialLibrary = useLibraryStore.getState();
const initialSettings = useSettingsStore.getState();
let root: Root | undefined;
let container: HTMLDivElement;

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  container?.remove();
  document.body.innerHTML = '';
  useLibraryStore.setState(initialLibrary, true);
  useSettingsStore.setState(initialSettings, true);
  pickFolder.mockReset();
});

function previewFor(mappings: Record<string, string>): LightroomImportPreview {
  const relinked = mappings['C:/Users/Benny/Pictures/'];
  return {
    catalogName: 'Lightroom Catalog',
    collectionCount: 2,
    groupCount: 1,
    matchedImageCount: relinked ? 3 : 0,
    missingImageCount: relinked ? 0 : 3,
    missingImages: relinked ? [] : ['C:/Users/Benny/Pictures/a.CR2'],
    smartCollections: ['Set / Five stars'],
    skippedOtherCount: 0,
    rootFolders: [
      {
        catalogPath: 'C:/Users/Benny/Pictures/',
        localPath: relinked ?? 'C:/Users/Benny/Pictures',
        resolution: relinked ? 'mapped' : 'original',
        found: !!relinked,
        imageCount: 3,
        missingImageCount: relinked ? 0 : 3,
      },
    ],
    replacesPreviousImport: false,
  };
}

async function mount() {
  const handleSettingsChange = vi.fn(async (settings: AppSettings) => {
    useSettingsStore.setState({ appSettings: settings });
  });
  useSettingsStore.setState({
    appSettings: {
      lightroomPathMappings: { '/Volumes/Old/': '/mnt/old' },
      rootFolders: ['/photos'],
      lastRootPath: null,
      theme: 'dark',
    } as AppSettings,
    handleSettingsChange,
  });
  useLibraryStore.setState({ rootPaths: ['/photos'], expandedFolders: new Set(['/photos']) });
  mockCommand(Invokes.PreviewLightroomCollections, (args) => previewFor(args?.mappings as Record<string, string>));
  mockCommand(Invokes.ImportLightroomCollections, (args) => previewFor(args?.mappings as Record<string, string>));
  const importedTree = [{ type: 'group', id: 'lightroom-import:abc', name: 'Lightroom Catalog', children: [] }];
  mockCommand(Invokes.GetAlbums, () => importedTree);

  const onClose = vi.fn();
  const refreshAllFolderTrees = vi.fn(async () => {});
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () =>
    root?.render(
      <LightroomImportModal catalogPath={CATALOG} onClose={onClose} refreshAllFolderTrees={refreshAllFolderTrees} />,
    ),
  );
  return { handleSettingsChange, onClose, refreshAllFolderTrees, importedTree };
}

function button(key: string) {
  const found = Array.from(document.body.querySelectorAll('button')).find((item) =>
    item.textContent?.includes(`modals.lightroomImport.${key}`),
  );
  if (!found) throw new Error(`Button ${key} not found`);
  return found;
}

function calls(command: string) {
  return invoke.mock.calls.filter(([cmd]) => cmd === command).map(([, args]) => args);
}

describe('LightroomImportModal', () => {
  it('previews with saved mappings, relinks without saving, then imports and saves', async () => {
    const { handleSettingsChange, onClose, refreshAllFolderTrees, importedTree } = await mount();

    expect(calls(Invokes.PreviewLightroomCollections)).toEqual([
      { path: CATALOG, mappings: { '/Volumes/Old/': '/mnt/old' } },
    ]);
    expect(document.body.textContent).toContain('modals.lightroomImport.notFound');
    expect(document.body.textContent).toContain('C:/Users/Benny/Pictures/a.CR2');

    pickFolder.mockResolvedValue('/mnt/pictures');
    await act(async () => button('relink').click());

    expect(calls(Invokes.PreviewLightroomCollections)).toHaveLength(2);
    expect(calls(Invokes.PreviewLightroomCollections)[1]).toEqual({
      path: CATALOG,
      mappings: { '/Volumes/Old/': '/mnt/old', 'C:/Users/Benny/Pictures/': '/mnt/pictures' },
    });
    expect(document.body.textContent).toContain('modals.lightroomImport.relinked');
    expect(calls(Invokes.ImportLightroomCollections)).toHaveLength(0);
    expect(handleSettingsChange).not.toHaveBeenCalled();

    const addRoots = document.body.querySelector<HTMLInputElement>('input[type="checkbox"]');
    expect(addRoots).not.toBeNull();
    await act(async () => addRoots!.click());
    await act(async () => button('import').click());

    const mappings = { '/Volumes/Old/': '/mnt/old', 'C:/Users/Benny/Pictures/': '/mnt/pictures' };
    expect(calls(Invokes.ImportLightroomCollections)).toEqual([{ path: CATALOG, mappings }]);
    expect(useLibraryStore.getState().albumTree).toEqual(importedTree);
    expect(handleSettingsChange).toHaveBeenCalledTimes(1);
    expect(handleSettingsChange.mock.calls[0][0]).toMatchObject({
      lightroomPathMappings: mappings,
      rootFolders: ['/photos', '/mnt/pictures'],
    });
    expect(useLibraryStore.getState().rootPaths).toEqual(['/photos', '/mnt/pictures']);
    expect(refreshAllFolderTrees).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('cancels without importing or saving anything', async () => {
    const { handleSettingsChange, onClose } = await mount();

    await act(async () =>
      Array.from(document.body.querySelectorAll('button'))
        .find((item) => item.textContent === 'modals.confirm.cancel')!
        .click(),
    );

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(calls(Invokes.ImportLightroomCollections)).toHaveLength(0);
    expect(handleSettingsChange).not.toHaveBeenCalled();
  });
});
