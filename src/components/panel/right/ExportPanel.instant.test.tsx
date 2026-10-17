// @vitest-environment jsdom
import { act, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import ExportPanel from './ExportPanel';
import { useKeyboardShortcuts } from '../../../hooks/useKeyboardShortcuts';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import { useLibraryStore } from '../../../store/useLibraryStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { Invokes, Panel, type AppSettings, type ImageFile, type SelectedImage } from '../../ui/AppProperties';
import { Status, type ExportPreset } from '../../ui/ExportImportProperties';
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), save: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: mocks.save, open: vi.fn(), confirm: vi.fn() }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
vi.mock('../../ui/ExportPresetsList', () => ({ default: () => null }));
vi.mock('../../ui/Slider', () => ({ default: () => null }));
vi.mock('../../ui/Dropdown', () => ({ default: () => null }));
vi.mock('../../ui/ImagePicker', () => ({ default: () => null }));
// InstagramFraming and the shortcut/export path are real; preview IPC deliberately never resolves.
const recipe = {
  id: 'recipe-instagram-crop-3x4',
  name: '3:4',
  instagramRatio: '3:4',
  fileFormat: 'jpeg',
  jpegQuality: 90,
  enableResize: true,
  resizeMode: 'width',
  resizeValue: 1080,
  dontEnlarge: false,
  keepMetadata: true,
  stripGps: true,
  filenameTemplate: '{original_filename}_ig34',
  enableWatermark: false,
  watermarkPath: null,
  watermarkAnchor: 'bottomRight',
  watermarkScale: 10,
  watermarkSpacing: 5,
  watermarkOpacity: 75,
} as ExportPreset;
const photo = { path: '/cropped.ARW', width: 1800, height: 2400, isReady: true } as SelectedImage;
const callbacks = {
  sortedImageList: [] as ImageFile[],
  handleBackToLibrary: vi.fn(),
  handleDeleteSelected: vi.fn(),
  handleDeleteRejected: vi.fn(),
  handleGoHome: vi.fn(),
  handleImageSelect: vi.fn(),
  handlePasteFiles: vi.fn(),
  handleZoomChange: vi.fn(),
};
const editor = useEditorStore.getState(),
  ui = useUIStore.getState(),
  library = useLibraryStore.getState(),
  settings = useSettingsStore.getState();
let root: Root, host: HTMLDivElement;
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
function Harness() {
  useKeyboardShortcuts(callbacks);
  const [preferences, setPreferences] = useState({ exportPresets: [] } as unknown as AppSettings);
  return (
    <ExportPanel
      selectedImage={photo}
      multiSelectedPaths={[]}
      rootPaths={[]}
      appSettings={preferences}
      onSettingsChange={setPreferences}
      exportState={{ status: Status.Idle, errorMessage: '', progress: { current: 0, total: 0 } }}
      setExportState={() => {}}
      isVisible
    />
  );
}
beforeEach(() => {
  vi.useFakeTimers();
  useUIStore.setState({
    ...ui,
    activeView: 'editor',
    isSettingsOpen: false,
    instagramExportRequest: null,
    activePanels: { ...ui.activePanels, rightTop: Panel.Adjustments },
  });
  useEditorStore.setState({ ...editor, selectedImage: photo });
  useLibraryStore.setState(library, true);
  useSettingsStore.setState({ ...settings, osPlatform: 'linux' });
  mocks.invoke.mockImplementation((name: string) =>
    name === Invokes.GetExportRecipes
      ? Promise.resolve([recipe])
      : name === 'instagram_export_folder'
        ? Promise.resolve('/Pictures/Instagram/day')
        : name === 'get_instagram_framing_preview'
          ? new Promise(() => {})
          : Promise.resolve(0),
  );
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.useRealTimers();
  vi.clearAllMocks();
  useEditorStore.setState(editor, true);
  useUIStore.setState(ui, true);
  useLibraryStore.setState(library, true);
  useSettingsStore.setState(settings, true);
});
it('Ctrl+Alt+I exports a current 3:4 crop with zero preview or estimation requests; Adjust frame is explicit and never gates export', async () => {
  await act(async () => root.render(<Harness />));
  const adjustments = {
    ...useEditorStore.getState().adjustments,
    orientationSteps: 1,
    crop: { unit: 'px' as const, x: 100, y: 200, width: 1800, height: 2400 },
  };
  await act(async () => useEditorStore.setState({ adjustments }));
  await act(async () =>
    document.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'i',
        code: 'KeyI',
        ctrlKey: true,
        altKey: true,
        bubbles: true,
        cancelable: true,
      }),
    ),
  );
  await act(async () => vi.advanceTimersByTimeAsync(1000));
  const exportButton = () =>
    [...host.querySelectorAll('button')].find((b) => b.textContent?.includes('export.status.exportSingle'))!;
  expect(exportButton().disabled).toBe(false);
  expect(host.querySelector('[data-instagram-framing]')).toBeNull();
  expect(host.textContent).not.toContain('export.status.estimatingSize');
  await act(async () => exportButton().click());
  expect(mocks.invoke).toHaveBeenCalledWith(
    Invokes.ExportImages,
    expect.objectContaining({
      currentEditPath: photo.path,
      currentEditAdjustments: adjustments,
      exportSettings: expect.objectContaining({ instagram: { ratio: '3:4', framesByPath: {} } }),
    }),
  );
  expect(
    mocks.invoke.mock.calls.filter(
      ([name]) => name === 'get_instagram_framing_preview' || name === Invokes.EstimateExportSizes,
    ),
  ).toEqual([]);
  expect(mocks.save).not.toHaveBeenCalled();
  await act(async () =>
    [...host.querySelectorAll('button')].find((b) => b.textContent === 'export.instagram.adjustFrame')!.click(),
  );
  await act(async () => vi.advanceTimersByTimeAsync(350));
  expect(mocks.invoke).toHaveBeenCalledWith('get_instagram_framing_preview', { path: photo.path, ratio: '3:4' });
  expect(exportButton().disabled).toBe(false);
  await act(async () => exportButton().click());
  expect(mocks.invoke.mock.calls.filter(([name]) => name === Invokes.ExportImages)).toHaveLength(2);
  expect(mocks.invoke.mock.calls.filter(([name]) => name === Invokes.EstimateExportSizes)).toEqual([]);
});
