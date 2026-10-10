// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import LightroomSidebar from './LightroomSidebar';
import { useEditorStore } from '../../store/useEditorStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { INITIAL_ADJUSTMENTS } from '../../utils/adjustments';
import { debouncedSetHistory } from '../../hooks/useEditorActions';
import { invoke, mockCommand } from '../../test/tauriMock';
import type { SelectedImage } from '../ui/AppProperties';
vi.mock('./right/PresetsPanel', () => ({ default: () => null }));
const translations = vi.hoisted(() => ({ t: (key: string) => key }));
vi.mock('react-i18next', () => ({ useTranslation: () => translations }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root;
let node: HTMLDivElement;
const editor = useEditorStore.getState(),
  library = useLibraryStore.getState();
beforeEach(async () => {
  mockCommand('get_photo_snapshots', () => [
    { id: 'warm', name: 'Warm', adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 2 } },
  ]);
  mockCommand('save_photo_snapshots', () => undefined);
  useLibraryStore.setState({ ...library, cardBrowseRoot: null });
  useEditorStore.getState().resetHistory({ ...INITIAL_ADJUSTMENTS });
  useEditorStore.setState({
    selectedImage: { path: '/photo.raw', isReady: true } as SelectedImage,
    previewOverride: null,
    showOriginal: false,
    isSliderDragging: false,
  });
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () =>
    root.render(
      <LightroomSidebar onNavigateToCommunity={() => {}} onZoom={() => {}}>
        <div />
      </LightroomSidebar>,
    ),
  );
});
afterEach(async () => {
  await act(async () => root.unmount());
  node.remove();
  debouncedSetHistory.cancel();
  useEditorStore.setState(editor, true);
  useLibraryStore.setState(library, true);
});
const button = (text: string) => [...node.querySelectorAll('button')].find((element) => element.textContent === text)!;
it('restores a persistent snapshot with one Undo and navigates real history', async () => {
  await act(async () => button('Warm').click());
  expect(useEditorStore.getState().adjustments.exposure).toBe(2);
  expect(useEditorStore.getState().historyIndex).toBe(1);
  await act(async () => useEditorStore.getState().undo());
  expect(useEditorStore.getState().adjustments.exposure).toBe(0);
  await act(async () => useEditorStore.getState().redo());
  await act(async () => button('lightroomWorkspace.opened').click());
  expect(useEditorStore.getState().historyIndex).toBe(0);
  expect(useEditorStore.getState().adjustments.exposure).toBe(0);
});
it('saves the current photo adjustments and blocks saves on a read-only card', async () => {
  const input = node.querySelector('input')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'Neutral');
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await act(async () => button('lightroomWorkspace.save').click());
  expect(invoke).toHaveBeenCalledWith('save_photo_snapshots', {
    path: '/photo.raw',
    snapshots: expect.arrayContaining([expect.objectContaining({ name: 'Neutral', adjustments: INITIAL_ADJUSTMENTS })]),
  });
  await act(async () => useLibraryStore.setState({ cardBrowseRoot: '/' }));
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'Blocked');
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  expect((button('lightroomWorkspace.save') as HTMLButtonElement).disabled).toBe(true);
});
