// @vitest-environment jsdom
import { act, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import ExportPanel from './ExportPanel';
import { useUIStore } from '../../../store/useUIStore';
import { Panel, Invokes, type AppSettings, type SelectedImage } from '../../ui/AppProperties';
import { Status, type ExportPreset } from '../../ui/ExportImportProperties';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  save: vi.fn(),
  open: vi.fn(),
  confirm: vi.fn(),
  state: vi.fn(),
  platform: 'linux',
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: mocks.save, open: mocks.open, confirm: mocks.confirm }));
vi.mock('../../../hooks/useOsPlatform', () => ({ useOsPlatform: () => mocks.platform }));
vi.mock('../../../hooks/useEditorActions', () => ({
  debouncedSave: { flush: vi.fn() },
  waitForPendingEditorSaves: () => Promise.resolve(),
}));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../ui/ExportPresetsList', () => ({ default: () => null }));
vi.mock('../../ui/Slider', () => ({ default: () => null }));
vi.mock('../../ui/Dropdown', () => ({ default: () => null }));
vi.mock('../../ui/ImagePicker', () => ({ default: () => null }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const photo = { path: '/photos/source.jpg', isReady: true, width: 1200, height: 800 } as SelectedImage;
let root: Root | undefined;
let host: HTMLDivElement;

async function exportThroughPanel(paths: string[] = [], preset: Partial<ExportPreset> = {}) {
  function Harness() {
    const [settings, setSettings] = useState({
      exportPresets: [
        {
          id: '__last_used__',
          name: 'Ordinary',
          fileFormat: 'jpeg',
          jpegQuality: 90,
          enableResize: false,
          resizeMode: 'width',
          resizeValue: 1080,
          dontEnlarge: false,
          keepMetadata: true,
          stripGps: true,
          filenameTemplate: '{original_filename}_edited',
          enableWatermark: false,
          watermarkPath: null,
          watermarkAnchor: 'bottomRight',
          watermarkScale: 20,
          watermarkSpacing: 2,
          watermarkOpacity: 100,
          ...preset,
        },
      ],
    } as AppSettings);
    return (
      <ExportPanel
        selectedImage={photo}
        multiSelectedPaths={paths}
        rootPaths={[]}
        exportState={{ status: Status.Idle, errorMessage: '', progress: { current: 0, total: 0 } }}
        setExportState={mocks.state}
        appSettings={settings}
        onSettingsChange={setSettings}
        isVisible
      />
    );
  }
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root!.render(<Harness />));
  const button = [...host.querySelectorAll('button')].find(
    (e) =>
      e.textContent?.includes('export.status.exportSingle') || e.textContent?.includes('export.status.exportMultiple'),
  );
  expect(button).toBeTruthy();
  expect((button as HTMLButtonElement).disabled).toBe(false);
  await act(async () => button!.dispatchEvent(new MouseEvent('click', { bubbles: true })));
}

function exportRequest() {
  return mocks.invoke.mock.calls.find(([command]) => command === Invokes.ExportImages)?.[1];
}

beforeEach(() => {
  mocks.platform = 'linux';
  mocks.save.mockResolvedValue('/exports/existing.jpg');
  mocks.open.mockResolvedValue('/exports');
  mocks.confirm.mockResolvedValue(true);
  mocks.invoke.mockImplementation((command: string) =>
    Promise.resolve(command === Invokes.GenerateExportFilename ? 'source_edited' : 0),
  );
  useUIStore.setState({
    activePanels: { ...useUIStore.getState().activePanels, rightTop: Panel.Export },
    instagramExportRequest: null,
  });
});
afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  host?.remove();
  vi.clearAllMocks();
});

it('authorizes atomic replacement only for the accepted desktop Save-As path without another confirmation', async () => {
  await exportThroughPanel();
  expect(mocks.save).toHaveBeenCalledOnce();
  expect(mocks.open).not.toHaveBeenCalled();
  expect(mocks.confirm).not.toHaveBeenCalled();
  expect(exportRequest()).toMatchObject({
    outputFolderOrFile: '/exports/existing.jpg',
    isExplicitFilePath: true,
    exportSettings: { overwriteExisting: true },
  });
});

it('does not export when Save-As is cancelled', async () => {
  mocks.save.mockResolvedValue(null);
  await exportThroughPanel();
  expect(exportRequest()).toBeUndefined();
  expect(mocks.confirm).not.toHaveBeenCalled();
});

it.each(['batch', 'originalFolder', 'preserveFolders'] as const)(
  'retains no-clobber automatic naming for %s without the overwrite toggle',
  async (mode) => {
    await exportThroughPanel(
      mode === 'batch' ? [photo.path, '/photos/second.jpg'] : [],
      mode === 'originalFolder'
        ? { destinationType: 'originalFolder' }
        : { preserveFolders: mode === 'preserveFolders' },
    );
    expect(mocks.save).not.toHaveBeenCalled();
    expect(mocks.confirm).not.toHaveBeenCalled();
    expect(exportRequest()).toMatchObject({ isExplicitFilePath: false, exportSettings: { overwriteExisting: false } });
  },
);

it('does not infer replacement consent from the Android generated filename', async () => {
  mocks.platform = 'android';
  await exportThroughPanel();
  expect(mocks.save).not.toHaveBeenCalled();
  expect(exportRequest()).toMatchObject({ exportSettings: { overwriteExisting: false } });
});

it('preserves a destination-bearing backend failure in the visible export state', async () => {
  const error = "Failed to write file to '/exports/existing.jpg': Permission denied";
  mocks.invoke.mockImplementation((command: string) =>
    command === Invokes.ExportImages
      ? Promise.reject(error)
      : Promise.resolve(command === Invokes.GenerateExportFilename ? 'source_edited' : 0),
  );
  await exportThroughPanel();
  expect(mocks.state).toHaveBeenLastCalledWith(expect.objectContaining({ status: Status.Error, errorMessage: error }));
});
