import { createElement, type MouseEvent } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppContextMenus, type UseAppContextMenusProps } from './useAppContextMenus';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { Invokes, type Option } from '../components/ui/AppProperties';
import { invoke, mockCommand } from '../test/tauriMock';
import { debouncedSave, debouncedSetHistory } from './useEditorActions';

const mocks = vi.hoisted(() => ({ menu: vi.fn(), open: vi.fn() }));
vi.mock('../context/ContextMenuContext', () => ({ useContextMenu: () => ({ showContextMenu: mocks.menu }) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

const path = '/photos/a.NEF';
const event = {
  preventDefault: vi.fn(),
  stopPropagation: vi.fn(),
  clientX: 1,
  clientY: 1,
} as unknown as MouseEvent<HTMLElement>;
const draft = { ...INITIAL_ADJUSTMENTS, exposure: 0.75 };
function menus() {
  let result!: ReturnType<typeof useAppContextMenus>;
  function Probe() {
    result = useAppContextMenus({ refreshImageList: vi.fn(async () => {}) } as unknown as UseAppContextMenusProps);
    return null;
  }
  renderToStaticMarkup(createElement(Probe));
  return result;
}
async function clickImport(folder = false) {
  const actions = menus();
  if (folder) actions.handleFolderTreeContextMenu(event, '/photos');
  else actions.handleEditorContextMenu(event);
  const options = mocks.menu.mock.calls.at(-1)![2] as Option[];
  const item = options.find(
    (option) =>
      option.label ===
      (folder ? 'contextMenus.folders.importMatchingXmpSidecars' : 'contextMenus.editor.importXmpAdjustments'),
  )!;
  expect(item).toBeDefined();
  await item.onClick!();
}

beforeEach(() => {
  vi.useFakeTimers();
  useLibraryStore.setState({
    cardBrowseRoot: null,
    libraryActivePath: path,
    currentFolderPath: '/photos',
    imageList: [],
  });
  useEditorStore.getState().resetHistory(INITIAL_ADJUSTMENTS);
  useEditorStore.setState({
    selectedImage: { path, exif: {}, height: 100, width: 100, isRaw: true, isReady: true, thumbnailUrl: '' },
    adjustments: draft,
  });
  debouncedSetHistory(draft);
  debouncedSave(path, draft);
  mocks.menu.mockClear();
  mocks.open.mockReset();
  mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => undefined);
});
afterEach(() => {
  debouncedSave.cancel();
  debouncedSetHistory.cancel();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('public XMP import state and history', () => {
  it('preserves the live draft and queued history on AlreadyApplied metadata-only import, without autosaving into the incoming packet', async () => {
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, (args) => {
      expect(args?.pendingAdjustments).toBe(draft);
      expect(invoke.mock.calls.some(([name]) => name === Invokes.SaveMetadataAndUpdateThumbnail)).toBe(false);
      return { adjustments: { exposure: 4 }, adjustmentsApplied: false, alreadyApplied: true, draftPreserved: true };
    });
    await clickImport();
    expect(useEditorStore.getState().adjustments).toBe(draft);
    expect(useEditorStore.getState().history).toEqual([INITIAL_ADJUSTMENTS, draft]);
    vi.advanceTimersByTime(600);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('waits for a previously started editor write before importing', async () => {
    let finish!: () => void;
    mockCommand(
      Invokes.SaveMetadataAndUpdateThumbnail,
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    vi.advanceTimersByTime(300);
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, () => ({
      adjustments: {},
      adjustmentsApplied: false,
      alreadyApplied: true,
      draftPreserved: true,
    }));
    const operation = clickImport();
    await Promise.resolve();
    expect(invoke.mock.calls.map(([name]) => name)).toEqual([Invokes.SaveMetadataAndUpdateThumbnail]);
    finish();
    await operation;
    expect(invoke.mock.calls.map(([name]) => name)).toEqual([
      Invokes.SaveMetadataAndUpdateThumbnail,
      Invokes.ImportXmpAdjustmentsForImage,
    ]);
    expect(useEditorStore.getState().adjustments).toBe(draft);
  });

  it('applies ordinary development import and resets history to its new state', async () => {
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, () => ({
      adjustments: { exposure: 1.25 },
      adjustmentsApplied: true,
      alreadyApplied: false,
      draftPreserved: false,
    }));
    await clickImport();
    expect(useEditorStore.getState().adjustments.exposure).toBe(1.25);
    expect(useEditorStore.getState().history).toHaveLength(1);
    vi.advanceTimersByTime(600);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('resumes the draft autosave if choosing a sidecar is cancelled', async () => {
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, () => {
      throw new Error('No matching XMP sidecar');
    });
    mocks.open.mockResolvedValue(null);
    await clickImport();
    expect(useEditorStore.getState().adjustments).toBe(draft);
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: draft });
  });

  it('folder metadata-only imports preserve the active history and send its pending draft', async () => {
    mockCommand(Invokes.ImportMatchingXmpSidecarsInFolder, (args) => {
      expect(args?.pendingDraft).toEqual({ path, adjustments: draft });
      return {
        matched: 1,
        imported: 1,
        unchanged: 0,
        skipped: 0,
        failed: 0,
        failures: [],
        importedPaths: [path],
        adjustmentsImportedPaths: [],
        alreadyAppliedPaths: [path],
        preservedDraftPaths: [path],
        unchangedPaths: [],
        notTransferred: [],
      };
    });
    await clickImport(true);
    expect(useEditorStore.getState().adjustments).toBe(draft);
    expect(useEditorStore.getState().history).toEqual([INITIAL_ADJUSTMENTS, draft]);
    vi.advanceTimersByTime(600);
    expect(invoke.mock.calls.map(([name]) => name)).toEqual([Invokes.ImportMatchingXmpSidecarsInFolder]);
  });
});
