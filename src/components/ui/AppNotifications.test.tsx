// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { toast, type ToastContentProps } from 'react-toastify';
import AppNotifications from './AppNotifications';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: ReturnType<typeof createRoot> | undefined;
let host: HTMLDivElement;
afterEach(async () => {
  await act(async () => {
    toast.dismiss();
    root?.unmount();
  });
  host?.remove();
  toast.clearWaitingQueue();
  vi.useRealTimers();
});
const fixtureFailure = 'Source file not found: an owned fixture path';
const notice = ({ toastProps }: ToastContentProps) => (
  <span data-notice={toastProps.toastId} data-auto-close={String(toastProps.autoClose)}>
    {fixtureFailure}
  </span>
);
it('retains queued startup errors and progress failures while later successes keep their timeout', async () => {
  vi.useFakeTimers();
  toast.error(notice, { toastId: 'startup-error' });
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<AppNotifications isLightTheme={false} />));
  await act(async () => vi.advanceTimersByTimeAsync(300));
  expect(host.querySelector('[data-notice=startup-error]')?.getAttribute('data-auto-close')).toBe('false');
  await act(async () => {
    toast.loading(notice, { toastId: 'transfer' });
    await vi.advanceTimersByTimeAsync(20);
    toast.update('transfer', { render: notice, type: 'error', isLoading: false, autoClose: 5000 });
    await vi.advanceTimersByTimeAsync(300);
  });
  expect(host.querySelector('[data-notice=transfer]')?.getAttribute('data-auto-close')).toBe('false');
  await act(async () => {
    for (let n = 0; n < 3; n++) toast.success(notice, { toastId: `success-${n}` });
    await vi.advanceTimersByTimeAsync(300);
  });
  expect(host.querySelector('[data-notice=startup-error]')?.getAttribute('data-auto-close')).toBe('false');
  for (let n = 0; n < 3; n++) {
    expect(host.querySelector(`[data-notice=success-${n}]`)?.getAttribute('data-auto-close')).toBe('5000');
  }
  const error = host.querySelector('[data-notice=startup-error]')!.closest('.Toastify__toast')!;
  await act(async () => error.dispatchEvent(new MouseEvent('click', { bubbles: true })));
  expect(toast.isActive('startup-error')).toBe(true);
  await act(async () => error.querySelector('button')!.dispatchEvent(new MouseEvent('click', { bubbles: true })));
  expect(toast.isActive('startup-error')).toBe(false);
});
