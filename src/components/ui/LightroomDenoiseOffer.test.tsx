// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '../../test/tauriMock';
import { useUIStore } from '../../store/useUIStore';
import LightroomDenoiseOffer, { type XmpDenoiseRequest } from './LightroomDenoiseOffer';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: unknown) => `${key} ${JSON.stringify(values ?? {})}`,
  }),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialUI = useUIStore.getState();
let root: Root | undefined;
let container: HTMLDivElement;

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  container?.remove();
  useUIStore.setState(initialUI, true);
});

async function mount(requests: XmpDenoiseRequest[]) {
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  const review = vi.fn((paths: string[]) => useUIStore.getState().openDenoiseModal(paths, true));
  await act(async () => root?.render(<LightroomDenoiseOffer requests={requests} onReview={review} />));
  return review;
}

describe('Lightroom AI Denoise review offer', () => {
  it('stages the existing RAW dialog only after Review, keeping Lightroom amount as reference', async () => {
    const review = await mount([{ path: '/photos/source.ARW', amount: 37 }]);
    expect(review).not.toHaveBeenCalled();
    expect(useUIStore.getState().denoiseModalState.isOpen).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
    expect(container.textContent).toContain('"amount":37');
    await act(async () => container.querySelector('button')?.click());
    expect(review).toHaveBeenCalledWith(['/photos/source.ARW']);
    expect(useUIStore.getState().denoiseModalState).toMatchObject({
      isOpen: true,
      isRaw: true,
      isProcessing: false,
      targetPaths: ['/photos/source.ARW'],
      jobId: null,
    });
    expect(invoke).not.toHaveBeenCalled();
  });

  it('groups unique RAW paths without inventing a batch strength', async () => {
    const review = await mount([
      { path: '/photos/a.ARW', amount: 37 },
      { path: '/photos/b.ORF', amount: 50 },
      { path: '/photos/a.ARW', amount: 37 },
    ]);
    expect(container.textContent).toContain('"count":2');
    expect(container.textContent).not.toContain('denoiseAmount');
    await act(async () => container.querySelector('button')?.click());
    expect(review).toHaveBeenCalledWith(['/photos/a.ARW', '/photos/b.ORF']);
  });

  it('offers nothing when the native importer provides no eligible request', async () => {
    await mount([]);
    expect(container.querySelector('button')).toBeNull();
  });

  it('retains the offer while another denoise preview or job is open', async () => {
    const review = await mount([{ path: '/photos/source.ARW', amount: null }]);
    await act(async () => useUIStore.getState().openDenoiseModal(['/photos/other.ARW'], true));
    expect(container.querySelector('button')?.disabled).toBe(true);
    await act(async () => container.querySelector('button')?.click());
    expect(review).not.toHaveBeenCalled();
    await act(async () => useUIStore.getState().closeDenoiseModal());
    expect(container.querySelector('button')?.disabled).toBe(false);
  });
});
