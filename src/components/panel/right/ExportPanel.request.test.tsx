// @vitest-environment jsdom
import { act, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import ExportPanel from './ExportPanel';
import { useUIStore } from '../../../store/useUIStore';
import { Panel, Invokes, type AppSettings, type SelectedImage } from '../../ui/AppProperties';
import { Status, type ExportPreset } from '../../ui/ExportImportProperties';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), error: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('react-toastify', () => ({ toast: { error: mocks.error } }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../ui/ExportPresetsList', () => ({
  default: ({ onApplyPreset }: { onApplyPreset: (preset: ExportPreset) => void }) => (
    <button onClick={() => onApplyPreset(ordinary)}>{ordinary.name}</button>
  ),
}));
vi.mock('../../ui/InstagramFraming', () => ({
  default: ({ ratio }: { ratio: string }) => <div data-ratio={ratio} />,
}));
vi.mock('../../ui/Slider', () => ({ default: () => null }));
vi.mock('../../ui/Dropdown', () => ({ default: () => null }));
vi.mock('../../ui/ImagePicker', () => ({ default: () => null }));
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const recipe: ExportPreset = {
  id: 'recipe-instagram-crop-4x5',
  name: 'IG4:5',
  instagramRatio: '4:5',
  fileFormat: 'jpeg',
  jpegQuality: 90,
  enableResize: true,
  resizeMode: 'width',
  resizeValue: 1080,
  dontEnlarge: false,
  keepMetadata: true,
  preserveTimestamps: false,
  stripGps: true,
  filenameTemplate: '{original_filename}',
  enableWatermark: false,
  watermarkPath: null,
  watermarkAnchor: 'bottomRight',
  watermarkScale: 20,
  watermarkSpacing: 2,
  watermarkOpacity: 100,
};
const ordinary: ExportPreset = { ...recipe, id: 'ordinary', name: 'Ordinary', instagramRatio: null };
const photo = { path: '/photo.jpg', isReady: true, width: 1200, height: 800 } as SelectedImage;
let root: Root | undefined;
let host: HTMLDivElement;
let latest: AppSettings;

function Harness() {
  const [settings, setSettings] = useState({
    exportPresets: [{ ...recipe, id: '__last_used__' }],
    lastSelectedExportPresetId: recipe.id,
    lastInstagramExportPresetId: recipe.id,
  } as AppSettings);
  latest = settings;
  return (
    <ExportPanel
      selectedImage={photo}
      multiSelectedPaths={[]}
      rootPaths={[]}
      exportState={{ status: Status.Idle, errorMessage: '', progress: { current: 0, total: 0 } }}
      setExportState={() => {}}
      appSettings={settings}
      onSettingsChange={setSettings}
      isVisible
    />
  );
}

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  host?.remove();
  useUIStore.setState({ instagramExportRequest: null });
  vi.clearAllMocks();
});

it.each(['disable', 'ordinary'] as const)('%s supersedes late direct-entry success and error', async (choice) => {
  for (const outcome of ['success', 'error']) {
    let resolve!: (value: ExportPreset[]) => void;
    let reject!: (reason: Error) => void;
    const pending = new Promise<ExportPreset[]>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    mocks.invoke.mockImplementation((name: string) =>
      name === Invokes.GetExportRecipes ? pending : Promise.resolve(0),
    );
    useUIStore.setState({
      activePanels: { ...useUIStore.getState().activePanels, rightTop: Panel.Export },
      instagramExportRequest: null,
    });
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root!.render(<Harness />));
    expect(host.querySelector('[data-ratio="4:5"]')).toBeTruthy();
    await act(async () => useUIStore.getState().setUI({ instagramExportRequest: 100 }));
    expect(mocks.invoke).toHaveBeenCalledWith(Invokes.GetExportRecipes);
    const text = choice === 'disable' ? 'export.instagram.disable' : ordinary.name;
    const button = [...host.querySelectorAll('button')].find((element) => element.textContent === text)!;
    await act(async () => button.dispatchEvent(new MouseEvent('click', { bubbles: true })));
    expect(useUIStore.getState().instagramExportRequest).toBeNull();
    expect(host.querySelector('[data-ratio]')).toBeNull();
    await act(async () => {
      if (outcome === 'success') resolve([recipe]);
      else reject(new Error('stale lookup'));
    });
    expect(host.querySelector('[data-ratio]')).toBeNull();
    expect(latest.exportPresets?.find((preset) => preset.id === '__last_used__')?.instagramRatio).toBeNull();
    expect(latest.lastSelectedExportPresetId).toBe(choice === 'disable' ? '' : ordinary.id);
    expect(latest.lastInstagramExportPresetId).toBe(recipe.id);
    expect(mocks.error).not.toHaveBeenCalled();
    await act(async () => root?.unmount());
    root = undefined;
    host.remove();
  }
});
