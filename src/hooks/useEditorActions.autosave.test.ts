import { afterEach, describe, expect, it, vi } from 'vitest';
import { mockCommand, invoke } from '../test/tauriMock';
import { Invokes } from '../components/ui/AppProperties';
import { rememberAppliedStyleStack, hashPresetAdjustments } from '../utils/appliedPreset';
import { changeStyleStack } from '../utils/styleStack';
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

it('serializes a delayed older autosave before a newer paired Style stack and controls', async () => {
  vi.useFakeTimers();
  const first = pendingWrite();
  const path = '/queue-style.raw';
  const writes: number[] = [];
  let controls = 0;
  mockCommand(Invokes.SaveMetadataAndUpdateThumbnail, async (args) => {
    const exposure = (args?.adjustments as typeof INITIAL_ADJUSTMENTS).exposure;
    if (exposure === 1) await first.promise;
    controls = exposure;
    writes.push(exposure);
  });
  mockCommand('save_applied_preset', (args) => {
    controls = (args?.adjustments as typeof INITIAL_ADJUSTMENTS).exposure;
    writes.push(controls);
  });
  debouncedSave(path, { ...INITIAL_ADJUSTMENTS, exposure: 1 });
  debouncedSave.flush();
  debouncedSave(path, { ...INITIAL_ADJUSTMENTS, exposure: 2 });
  debouncedSave.flush();
  const base = { ...INITIAL_ADJUSTMENTS, exposure: 2 };
  const stack = changeStyleStack(
    null,
    base,
    { preset: { id: 'look', name: 'Look', presetType: 'style', adjustments: { exposure: 3 } }, amount: 100 },
    { width: 6000, height: 4000 },
  );
  await hashPresetAdjustments(stack.result);
  const saved = rememberAppliedStyleStack(path, stack, stack.result);
  await vi.advanceTimersByTimeAsync(300);
  expect(writes).toEqual([]);
  first.resolve();
  await saved;
  await waitForPendingEditorSaves();
  expect(writes).toEqual([1, 2, 3]);
  expect(controls).toBe(3);
});

it('holds paired Style writes across native XMP import and retains only their recipe after the old controls are consumed', async () => {
  vi.useFakeTimers();
  const path = '/styles-import.raw';
  const stack = changeStyleStack(
    null,
    INITIAL_ADJUSTMENTS,
    { preset: { id: 'look', name: 'Look', presetType: 'style', adjustments: { contrast: 40 } }, amount: 100 },
    { width: 6000, height: 4000 },
  );
  useEditorStore.setState({
    selectedImage: { path, width: 6000, height: 4000, isReady: true, isRaw: true, thumbnailUrl: '', exif: {} },
    adjustments: stack.result,
  });
  await hashPresetAdjustments(stack.result);
  const writes: unknown[] = [];
  mockCommand('save_applied_preset', (args) => writes.push(args));
  const saving = rememberAppliedStyleStack(path, stack, stack.result);
  const pause = await prepareEditorDraftForXmpImport(path);
  await vi.advanceTimersByTimeAsync(400);
  expect(writes).toEqual([]);
  pause.finish([path]);
  await saving;
  expect(writes).toHaveLength(1);
  expect(writes[0]).toMatchObject({ path, record: { stack: { entries: [{ presetId: 'look', amount: 100 }] } } });
  expect(writes[0]).not.toHaveProperty('adjustments');
});

it('drains an in-flight paired Style save before permitting a native XMP import', async () => {
  vi.useFakeTimers();
  const path = '/styles-import-inflight.raw';
  const stack = changeStyleStack(
    null,
    INITIAL_ADJUSTMENTS,
    { preset: { id: 'look', name: 'Look', presetType: 'style', adjustments: { contrast: 40 } }, amount: 100 },
    { width: 6000, height: 4000 },
  );
  useEditorStore.setState({
    selectedImage: { path, width: 6000, height: 4000, isReady: true, isRaw: true, thumbnailUrl: '', exif: {} },
    adjustments: stack.result,
  });
  await hashPresetAdjustments(stack.result);
  const write = pendingWrite();
  let started = false;
  mockCommand('save_applied_preset', () => {
    started = true;
    return write.promise;
  });
  const saving = rememberAppliedStyleStack(path, stack, stack.result);
  await vi.advanceTimersByTimeAsync(400);
  expect(started).toBe(true);
  let ready = false;
  const preparing = prepareEditorDraftForXmpImport(path).then((pause) => {
    ready = true;
    return pause;
  });
  await Promise.resolve();
  expect(ready).toBe(false);
  write.resolve();
  const pause = await preparing;
  expect(ready).toBe(true);
  pause.finish([]);
  await saving;
});

it.each([
  { active: true, delay: 0 },
  { active: false, delay: 0 },
  { active: false, delay: 400 },
])(
  'preserves imported controls when the pending Style intent is active=$active and import finishes after $delay ms',
  async ({ active, delay }) => {
    vi.useFakeTimers();
    const path = `/style-intent-${active}-${delay}.raw`;
    const stack = changeStyleStack(
      null,
      INITIAL_ADJUSTMENTS,
      {
        preset: { id: 'look', name: 'Look', presetType: 'style', adjustments: { exposure: 2 } },
        amount: 100,
      },
      { width: 6000, height: 4000 },
    );
    useEditorStore.setState({
      selectedImage: {
        path: active ? path : '/other-active.raw',
        width: 6000,
        height: 4000,
        isReady: true,
        isRaw: true,
        thumbnailUrl: '',
        exif: {},
      },
      adjustments: active ? stack.result : INITIAL_ADJUSTMENTS,
    });
    await hashPresetAdjustments(stack.result);
    let diskExposure: number;
    const writes: Array<Record<string, unknown>> = [];
    mockCommand('save_applied_preset', (args) => {
      writes.push(args!);
      if (args?.adjustments) diskExposure = (args.adjustments as typeof INITIAL_ADJUSTMENTS).exposure;
    });
    const saving = rememberAppliedStyleStack(path, stack, stack.result);
    const pause = await prepareEditorDraftForXmpImport(path);
    expect(pause.draft?.adjustments).toBe(stack.result);
    diskExposure = 3; // A real importer owns the sidecar during this pause.
    if (delay) await vi.advanceTimersByTimeAsync(delay);
    expect(writes).toEqual([]);
    pause.finish([path]);
    await vi.advanceTimersByTimeAsync(400);
    await saving;
    expect(diskExposure).toBe(3);
    expect(writes).toHaveLength(1);
    expect(writes[0]).not.toHaveProperty('adjustments');
    expect(writes[0]).toMatchObject({ record: { stack: { entries: [{ presetId: 'look', amount: 100 }] } } });
  },
);
