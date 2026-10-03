// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { emit, invoke, mockCommand } from '../../test/tauriMock';
import { useUIStore } from '../../store/useUIStore';
import { useProcessStore } from '../../store/useProcessStore';
import { useProductivityActions } from '../../hooks/useProductivityActions';
import { useTauriListeners } from '../../hooks/useTauriListeners';
import { Invokes } from '../ui/AppProperties';
import DenoiseModal from './DenoiseModal';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialUI = useUIStore.getState();
const initialProcess = useProcessStore.getState();
let root: Root | undefined;
let container: HTMLDivElement;

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  container?.remove();
  useUIStore.setState(initialUI, true);
  useProcessStore.setState(initialProcess, true);
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

async function mount(paths = ['/photos/a.raw']) {
  mockCommand(Invokes.IsRaw9Available, () => false);
  const refreshImageList = vi.fn(async () => {});
  const listenerProps = {
    refreshAllFolderTrees: vi.fn(),
    handleSelectSubfolder: vi.fn(),
    refreshImageList,
    markGenerated: vi.fn(),
  };
  let actions!: ReturnType<typeof useProductivityActions>;
  function Harness() {
    const { denoiseModalState: state, pendingDenoiseJob, closeDenoiseModal } = useUIStore();
    actions = useProductivityActions(refreshImageList);
    useTauriListeners(listenerProps);
    return (
      <DenoiseModal
        {...state}
        isProcessing={state.isProcessing || pendingDenoiseJob !== null}
        originalBase64={state.originalBase64 || null}
        onClose={closeDenoiseModal}
        onDenoise={actions.handleApplyDenoise}
        onBatchDenoise={actions.handleBatchDenoise}
        onSave={actions.handleSaveDenoisedImage}
        onOpenFile={vi.fn()}
        aiModelDownloadStatus={null}
      />
    );
  }
  useUIStore.getState().openDenoiseModal(paths, true);
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root?.render(<Harness />));
  return { actions, refreshImageList };
}

function button(key: string) {
  const found = Array.from(container.querySelectorAll('button')).find(
    (item) => item.textContent === `modals.denoise.${key}`,
  );
  if (!found) throw new Error(`Button ${key} not found`);
  return found;
}

async function click(target: Element) {
  await act(async () => {
    target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    (target as HTMLElement).click();
  });
}

const backdrop = () => container.firstElementChild!;
const state = () => useUIStore.getState();

