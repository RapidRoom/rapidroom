import type { ImageFile } from '../components/ui/AppProperties';

interface MetadataWrite {
  apply(): () => void;
  save(): Promise<unknown>;
  onError(error: unknown): void;
  onSaved?(): void;
  visibleOrder?: readonly ImageFile[];
  undo: () => void;
}

const pending: MetadataWrite[] = [];
const savedCallbacks: Array<() => void> = [];

export const getPendingLibraryVisibleOrder = () => pending[0]?.visibleOrder;

function runAfterSave(callback: () => void) {
  try {
    callback();
  } catch (error) {
    console.error('Failed to advance the library selection:', error);
  }
}

export function afterPendingLibraryMetadataWrites(callback: () => void) {
  if (pending.length) savedCallbacks.push(callback);
  else runAfterSave(callback);
}

// Save ratings and flags in action order. On failure, rebuild later optimistic
// changes from the last saved state instead of restoring an outdated snapshot.
async function savePending() {
  while (pending.length > 0) {
    const write = pending[0];
    try {
      await write.save();
      pending.shift();
      if (write.onSaved) savedCallbacks.push(write.onSaved);
    } catch (error) {
      for (const item of [...pending].reverse()) item.undo();
      pending.shift();
      for (const item of pending) item.undo = item.apply();
      write.onError(error);
    }
  }
  for (const callback of savedCallbacks.splice(0)) runAfterSave(callback);
}

export function enqueueLibraryMetadataWrite(write: Omit<MetadataWrite, 'undo'>) {
  const first = pending.length === 0;
  pending.push({ ...write, undo: write.apply() });
  if (first) void savePending();
}
