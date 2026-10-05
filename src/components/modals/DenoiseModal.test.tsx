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

let nextJobId = 0;

const calls = (cmd: string) => invoke.mock.calls.filter(([name]) => name === cmd).map(([, args]) => args);

async function mount(paths = ['/photos/a.raw']) {
  nextJobId = 0;
  mockCommand(Invokes.IsRaw9Available, () => false);
  mockCommand(Invokes.CreateDenoiseJob, () => ++nextJobId);
  mockCommand(Invokes.CancelDenoise, () => {});
  const refreshImageList = vi.fn(async () => {});
  const listenerProps = {
    refreshAllFolderTrees: vi.fn(),
    handleSelectSubfolder: vi.fn(),
    refreshImageList,
    markGenerated: vi.fn(),
  };
  let actions!: ReturnType<typeof useProductivityActions>;
  function Harness() {
    const { denoiseModalState: state, closeDenoiseModal } = useUIStore();
    actions = useProductivityActions(refreshImageList);
    useTauriListeners(listenerProps);
    return (
      <DenoiseModal
        {...state}
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

  it("cancels the backend job by ID, allows a new job at once and ignores the old job's events", async () => {
    const first = deferred<void>();
    const second = deferred<void>();
    mockCommand(Invokes.ApplyDenoising, (args) => (args?.jobId === 1 ? first.promise : second.promise));
    await mount();
    await click(button('btnStart'));
    expect(calls(Invokes.ApplyDenoising)).toEqual([
      { jobId: 1, path: '/photos/a.raw', intensity: 1, method: 'tree_best', sharpen: true },
    ]);
    await click(backdrop());
    expect(state().denoiseModalState.isOpen).toBe(true);
    expect(button('cancel').disabled).toBe(false);
    expect(button('cancel').closest('.pointer-events-none')).toBeNull();
    await click(button('cancel'));
    expect(state().denoiseModalState).toMatchObject({ isOpen: false, isProcessing: false, jobId: null });
    expect(calls(Invokes.CancelDenoise)).toEqual([{ jobId: 1 }]);

    await act(async () => state().openDenoiseModal(['/photos/b.raw'], true));
    await click(button('btnStart'));
    expect(calls(Invokes.ApplyDenoising)[1]).toMatchObject({ jobId: 2, path: '/photos/b.raw' });

    await act(async () => {
      await emit('denoise-progress', { jobId: 1, message: 'Late progress' });
      await emit('denoise-complete', { jobId: 1, denoised: 'late result', original: 'late original' });
      await emit('denoise-error', { jobId: 1, message: 'Late error' });
      first.resolve();
    });
    expect(state().denoiseModalState).toMatchObject({
      isOpen: true,
      isProcessing: true,
      jobId: 2,
      targetPaths: ['/photos/b.raw'],
      previewBase64: null,
      error: null,
      progressMessage: 'Starting engine...',
    });

    await act(async () => {
      await emit('denoise-progress', { jobId: 2, message: 'Step 1/2 - 50%' });
    });
    expect(state().denoiseModalState.progressMessage).toBe('Step 1/2 - 50%');
    await act(async () => {
      await emit('denoise-complete', { jobId: 2, denoised: 'new preview', original: 'new original' });
      second.resolve();
    });
    expect(state().denoiseModalState).toMatchObject({
      isProcessing: false,
      previewBase64: 'new preview',
      originalBase64: 'new original',
    });
    expect(calls(Invokes.CancelDenoise)).toHaveLength(1);
  });

  it('cancels a job that is allocated after the dialog was cancelled', async () => {
    const allocation = deferred<number>();
    mockCommand(Invokes.ApplyDenoising, () => {});
    await mount();
    mockCommand(Invokes.CreateDenoiseJob, () => allocation.promise);
    await click(button('btnStart'));
    await click(button('cancel'));
    expect(calls(Invokes.CancelDenoise)).toHaveLength(0);
    await act(async () => allocation.resolve(7));
    expect(calls(Invokes.CancelDenoise)).toEqual([{ jobId: 7 }]);
    expect(calls(Invokes.ApplyDenoising)).toHaveLength(0);
    expect(state().denoiseModalState).toMatchObject({ isOpen: false, jobId: null, error: null });
  });

  it('cancels a batch by ID, ignores its events and keeps its late continuation away from a reopened dialog', async () => {
    const job = deferred<string[]>();
    mockCommand(Invokes.BatchDenoiseImages, () => job.promise);
    const { refreshImageList } = await mount(['/photos/a.raw', '/photos/b.raw']);
    await click(button('btnBatchDenoise'));
    expect(calls(Invokes.BatchDenoiseImages)).toEqual([
      { jobId: 1, paths: ['/photos/a.raw', '/photos/b.raw'], intensity: 1, method: 'tree_best', sharpen: true },
    ]);
    await act(async () => emit('denoise-complete', { jobId: 1, denoised: 'intermediate batch image' }));
    await act(async () => emit('denoise-error', { jobId: 1, message: 'Failed to denoise a.raw' }));
    expect(state().denoiseModalState).toMatchObject({
      isProcessing: true,
      previewBase64: null,
      error: 'Failed to denoise a.raw',
    });
    await click(backdrop());
    expect(state().denoiseModalState.isOpen).toBe(true);
    await click(button('close'));
    expect(calls(Invokes.CancelDenoise)).toEqual([{ jobId: 1 }]);

    await act(async () => state().openDenoiseModal(['/photos/c.raw'], true));
    expect(state().denoiseModalState).toMatchObject({ isOpen: true, targetPaths: ['/photos/c.raw'] });
    await act(async () => {
      await emit('denoise-batch-progress', { jobId: 1, current: 2, total: 2, path: '/photos/b.raw' });
      job.resolve(['/photos/a_Denoised.tiff']);
    });
    expect(refreshImageList).toHaveBeenCalledTimes(1);
    expect(state().denoiseModalState).toMatchObject({
      isOpen: true,
      isProcessing: false,
      targetPaths: ['/photos/c.raw'],
      previewBase64: null,
      error: null,
    });
  });

  it('ignores a rejected command after Cancel', async () => {
    const job = deferred<void>();
    mockCommand(Invokes.ApplyDenoising, () => job.promise);
    await mount();
    await click(button('btnStart'));
    await click(button('cancel'));
    await act(async () => job.reject(new Error('Model unavailable')));
    expect(state().denoiseModalState).toMatchObject({ isOpen: false, error: null });
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
  });

  it('accepts its own terminal event delivered after the command promise resolves', async () => {
    mockCommand(Invokes.ApplyDenoising, () => {});
    await mount();
    await click(button('btnStart'));
    expect(state().denoiseModalState).toMatchObject({ isProcessing: true, jobId: 1 });
    await act(async () => state().openDenoiseModal(['/photos/b.raw'], true));
    expect(state().denoiseModalState.targetPaths).toEqual(['/photos/a.raw']);
    await act(async () => emit('denoise-complete', { jobId: 99, denoised: 'other', original: 'other' }));
    expect(state().denoiseModalState.previewBase64).toBeNull();
    await act(async () => emit('denoise-complete', { jobId: 1, denoised: 'preview', original: 'original' }));
    expect(state().denoiseModalState).toMatchObject({
      isProcessing: false,
      previewBase64: 'preview',
      originalBase64: 'original',
    });
  });

  it('saves the shown preview by its job ID and blocks dismissal while saving', async () => {
    mockCommand(Invokes.ApplyDenoising, () => {});
    const save = deferred<string>();
    mockCommand(Invokes.SaveDenoisedImage, () => save.promise);
    await mount();
    await click(button('btnStart'));
    await act(async () => emit('denoise-complete', { jobId: 1, denoised: 'preview', original: 'original' }));
    await click(button('btnSave'));
    expect(calls(Invokes.SaveDenoisedImage)).toEqual([{ jobId: 1, originalPathStr: '/photos/a.raw' }]);
    expect(button('close').disabled).toBe(true);
    await click(backdrop());
    expect(state().denoiseModalState.isOpen).toBe(true);
    await act(async () => save.resolve('/photos/a_Denoised.tiff'));
    expect(button('openInEditor').disabled).toBe(false);
    expect(calls(Invokes.CancelDenoise)).toHaveLength(0);
  });
});

describe('AI raw denoise presets', () => {
  it('defaults to Best with separate sharpening and hides BM3D until More methods', async () => {
    await mount();
    expect(container.querySelector<HTMLInputElement>('input[type="checkbox"]')?.checked).toBe(true);
    const dropdown = container.querySelector('button[aria-haspopup="listbox"]')!;
    await click(dropdown);
    expect(container.textContent).toContain('modals.denoise.presetFast');
    expect(container.textContent).not.toContain('modals.denoise.methodBm3d');
    await click(dropdown);
    await click(button('moreMethods'));
    await click(dropdown);
    expect(container.textContent).toContain('modals.denoise.methodBm3d');
  });

  it('runs Fast without sharpening when the user switches it off', async () => {
    mockCommand(Invokes.ApplyDenoising, () => {});
    await mount();
    await click(container.querySelector('button[aria-haspopup="listbox"]')!);
    const option = [...container.querySelectorAll('[role="option"]')].find(
      (el) => el.textContent === 'modals.denoise.presetFast',
    )!;
    await click(option);
    await click(container.querySelector('input[type="checkbox"]')!);
    await click(button('btnStart'));
    expect(calls(Invokes.ApplyDenoising)).toEqual([
      { jobId: 1, path: '/photos/a.raw', intensity: 1, method: 'tree_fast', sharpen: false },
    ]);
  });
});
