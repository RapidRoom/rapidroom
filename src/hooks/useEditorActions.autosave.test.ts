import { afterEach, describe, expect, it, vi } from 'vitest';
import { mockCommand, invoke } from '../test/tauriMock';
import { Invokes } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { useEditorStore } from '../store/useEditorStore';
import { debouncedSave, waitForPendingEditorSaves, prepareEditorDraftForXmpImport } from './useEditorActions';

const finishWrites: Array<() => void> = [];

function pendingWrite() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  finishWrites.push(resolve);
  return { promise, resolve, reject };
}

afterEach(async () => {
  debouncedSave.cancel();
  finishWrites.splice(0).forEach((finish) => finish());
  await waitForPendingEditorSaves();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('waiting for editor writes before XMP import', () => {
  it('does not save a clean photo selected while an older write drains', async () => {
    vi.useFakeTimers();
    const write = pendingWrite();
    let saves = 0;
    mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => (++saves === 1 ? write.promise : undefined));
    const image = (path: string) => ({
      path,
      exif: {},
      width: 100,
      height: 100,
      isRaw: true,
      isReady: true,
      thumbnailUrl: '',
    });
    const first = { ...INITIAL_ADJUSTMENTS, exposure: 1 };
    const clean = { ...INITIAL_ADJUSTMENTS, exposure: 2 };
    useEditorStore.setState({ selectedImage: image('/photos/a.NEF'), adjustments: first });
    debouncedSave('/photos/a.NEF', first);
    debouncedSave.flush();
    const preparing = prepareEditorDraftForXmpImport('/photos/a.NEF');
    useEditorStore.setState({ selectedImage: image('/photos/b.NEF'), adjustments: clean });
    write.resolve();
    const pause = await preparing;
    expect(pause.draft).toEqual({ path: '/photos/a.NEF', adjustments: first });
    pause.finish([]);
    vi.advanceTimersByTime(1000);
    await waitForPendingEditorSaves();
    expect(
      invoke.mock.calls
        .filter(([name]) => name === Invokes.SaveMetadataAndUpdateThumbnail)
        .map(([, args]) => args?.path),
    ).toEqual(['/photos/a.NEF']);
    expect(useEditorStore.getState().adjustments).toBe(clean);
  });

  it('waits for the latest started draft write', async () => {
    vi.useFakeTimers();
    const write = pendingWrite();
    mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => write.promise);
    debouncedSave('/photos/a.NEF', { ...INITIAL_ADJUSTMENTS, exposure: 0.5 });
    debouncedSave('/photos/a.NEF', { ...INITIAL_ADJUSTMENTS, exposure: 0.75 });
    vi.advanceTimersByTime(300);
    let finished = false;
    const flush = waitForPendingEditorSaves().then(() => {
      finished = true;
    });
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, {
      path: '/photos/a.NEF',
      adjustments: expect.objectContaining({ exposure: 0.75 }),
    });
    expect(finished).toBe(false);
    write.resolve();
    await flush;
    expect(finished).toBe(true);
  });

  it('waits for an already running autosave when no debounce is queued', async () => {
    vi.useFakeTimers();
    const write = pendingWrite();
    mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => write.promise);
    debouncedSave('/photos/a.NEF', INITIAL_ADJUSTMENTS);
    vi.advanceTimersByTime(300);
    let finished = false;
    const flush = waitForPendingEditorSaves().then(() => {
      finished = true;
    });
    await Promise.resolve();
    expect(finished).toBe(false);
    write.resolve();
    await flush;
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(finished).toBe(true);
  });

  it('holds new write requests across the whole pause and drains multiple existing writes', async () => {
    vi.useFakeTimers();
    const first = pendingWrite();
    const second = pendingWrite();
    let started = 0;
    mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => (++started === 1 ? first.promise : second.promise));
    useEditorStore.setState({ selectedImage: null });
    debouncedSave('/photos/a.NEF', INITIAL_ADJUSTMENTS);
    debouncedSave.flush();
    debouncedSave('/photos/b.NEF', INITIAL_ADJUSTMENTS);
    debouncedSave.flush();
    let prepared = false;
    const operation = prepareEditorDraftForXmpImport().then((pause) => {
      prepared = true;
      return pause;
    });
    const latest = { ...INITIAL_ADJUSTMENTS, exposure: 1.5 };
    debouncedSave('/photos/c.NEF', latest);
    debouncedSave.flush();
    vi.advanceTimersByTime(600);
    expect(started).toBe(2);
    first.resolve();
    for (let i = 0; i < 10; i++) await Promise.resolve();
    expect(prepared).toBe(false);
    second.resolve();
    const pause = await operation;
    expect(started).toBe(2);
    pause.finish(['/photos/a.NEF', '/photos/b.NEF']);
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, {
      path: '/photos/c.NEF',
      adjustments: latest,
    });
  });

  it('keeps the pause until every started write settles even when one fails', async () => {
    vi.useFakeTimers();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const first = pendingWrite();
    const second = pendingWrite();
    let started = 0;
    mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => (++started === 1 ? first.promise : second.promise));
    useEditorStore.setState({ selectedImage: null });
    debouncedSave('/photos/a.NEF', INITIAL_ADJUSTMENTS);
    debouncedSave.flush();
    debouncedSave('/photos/b.NEF', INITIAL_ADJUSTMENTS);
    debouncedSave.flush();
    let settled = false;
    const operation = prepareEditorDraftForXmpImport().finally(() => {
      settled = true;
    });
    const refusal = expect(operation).rejects.toThrow('disk full');
    const latest = { ...INITIAL_ADJUSTMENTS, exposure: 1.5 };
    debouncedSave('/photos/c.NEF', latest);
    first.reject(new Error('disk full'));
    for (let i = 0; i < 10; i++) await Promise.resolve();
    vi.advanceTimersByTime(600);
    expect(settled).toBe(false);
    expect(started).toBe(2);
    second.resolve();
    await refusal;
    vi.advanceTimersByTime(300);
    expect(invoke).toHaveBeenLastCalledWith(Invokes.SaveMetadataAndUpdateThumbnail, {
      path: '/photos/c.NEF',
      adjustments: latest,
    });
  });

  it('refuses to continue after a pending write fails', async () => {
    vi.useFakeTimers();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const write = pendingWrite();
    mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, () => write.promise);
    debouncedSave('/photos/a.NEF', INITIAL_ADJUSTMENTS);
    vi.advanceTimersByTime(300);
    const flush = waitForPendingEditorSaves();
    const refusal = expect(flush).rejects.toThrow('disk full');
    write.reject(new Error('disk full'));
    await refusal;
  });
});
