// @vitest-environment jsdom
import { act, createElement, createRef } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { Invokes, type SelectedImage } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { mockCommand } from '../test/tauriMock';
import { INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { useImageProcessing } from './useImageProcessing';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const editor = useEditorStore.getState();
const ui = useUIStore.getState();
afterEach(() => {
  useEditorStore.setState(editor, true);
  useUIStore.setState(ui, true);
  vi.restoreAllMocks();
});

function image(path: string, loadGeneration: number): SelectedImage {
  return { path, loadGeneration, isReady: false, isRaw: true, exif: {}, width: 12, height: 8, thumbnailUrl: '' };
}

it('rejects a late preview from an earlier visit to the same photo, including its failure cleanup', async () => {
  const pending: Array<{ resolve: (buffer: ArrayBuffer) => void; reject: (error: string) => void }> = [];
  const calls: unknown[] = [];
  mockCommand(Invokes.ApplyAdjustments, (args) => {
    calls.push(args);
    return new Promise<ArrayBuffer>((resolve, reject) => pending.push({ resolve, reject }));
  });
  useEditorStore.setState({ selectedImage: image('/photos/A.ARW', 1) });
  let processing: ReturnType<typeof useImageProcessing>;
  function Harness() {
    processing = useImageProcessing(createRef(), createRef(), {
      previewJobIdRef: { current: 0 },
      latestRenderedJobIdRef: { current: 0 },
      currentResRef: { current: 0 },
    });
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => root.render(createElement(Harness)));
  const error = vi.spyOn(console, 'error');
  const create = vi.fn(() => 'blob:should-not-be-created');
  vi.stubGlobal(
    'URL',
    class extends URL {
      static createObjectURL = create;
      static revokeObjectURL = vi.fn();
    },
  );
  try {
    const oldBytes = processing!.executeApplyAdjustments(INITIAL_ADJUSTMENTS);
    const oldFailure = processing!.executeApplyAdjustments(INITIAL_ADJUSTMENTS);
    expect(calls[0]).toMatchObject({ imagePath: '/photos/A.ARW', loadGeneration: 1 });
    await act(async () => useEditorStore.setState({ selectedImage: image('/photos/B.ARW', 2) }));
    const patch = { url: 'blob:current', normX: 0, normY: 0, normW: 1, normH: 1 };
    await act(async () =>
      useEditorStore.setState({ selectedImage: image('/photos/A.ARW', 3), interactivePatch: patch }),
    );
    await act(async () => {
      pending[0].resolve(new ArrayBuffer(32));
      pending[1].reject('No original image loaded');
      await Promise.all([oldBytes, oldFailure]);
    });
    expect(useEditorStore.getState().interactivePatch).toBe(patch);
    expect(useEditorStore.getState().finalPreviewUrl).toBe(editor.finalPreviewUrl);
    expect(create).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
    const currentFailure = processing!.executeApplyAdjustments(INITIAL_ADJUSTMENTS);
    await act(async () => {
      pending[2].reject('Current GPU failed');
      await currentFailure;
    });
    expect(error).toHaveBeenCalledWith('Failed to apply adjustments:', 'Current GPU failed');
    expect(useEditorStore.getState().interactivePatch).toBeNull();
  } finally {
    await act(async () => root.unmount());
    vi.unstubAllGlobals();
  }
});
