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
  it.each([false, true])(
    'does not autosave a settled editor after a table-only refusal (folder=%s)',
    async (folder) => {
      debouncedSave.cancel();
      debouncedSetHistory.flush();
      const refusal = { name: 'Brush group', reason: 'the companion .acr file is unavailable' };
      mockCommand(Invokes.ImportXmpAdjustmentsForImage, () => ({
        adjustments: { exposure: 2 },
        adjustmentsApplied: false,
        alreadyApplied: false,
        draftPreserved: false,
        unchanged: true,
        notTransferred: ['masks', 'maskBrushTable'],
        brushTableRefusals: [refusal],
      }));
      mockCommand(Invokes.ImportMatchingXmpSidecarsInFolder, () => ({
        matched: 1,
        imported: 0,
        unchanged: 1,
        skipped: 0,
        failed: 0,
        failures: [],
        importedPaths: [],
        adjustmentsImportedPaths: [],
        alreadyAppliedPaths: [],
        preservedDraftPaths: [],
        unchangedPaths: [path],
        notTransferred: [{ path, items: ['masks', 'maskBrushTable'], brushTableRefusals: [refusal] }],
      }));
      await clickImport(folder);
      vi.advanceTimersByTime(1200);
      expect(useEditorStore.getState().adjustments).toBe(draft);
      expect(useEditorStore.getState().history).toEqual([INITIAL_ADJUSTMENTS, draft]);
      expect(invoke.mock.calls.some(([name]) => name === Invokes.SaveMetadataAndUpdateThumbnail)).toBe(false);
    },
  );

  it('preserves the live draft and queued history on AlreadyApplied metadata-only import, without autosaving into the incoming packet', async () => {
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, (args) => {
      expect(args?.pendingAdjustments).toBe(draft);
      expect(invoke.mock.calls.some(([name]) => name === Invokes.SaveMetadataAndUpdateThumbnail)).toBe(false);
      return { adjustments: { exposure: 4 }, adjustmentsApplied: false, alreadyApplied: true, draftPreserved: true };
    });
    await clickImport();
    expect(useEditorStore.getState().adjustments).toBe(draft);
    expect(useEditorStore.getState().history).toEqual([INITIAL_ADJUSTMENTS, draft]);
    debouncedSave(path, useEditorStore.getState().adjustments);
    vi.advanceTimersByTime(600);
    expect(invoke).toHaveBeenCalledTimes(1);
    const changed = { ...draft, exposure: 1.5 };
    debouncedSave(path, changed);
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: changed });
  });

  it('keeps report-only imports read-only and resumes the unconsumed live draft autosave', async () => {
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, (args) => {
      expect(args?.pendingAdjustments).toBe(draft);
      return {
        adjustments: { exposure: 2 },
        adjustmentsApplied: false,
        alreadyApplied: false,
        draftPreserved: false,
        unchanged: true,
        settingsReport: {
          mapped: 0,
          approximated: 0,
          ignored: 1,
          omitted: 0,
          settings: [{ field: 'PerspectiveVertical', outcome: 'ignored', reason: 'perspective' }],
        },
      };
    });
    await clickImport();
    expect(useEditorStore.getState().adjustments).toBe(draft);
    expect(useEditorStore.getState().history).toEqual([INITIAL_ADJUSTMENTS, draft]);
    expect(invoke.mock.calls.map(([name]) => name)).toEqual([Invokes.ImportXmpAdjustmentsForImage]);
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: draft });
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
    // useImageProcessing echoes newly applied adjustments into autosave.
    debouncedSave(path, useEditorStore.getState().adjustments);
    vi.advanceTimersByTime(600);
    expect(invoke).toHaveBeenCalledTimes(1);
    const newer = { ...useEditorStore.getState().adjustments, exposure: 2 };
    debouncedSave(path, newer);
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: newer });
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

  it('a failed folder import resumes the newest draft queued while import awaited', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    let fail!: (error: Error) => void;
    mockCommand(
      Invokes.ImportMatchingXmpSidecarsInFolder,
      () =>
        new Promise((_resolve, reject) => {
          fail = reject;
        }),
    );
    const operation = clickImport(true);
    for (let i = 0; i < 10; i++) await Promise.resolve();
    const latest = { ...draft, exposure: 1.5 };
    useEditorStore.setState({ adjustments: latest });
    debouncedSave(path, latest);
    debouncedSave.flush();
    vi.advanceTimersByTime(600);
    expect(invoke.mock.calls.map(([name]) => name)).toEqual([Invokes.ImportMatchingXmpSidecarsInFolder]);
    fail(new Error('read failed'));
    await operation;
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: latest });
  });

  it('cancelled file selection resumes the newer live draft instead of its earlier snapshot', async () => {
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, () => {
      throw new Error('No matching XMP sidecar');
    });
    let cancel!: (value: null) => void;
    mocks.open.mockImplementation(
      () =>
        new Promise<null>((resolve) => {
          cancel = resolve;
        }),
    );
    const operation = clickImport();
    for (let i = 0; i < 10; i++) await Promise.resolve();
    const latest = { ...draft, exposure: 1.5 };
    useEditorStore.setState({ adjustments: latest });
    debouncedSave(path, latest);
    cancel(null);
    await operation;
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: latest });
  });

  it('edits made during a successful development import remain the latest active draft', async () => {
    let complete!: (value: unknown) => void;
    mockCommand(
      Invokes.ImportXmpAdjustmentsForImage,
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    );
    const operation = clickImport();
    for (let i = 0; i < 10; i++) await Promise.resolve();
    const latest = { ...draft, exposure: 1.5 };
    useEditorStore.setState({ adjustments: latest });
    debouncedSave(path, latest);
    complete({ adjustments: { exposure: 4 }, adjustmentsApplied: true, alreadyApplied: false, draftPreserved: false });
    await operation;
    expect(useEditorStore.getState().adjustments).toBe(latest);
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, { path, adjustments: latest });
  });

  it('preserves the target draft and queued edits to a newly selected photo while an existing write drains', async () => {
    let finish!: () => void;
    mockCommand(
      Invokes.SaveMetadataAndUpdateThumbnail,
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    vi.advanceTimersByTime(300);
    mockCommand(Invokes.ImportXmpAdjustmentsForImage, (args) => {
      expect(args?.pendingAdjustments).toBe(draft);
      return { adjustments: {}, adjustmentsApplied: false, alreadyApplied: true, draftPreserved: true };
    });
    const operation = clickImport();
    const other = { ...draft, exposure: 2 };
    const selected = useEditorStore.getState().selectedImage!;
    useEditorStore.setState({ selectedImage: { ...selected, path: '/photos/b.NEF' }, adjustments: other });
    debouncedSave('/photos/b.NEF', other);
    finish();
    await operation;
    expect(useEditorStore.getState().adjustments).toBe(other);
    mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => undefined);
    vi.advanceTimersByTime(300);
    expect(
      invoke.mock.calls.filter(
        ([name, args]) => name === Invokes.SaveMetadataAndUpdateThumbnail && args?.path === path,
      ),
    ).toHaveLength(1);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, {
      path: '/photos/b.NEF',
      adjustments: other,
    });
  });

  it('folder import keeps newer edits to a photo selected while the import was pending', async () => {
    let complete!: (value: unknown) => void;
    mockCommand(
      Invokes.ImportMatchingXmpSidecarsInFolder,
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    );
    const operation = clickImport(true);
    for (let i = 0; i < 10; i++) await Promise.resolve();
    const otherPath = '/photos/b.NEF';
    const other = { ...draft, exposure: 2 };
    useEditorStore.setState({
      selectedImage: { ...useEditorStore.getState().selectedImage!, path: otherPath },
      adjustments: other,
    });
    debouncedSave(otherPath, other);
    mockCommand(Invokes.LoadMetadata, () => ({ adjustments: { exposure: 4 } }));
    complete({
      matched: 1,
      imported: 1,
      unchanged: 0,
      skipped: 0,
      failed: 0,
      failures: [],
      importedPaths: [otherPath],
      adjustmentsImportedPaths: [otherPath],
      alreadyAppliedPaths: [],
      preservedDraftPaths: [],
      unchangedPaths: [],
      notTransferred: [],
    });
    await operation;
    expect(useEditorStore.getState().adjustments).toBe(other);
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, {
      path: otherPath,
      adjustments: other,
    });
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
    debouncedSave(path, useEditorStore.getState().adjustments);
    vi.advanceTimersByTime(600);
    expect(invoke.mock.calls.map(([name]) => name)).toEqual([Invokes.ImportMatchingXmpSidecarsInFolder]);
  });
});
