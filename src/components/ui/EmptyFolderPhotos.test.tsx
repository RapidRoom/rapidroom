// @vitest-environment jsdom
import React from 'react';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import EmptyFolderPhotos from './EmptyFolderPhotos';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('./Button', () => ({
  default: ({ children, onClick }: { children: React.ReactNode; onClick: () => void }) => (
    <button onClick={onClick}>{children}</button>
  ),
}));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const container = document.createElement('div');
document.body.append(container);
let root = createRoot(container);
afterEach(async () => {
  await act(async () => root.unmount());
  root = createRoot(container);
  invoke.mockReset();
});
it('ignores a late old-folder scan and offers recursive mode only from the current result', async () => {
  let resolveOld!: (value: { count: number; complete: boolean }) => void;
  let resolveNew!: (value: { count: number; complete: boolean }) => void;
  invoke
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
    )
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveNew = resolve;
        }),
    );
  const show = vi.fn();
  await act(async () => root.render(<EmptyFolderPhotos path="/old" onShowSubfolders={show} />));
  await act(async () => root.render(<EmptyFolderPhotos path="/new" onShowSubfolders={show} />));
  await act(async () => resolveOld({ count: 99, complete: true }));
  expect(container.querySelector('button')).toBeNull();
  await act(async () => resolveNew({ count: 2, complete: true }));
  await act(async () => container.querySelector('button')!.click());
  expect(show).toHaveBeenCalledOnce();
  expect(invoke.mock.calls.map((call) => call[1])).toEqual([
    { path: '/old', recursive: true },
    { path: '/new', recursive: true },
  ]);
});
it('keeps a manual recursive action when a bounded scan cannot finish, without claiming a photo count', async () => {
  invoke.mockResolvedValue({ count: 0, complete: false });
  await act(async () => root.render(<EmptyFolderPhotos path="/huge" onShowSubfolders={() => {}} />));
  expect(container.textContent).toContain('scanLimited');
  expect(container.querySelector('button')).not.toBeNull();
  expect(container.textContent).not.toContain('descendants');
});
