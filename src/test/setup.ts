import { afterEach, vi } from 'vitest';
import { resetTauriMock } from './tauriMock';

vi.mock('@tauri-apps/api/core', async (importOriginal) => {
  const mock = await import('./tauriMock');
  return {
    ...(await importOriginal<typeof import('@tauri-apps/api/core')>()),
    invoke: mock.invoke,
    convertFileSrc: mock.convertFileSrc,
  };
});

vi.mock('@tauri-apps/api/event', async (importOriginal) => {
  const mock = await import('./tauriMock');
  return {
    ...(await importOriginal<typeof import('@tauri-apps/api/event')>()),
    listen: mock.listen,
    once: mock.once,
    emit: mock.emit,
  };
});

afterEach(() => {
  resetTauriMock();
});
