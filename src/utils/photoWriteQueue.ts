import type { Adjustments } from './adjustments';

// Controls and Style metadata share one writer per photo. A slow earlier
// autosave must finish before a newer stack and its paired controls are saved.
const pending = new Map<string, Promise<unknown>>();
export interface PhotoWriteIntent {
  path: string;
  snapshot: Adjustments;
  consumed: boolean;
  controls: boolean;
  release(): void;
}
const intents = new Set<PhotoWriteIntent>();
let pausedIntents: Set<PhotoWriteIntent> | null = null;
export function registerPhotoWriteIntent(path: string, snapshot: Adjustments, controls = true): PhotoWriteIntent {
  const intent: PhotoWriteIntent = { path, snapshot, controls, consumed: false, release: () => intents.delete(intent) };
  intents.add(intent);
  pausedIntents?.add(intent);
  return intent;
}
export function pendingPhotoWriteDrafts(): Array<{ path: string; adjustments: Adjustments }> {
  const latest = new Map<string, Adjustments>();
  for (const intent of intents) if (intent.controls && !intent.consumed) latest.set(intent.path, intent.snapshot);
  return [...latest].map(([path, adjustments]) => ({ path, adjustments }));
}
interface BufferedWrite {
  path: string;
  snapshot?: Adjustments;
  write: () => Promise<unknown>;
  onConsumed?: () => Promise<unknown>;
  intent?: PhotoWriteIntent;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}
let paused: BufferedWrite[] | null = null;

export function queuePhotoWrite(
  path: string,
  write: () => Promise<unknown>,
  snapshot?: Adjustments,
  onConsumed?: () => Promise<unknown>,
  intent?: PhotoWriteIntent,
): Promise<unknown> {
  if (paused)
    return new Promise((resolve, reject) =>
      paused!.push({ path, snapshot, write, onConsumed, intent, resolve, reject }),
    );
  const run = () => (intent?.consumed ? (onConsumed?.() ?? Promise.resolve()) : write());
  const previous = pending.get(path);
  const next = previous ? previous.catch(() => {}).then(run) : run();
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
  const participating = new Set(intents);
  pausedIntents = participating;
  return (resume) => {
    if (paused !== buffer) return;
    for (const intent of participating) if (!resume(intent.path, intent.snapshot)) intent.consumed = true;
    pausedIntents = null;
    paused = null;
    for (const item of buffer) {
      if (item.intent || resume(item.path, item.snapshot))
        void queuePhotoWrite(item.path, item.write, item.snapshot, item.onConsumed, item.intent).then(
          item.resolve,
          item.reject,
        );
      else if (item.onConsumed) void queuePhotoWrite(item.path, item.onConsumed).then(item.resolve, item.reject);
      else item.resolve(undefined);
    }
  };
}
