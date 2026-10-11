// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import CropPanel from './CropPanel';
import { useUIStore } from '../../../store/useUIStore';
import { useEditorStore } from '../../../store/useEditorStore';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
import { debouncedSetHistory } from '../../../hooks/useEditorActions';
import { invoke } from '../../../test/tauriMock';
import { Panel, type SelectedImage } from '../../ui/AppProperties';
vi.mock('../../../context/ContextMenuContext', () => ({ useContextMenu: () => ({ showContextMenu: vi.fn() }) }));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
  Trans: ({ children }: { children: React.ReactNode }) => children,
}));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root | undefined;
let node: HTMLDivElement | undefined;
const initial = useEditorStore.getState();
const initialUi = useUIStore.getState();
afterEach(async () => {
  await act(async () => root?.unmount());
  node?.remove();
  debouncedSetHistory.cancel();
  useEditorStore.setState(initial, true);
  useUIStore.setState(initialUi, true);
  document.querySelector('[role="dialog"]')?.remove();
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

it('reviewer: embedded Lens and Transform do not duplicate crop-only keyboard handlers', async () => {
  invoke.mockImplementation(async (command) => (command === 'load_settings' ? {} : []));
  useEditorStore.getState().setEditor({ overlayRotation: 0 });
  useEditorStore.setState({
    selectedImage: { path: '/test.raw', width: 4608, height: 3072, isReady: true, exif: {} } as SelectedImage,
  });
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () =>
    root!.render(
      <>
        <CropPanel section="lens" />
        <CropPanel section="transform" />
      </>,
    ),
  );
  await act(async () =>
    window.dispatchEvent(
      new KeyboardEvent('keydown', { code: 'KeyO', key: 'O', shiftKey: true, bubbles: true, cancelable: true }),
    ),
  );
  expect(useEditorStore.getState().overlayRotation).toBe(0);
});

it('cycles the active crop overlay once and blocks fields, dialogs and other panels', async () => {
  invoke.mockImplementation(async (command) => (command === 'load_settings' ? {} : []));
  useEditorStore.setState({
    selectedImage: { path: '/test.raw', width: 4608, height: 3072, isReady: true, exif: {} } as SelectedImage,
    overlayRotation: 0,
  });
  useUIStore.setState({ activeView: 'editor', activePanel: Panel.Crop, isSettingsOpen: false });
  node = document.createElement('div');
  document.body.append(node);
  root = createRoot(node);
  await act(async () =>
    root!.render(
      <>
        <CropPanel />
        <CropPanel section="lens" />
        <CropPanel section="transform" />
      </>,
    ),
  );
  const press = () =>
    window.dispatchEvent(
      new KeyboardEvent('keydown', { code: 'KeyO', key: 'O', shiftKey: true, bubbles: true, cancelable: true }),
    );
  await act(async () => {
    press();
  });
  expect(useEditorStore.getState().overlayRotation).toBe(1);
  const select = document.createElement('select');
  node.append(select);
  select.focus();
  await act(async () => {
    press();
  });
  expect(useEditorStore.getState().overlayRotation).toBe(1);
  select.remove();
  const dialog = document.createElement('div');
  dialog.setAttribute('role', 'dialog');
  document.body.append(dialog);
  await act(async () => {
    press();
  });
  expect(useEditorStore.getState().overlayRotation).toBe(1);
  dialog.remove();
  await act(async () => useUIStore.setState({ confirmModalState: { ...initialUi.confirmModalState, isOpen: true } }));
  await act(async () => {
    press();
  });
  expect(useEditorStore.getState().overlayRotation).toBe(1);
  await act(async () =>
    useUIStore.setState({ confirmModalState: initialUi.confirmModalState, activePanel: Panel.Adjustments }),
  );
  await act(async () => {
    press();
  });
  expect(useEditorStore.getState().overlayRotation).toBe(1);
});
