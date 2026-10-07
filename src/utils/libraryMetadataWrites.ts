interface MetadataWrite {
  apply(): () => void;
  save(): Promise<unknown>;
  onError(error: unknown): void;
  onSaved?(): void;
  undo: () => void;
}

const pending: MetadataWrite[] = [];

// Save ratings and flags in action order. On failure, rebuild later optimistic
// changes from the last saved state instead of restoring an outdated snapshot.
async function savePending() {
  while (pending.length > 0) {
    const write = pending[0];
    try {
      await write.save();
      pending.shift();
      try {
        write.onSaved?.();
      } catch (error) {
        console.error('Failed to advance the library selection:', error);
      }
    } catch (error) {
      for (const item of [...pending].reverse()) item.undo();
      pending.shift();
      for (const item of pending) item.undo = item.apply();
      write.onError(error);
    }
  }
}

export function enqueueLibraryMetadataWrite(write: Omit<MetadataWrite, 'undo'>) {
  const first = pending.length === 0;
  pending.push({ ...write, undo: write.apply() });
  if (first) void savePending();
}
