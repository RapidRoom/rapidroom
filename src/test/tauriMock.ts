// Test-only in-memory stand-in for the Tauri JS API. It has no app-specific
// imports, so it can be copied into any Tauri 2 frontend as-is.
import { vi } from 'vitest';

type CommandHandler = (args?: Record<string, unknown>) => unknown;
type TauriEvent<T = unknown> = { event: string; id: number; payload: T };
type EventHandler = (event: TauriEvent) => void;

const commands = new Map<string, CommandHandler>();
const listeners = new Map<string, Set<EventHandler>>();
let nextEventId = 1;

export function mockCommand(cmd: string, handler: CommandHandler) {
  commands.set(cmd, handler);
}

export const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
  const handler = commands.get(cmd);
  if (!handler) throw new Error(`tauriMock: no handler for command "${cmd}"`);
  return handler(args);
});

export const listen = vi.fn(async (event: string, handler: EventHandler) => {
  const handlers = listeners.get(event) ?? new Set<EventHandler>();
  listeners.set(event, handlers);
  const entry: EventHandler = (e) => handler(e);
  handlers.add(entry);
  return () => {
    handlers.delete(entry);
  };
});

export const once = vi.fn(async (event: string, handler: EventHandler) => {
  const unlisten = await listen(event, (e) => {
    unlisten();
    handler(e);
  });
  return unlisten;
});

export const emit = vi.fn(async (event: string, payload?: unknown) => {
  for (const handler of [...(listeners.get(event) ?? [])]) {
    handler({ event, id: nextEventId++, payload });
  }
});

export function listenerCount(event?: string) {
  if (event !== undefined) return listeners.get(event)?.size ?? 0;
  let total = 0;
  listeners.forEach((handlers) => (total += handlers.size));
  return total;
}

export function convertFileSrc(filePath: string, protocol = 'asset') {
  return `${protocol}://localhost/${encodeURIComponent(filePath)}`;
}

export function resetTauriMock() {
  commands.clear();
  listeners.clear();
  nextEventId = 1;
  invoke.mockClear();
  listen.mockClear();
  once.mockClear();
  emit.mockClear();
}