describe('Denoise dialog cancellation', () => {
  it('dismisses an idle backdrop, but not clicks inside the dialog', async () => {
    await mount();
    await click(backdrop().firstElementChild!);
    expect(state().denoiseModalState.isOpen).toBe(true);
    await click(backdrop());
    expect(state().denoiseModalState.isOpen).toBe(false);
  });

  it('blocks busy backdrop clicks, enables Cancel, ignores late results and prevents reopening until the command finishes', async () => {
    const job = deferred<void>();
    mockCommand(Invokes.ApplyDenoising, () => job.promise);
    const { actions } = await mount();
    await click(button('btnStart'));
    expect(state().pendingDenoiseJob).toBe('single');
    await click(backdrop());
    expect(state().denoiseModalState.isOpen).toBe(true);
    expect(button('cancel').disabled).toBe(false);
    expect(button('cancel').closest('.pointer-events-none')).toBeNull();
    await click(button('cancel'));
    expect(state().denoiseModalState.isOpen).toBe(false);
    expect(state().pendingDenoiseJob).toBe('single');

    await act(async () => {
      state().openDenoiseModal(['/photos/b.raw'], true);
      await actions.handleApplyDenoise(0.5, 'ai');
      await actions.handleBatchDenoise(0.5, 'ai', ['/photos/b.raw', '/photos/c.raw']);
      await emit('denoise-progress', 'Late progress');
      await emit('denoise-complete', { denoised: 'late result', original: 'late original' });
      await emit('denoise-error', 'Late error');
    });
    expect(state().denoiseModalState).toMatchObject({
      isOpen: false,
      targetPaths: ['/photos/a.raw'],
      previewBase64: null,
      originalBase64: null,
      error: null,
      progressMessage: null,
    });
    expect(invoke.mock.calls.filter(([cmd]) => cmd === Invokes.ApplyDenoising)).toHaveLength(1);
    expect(invoke.mock.calls.filter(([cmd]) => cmd === 'batch_denoise_images')).toHaveLength(0);
    await act(async () => job.resolve());
    expect(state().pendingDenoiseJob).toBeNull();
    await act(async () => state().openDenoiseModal(['/photos/b.raw'], true));
    expect(state().denoiseModalState).toMatchObject({
      isOpen: true,
      targetPaths: ['/photos/b.raw'],
      previewBase64: null,
      error: null,
    });
  });

  it('allows a fresh job after cancellation and accepts its own preview', async () => {
    const first = deferred<void>();
    const second = deferred<void>();
    let count = 0;
    mockCommand(Invokes.ApplyDenoising, () => (++count === 1 ? first.promise : second.promise));
    await mount();
    await click(button('btnStart'));
    await click(button('cancel'));
    await act(async () => first.resolve());
    await act(async () => state().openDenoiseModal(['/photos/b.raw'], true));
    await click(button('btnStart'));
    await act(async () => {
      await emit('denoise-complete', { denoised: 'new preview', original: 'new original' });
      second.resolve();
    });
    expect(state().denoiseModalState).toMatchObject({
      isOpen: true,
      isProcessing: false,
      previewBase64: 'new preview',
      originalBase64: 'new original',
    });
    expect(button('btnSave').disabled).toBe(false);
  });

  it('lets batch Cancel stop waiting and prevents its late continuation from closing a reopened dialog', async () => {
    const job = deferred<string[]>();
    mockCommand('batch_denoise_images', () => job.promise);
    const { refreshImageList } = await mount(['/photos/a.raw', '/photos/b.raw']);
    await click(button('btnBatchDenoise'));
    expect(state().pendingDenoiseJob).toBe('batch');
    await act(async () => emit('denoise-complete', { denoised: 'intermediate batch image' }));
    expect(state().denoiseModalState.isProcessing).toBe(true);
    expect(state().denoiseModalState.previewBase64).toBeNull();
    await click(backdrop());
    expect(state().denoiseModalState.isOpen).toBe(true);
    expect(button('cancel').disabled).toBe(false);
    await click(button('cancel'));
    await act(async () => state().openDenoiseModal(['/photos/c.raw'], true));
    expect(state().denoiseModalState.isOpen).toBe(false);

    const unsubscribe = useUIStore.subscribe((next, previous) => {
      if (previous.pendingDenoiseJob === 'batch' && next.pendingDenoiseJob === null) {
        next.openDenoiseModal(['/photos/c.raw'], true);
      }
    });
    await act(async () => job.resolve(['/photos/a_Denoised.tiff']));
    unsubscribe();
    expect(refreshImageList).toHaveBeenCalledTimes(1);
    expect(state().denoiseModalState).toMatchObject({
      isOpen: true,
      targetPaths: ['/photos/c.raw'],
      previewBase64: null,
      error: null,
    });
    expect(state().pendingDenoiseJob).toBeNull();
  });

  it('ignores a rejected command after Cancel and releases the pending-job guard', async () => {
    const job = deferred<void>();
    mockCommand(Invokes.ApplyDenoising, () => job.promise);
    await mount();
    await click(button('btnStart'));
    await click(button('cancel'));
    await act(async () => job.reject(new Error('Model unavailable')));
    expect(state().denoiseModalState).toMatchObject({ isOpen: false, error: null });
    expect(state().pendingDenoiseJob).toBeNull();
  });

  it('still displays an active command failure', async () => {
    mockCommand(Invokes.ApplyDenoising, () => {
      throw new Error('Model unavailable');
    });
    await mount();
    await click(button('btnStart'));
    expect(state().denoiseModalState).toMatchObject({
      isOpen: true,
      isProcessing: false,
      error: 'Error: Model unavailable',
    });
    expect(state().pendingDenoiseJob).toBeNull();
  });

  it('accepts a terminal event delivered after the active command promise resolves', async () => {
    mockCommand(Invokes.ApplyDenoising, () => {});
    await mount();
    await click(button('btnStart'));
    expect(state().pendingDenoiseJob).toBeNull();
    expect(state().denoiseModalState.isProcessing).toBe(true);
    await act(async () => state().openDenoiseModal(['/photos/b.raw'], true));
    expect(state().denoiseModalState.targetPaths).toEqual(['/photos/a.raw']);
    await act(async () => emit('denoise-complete', { denoised: 'preview', original: 'original' }));
    expect(state().denoiseModalState).toMatchObject({
      isProcessing: false,
      previewBase64: 'preview',
      originalBase64: 'original',
    });
  });

  it('continues blocking dismissal while saving a completed preview', async () => {
    mockCommand(Invokes.ApplyDenoising, () => {});
    const save = deferred<string>();
    mockCommand(Invokes.SaveDenoisedImage, () => save.promise);
    await mount();
    await click(button('btnStart'));
    await act(async () => emit('denoise-complete', { denoised: 'preview', original: 'original' }));
    await click(button('btnSave'));
    expect(button('close').disabled).toBe(true);
    await click(backdrop());
    expect(state().denoiseModalState.isOpen).toBe(true);
    await act(async () => save.resolve('/photos/a_Denoised.tiff'));
    expect(button('openInEditor').disabled).toBe(false);
  });
});
