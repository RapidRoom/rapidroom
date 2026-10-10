// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import CropPanel from './CropPanel';
import { useEditorStore } from '../../../store/useEditorStore';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
import { debouncedSetHistory } from '../../../hooks/useEditorActions';
import { invoke } from '../../../test/tauriMock';
import type { SelectedImage } from '../../ui/AppProperties';
vi.mock('../../../context/ContextMenuContext', () => ({ useContextMenu: () => ({ showContextMenu: vi.fn() }) }));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
  Trans: ({ children }: { children: React.ReactNode }) => children,
}));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root | undefined;
let node: HTMLDivElement | undefined;
const initial = useEditorStore.getState();
afterEach(async () => {
  await act(async () => root?.unmount());
  node?.remove();
  debouncedSetHistory.cancel();
  useEditorStore.setState(initial, true);
});
it('mounting and changing embedded Lens/Transform panels preserves an existing rotated crop and history', async () => {
  invoke.mockImplementation(async (command) => (command === 'load_settings' ? {} : []));
  const adjustments = {
    ...INITIAL_ADJUSTMENTS,
    orientationSteps: 1,
    aspectRatio: 1.5,
    crop: { unit: 'px' as const, x: 12, y: 24, width: 900, height: 600 },
    rotation: 7,
  };
  useEditorStore.getState().resetHistory(adjustments);
  useEditorStore.setState({
    selectedImage: { path: '/test.raw', width: 4608, height: 3072, isReady: true, exif: {} } as SelectedImage,
  });
  const history = useEditorStore.getState().history;
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () => root!.render(<CropPanel section="lens" />));
  expect(node.textContent).toContain('modals.lensCorrection.title');
  expect(node.textContent).not.toContain('editor.crop.aspectRatioHeading');
  expect(node.textContent).not.toContain('modals.transform.geometry');
  expect(useEditorStore.getState().adjustments).toBe(adjustments);
  await act(async () => root!.render(<CropPanel section="transform" />));
  expect(node.textContent).toContain('modals.transform.title');
  expect(node.textContent).not.toContain('modals.lensCorrection.title');
  expect(useEditorStore.getState().adjustments).toBe(adjustments);
  expect(useEditorStore.getState().history).toBe(history);
});
