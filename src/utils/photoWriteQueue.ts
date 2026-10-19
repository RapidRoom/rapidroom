// Controls and Style metadata share one writer per photo. A slow earlier
// autosave must finish before a newer stack and its paired controls are saved.
const pending = new Map<string, Promise<unknown>>();
export function queuePhotoWrite<T>(path: string, write: () => Promise<T>): Promise<T> {
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
