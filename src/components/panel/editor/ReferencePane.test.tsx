// @vitest-environment jsdom
import { act, createElement, createRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../../../test/tauriMock';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import { useImageProcessing } from '../../../hooks/useImageProcessing';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
import { Invokes, type SelectedImage } from '../../ui/AppProperties';
import ReferencePane from './ReferencePane';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialEditor = useEditorStore.getState();
const initialUI = useUIStore.getState();
let root: Root;
let container: HTMLDivElement;
let blobCount = 0;
const image: SelectedImage = {
  path: '/photos/active.raw',
  loadGeneration: 1,
  isReady: true,
  width: 12,
  height: 8,
  isRaw: true,
  exif: {},
  thumbnailUrl: '',
};
function Harness() {
  useImageProcessing(createRef(), createRef(), {
    previewJobIdRef: { current: 0 },
    latestRenderedJobIdRef: { current: 0 },
    currentResRef: { current: 100 },
  });
  return createElement(ReferencePane);
}
const flush = () => act(async () => new Promise((resolve) => setTimeout(resolve, 5)));
beforeEach(() => {
  container = document.createElement('div');
  root = createRoot(container);
  blobCount = 0;
  URL.createObjectURL = vi.fn(() => `blob:ref-${++blobCount}`);
  URL.revokeObjectURL = vi.fn();
  useUIStore.setState({ activeView: 'editor' });
  useEditorStore.setState({ selectedImage: image, adjustments: INITIAL_ADJUSTMENTS });
  mockCommand(Invokes.ApplyAdjustments, () => new ArrayBuffer(0));
  mockCommand(Invokes.ReleaseOriginalPreview, () => undefined);
});
afterEach(async () => {
  await act(async () => root.unmount());
  useEditorStore.setState(initialEditor, true);
  useUIStore.setState(initialUI, true);
});
async function pin(path: string) {
  await act(async () =>
    useEditorStore
      .getState()
      .dispatchReferenceView({ type: 'set-reference', image: { path, label: path.split('/').pop()! } }),
  );
}
describe('pinned sources on upstream comparison', () => {
  it('uses only read-only reference commands and preserves the active recipe/history', async () => {
    const saved = { exposure: 1.5 };
    mockCommand(Invokes.ReadCompareAdjustments, () => saved);
    mockCommand(Invokes.GeneratePreviewForPath, () => new ArrayBuffer(3));
    const before = useEditorStore.getState();
    await pin('/photos/ref.raw?vc=base');
    await act(async () => root.render(createElement(Harness)));
    await flush();
    expect(
      invoke.mock.calls.filter(([cmd]) =>
        [Invokes.ReadCompareAdjustments, Invokes.GeneratePreviewForPath].includes(cmd as Invokes),
      ),
    ).toEqual([
      [Invokes.ReadCompareAdjustments, { path: '/photos/ref.raw?vc=base' }],
      [Invokes.GeneratePreviewForPath, { path: '/photos/ref.raw?vc=base', jsAdjustments: saved, readOnly: true }],
    ]);
    expect(useEditorStore.getState()).toMatchObject({
      compareView: 'sideBySide',
      originalPreview: {
        path: image.path,
        sourcePath: '/photos/ref.raw?vc=base',
        url: 'blob:ref-1',
      },
    });
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(
      invoke.mock.calls.some(([cmd]) =>
        [Invokes.LoadImage, Invokes.LoadMetadata, Invokes.SaveMetadataAndUpdateThumbnail].includes(cmd as Invokes),
      ),
    ).toBe(false);
  });
  it('drops a stale reference response after pinning a different source', async () => {
    let resolve!: (value: ArrayBuffer) => void;
    const slow = new Promise<ArrayBuffer>((done) => {
      resolve = done;
    });
    mockCommand(Invokes.ReadCompareAdjustments, () => INITIAL_ADJUSTMENTS);
    mockCommand(Invokes.GeneratePreviewForPath, (args) =>
      args?.path === '/photos/first.raw' ? slow : new ArrayBuffer(3),
    );
    await pin('/photos/first.raw');
    await act(async () => root.render(createElement(Harness)));
    await flush();
    await pin('/photos/second.raw');
    await flush();
    await act(async () => {
      resolve(new ArrayBuffer(4));
      await slow;
    });
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    expect(useEditorStore.getState().originalPreview?.sourcePath).toBe('/photos/second.raw');
  });
  it('shows an unavailable source without healing metadata or starting a render', async () => {
    mockCommand(Invokes.ReadCompareAdjustments, () => {
      throw new Error('Malformed reference');
    });
    await pin('/photos/deleted.raw');
    await act(async () => root.render(createElement(Harness)));
    await flush();
    expect(container.textContent).toContain('editor.referenceView.unavailable');
    expect(invoke.mock.calls.some(([cmd]) => cmd === Invokes.GeneratePreviewForPath)).toBe(false);
    expect(useEditorStore.getState().originalPreview).toMatchObject({ sourcePath: '/photos/deleted.raw', error: true });
  });
});
