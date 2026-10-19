import type { Adjustments } from './adjustments';

// Controls and Style metadata share one writer per photo. A slow earlier
// autosave must finish before a newer stack and its paired controls are saved.
const pending = new Map<string, Promise<unknown>>();
interface BufferedWrite {
  path: string;
  snapshot?: Adjustments;
  write: () => Promise<unknown>;
  onConsumed?: () => Promise<unknown>;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}
let paused: BufferedWrite[] | null = null;

export function queuePhotoWrite(
  path: string,
  write: () => Promise<unknown>,
  snapshot?: Adjustments,
  onConsumed?: () => Promise<unknown>,
): Promise<unknown> {
  if (paused)
    return new Promise((resolve, reject) => paused!.push({ path, snapshot, write, onConsumed, resolve, reject }));
  const previous = pending.get(path);
  const next = previous ? previous.catch(() => {}).then(write) : write();
  pending.set(path, next);
  void next
    .finally(() => {
      if (pending.get(path) === next) pending.delete(path);
    })
    .catch(() => {});
  return next;
}

export async function waitForPhotoWrites(): Promise<void> {
  let failure: PromiseRejectedResult | undefined;
  while (pending.size) {
    const results = await Promise.allSettled([...pending.values()]);
    for (const result of results) if (result.status === 'rejected') failure ??= result;
  }
  if (failure) throw failure.reason;
}

export function pausePhotoWrites(): (resume: (path: string, snapshot?: Adjustments) => boolean) => void {
  if (paused) throw new Error('Photo writes are already paused');
  const buffer: BufferedWrite[] = [];
  paused = buffer;
  return (resume) => {
    if (paused !== buffer) return;
    paused = null;
    for (const item of buffer) {
      if (resume(item.path, item.snapshot))
        void queuePhotoWrite(item.path, item.write, item.snapshot).then(item.resolve, item.reject);
      else if (item.onConsumed) void queuePhotoWrite(item.path, item.onConsumed).then(item.resolve, item.reject);
      else item.resolve(undefined);
    }
  };
}
