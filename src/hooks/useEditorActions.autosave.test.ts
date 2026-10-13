import { afterEach, describe, expect, it, vi } from 'vitest';
import { mockCommand, invoke } from '../test/tauriMock';
import { Invokes } from '../components/ui/AppProperties';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { debouncedSave, waitForPendingEditorSaves } from './useEditorActions';

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
