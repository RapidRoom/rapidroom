// @vitest-environment jsdom
import { act, useState } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { Theme, type Preset, type SelectedImage } from '../ui/AppProperties';
import { INITIAL_ADJUSTMENTS, type Adjustments } from '../../utils/adjustments';
import { useEditorStore } from '../../store/useEditorStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { saveCatalogPresets, usePresetCatalog } from '../../store/usePresetCatalog';
import { usePresetApplication } from '../../utils/presetApplication';
import { debouncedSave, debouncedSetHistory } from '../../hooks/useEditorActions';
import { mockCommand } from '../../test/tauriMock';
import CurvePresetControl from './CurvePresetControl';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const editorInitial = useEditorStore.getState();
const settingsInitial = useSettingsStore.getState();
const image: SelectedImage = {
  path: '/photo.raw',
  width: 6000,
  height: 4000,
  isRaw: true,
  exif: {},
  isReady: true,
  thumbnailUrl: '',
};
const curve = (middle: number) => ({
  ...INITIAL_ADJUSTMENTS.curves,
  luma: [
    { x: 0, y: 0 },
    { x: 128, y: middle },
    { x: 255, y: 255 },
  ],
});
let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  useEditorStore.setState(editorInitial, true);
  useSettingsStore.setState(settingsInitial, true);
  useSettingsStore.setState({ appSettings: { theme: Theme.Dark, lastRootPath: null } });
  usePresetApplication.setState({ application: null });
  useEditorStore.setState({
    selectedImage: image,
    adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 2, curves: curve(120) },
  });
  usePresetCatalog.setState({ presets: [], isLoading: false, loaded: true });
  mockCommand('save_applied_preset', () => undefined);
  mockCommand('save_presets', () => undefined);
  mockCommand('get_default_raw_lens_adjustments', () => ({}));

  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root.render(<Harness />));
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  debouncedSetHistory.cancel();
  debouncedSave.cancel();
  saveCatalogPresets.cancel();
  useEditorStore.setState(editorInitial, true);
  useSettingsStore.setState(settingsInitial, true);
  usePresetApplication.setState({ application: null });
  vi.restoreAllMocks();
});

async function click(element: Element) {
  await act(async () => {
    element.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
}

async function enterName(input: HTMLInputElement, name: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, name);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

it('applies only saved curves and renames or deletes the selected preset', async () => {
  const savedCurve: Preset = {
    id: 'signature',
    name: 'Signature',
    presetType: 'tool',
    curvePreset: true,
    adjustments: { curves: curve(170), curveMode: 'point' },
  };
  usePresetCatalog.setState({
    presets: [
      { preset: savedCurve },
      { preset: { id: 'exposure', name: 'Exposure look', presetType: 'tool', adjustments: { exposure: 3 } } },
    ],
  });
  const crop = useEditorStore.getState().adjustments.crop;
  await act(async () => root.render(<Harness />));

  await click(container.querySelector('button[aria-haspopup="listbox"]')!);
  const options = [...container.querySelectorAll('[role="option"]')];
  expect(options.map((option) => option.textContent)).toEqual(['Signature']);
  await click(options[0]);
  expect(useEditorStore.getState().adjustments.curves).toEqual(curve(170));
  expect(useEditorStore.getState().adjustments.exposure).toBe(2);
  expect(useEditorStore.getState().adjustments.crop).toBe(crop);

  await click(container.querySelector('button[aria-label="adjustments.curves.presets.rename"]')!);
  const renameDialog = container.querySelector('[role="dialog"]')!;
  await enterName(renameDialog.querySelector('input')!, 'Renamed curve');
  const renameButton = [...renameDialog.querySelectorAll('button')].find(
    (button) => button.textContent === 'adjustments.curves.presets.rename',
  )!;
  await click(renameButton);
  expect(usePresetCatalog.getState().presets[0].preset?.name).toBe('Renamed curve');

  await click(container.querySelector('button[aria-label="adjustments.curves.presets.delete"]')!);
  expect(usePresetCatalog.getState().presets).toHaveLength(1);
  expect(usePresetCatalog.getState().presets[0].preset?.id).toBe('exposure');
});

function Harness() {
  const [adjustments, setState] = useState<Adjustments>(useEditorStore.getState().adjustments);
  return (
    <CurvePresetControl
      adjustments={adjustments}
      setAdjustments={(update) =>
        setState((previous) => {
          const next = update(previous);
          useEditorStore.setState({ adjustments: next });
          return next;
        })
      }
    />
  );
}

it('saves the current curve set through the preset catalog', async () => {
  await click(container.querySelector('button[aria-label="adjustments.curves.presets.save"]')!);
  const dialog = container.querySelector('[role="dialog"]')!;
  await enterName(dialog.querySelector('input')!, 'My curve');
  const saveButton = [...dialog.querySelectorAll('button')].find(
    (button) => button.textContent === 'adjustments.curves.presets.save',
  )!;
  await click(saveButton);
  await vi.waitFor(() => expect(usePresetCatalog.getState().presets).toHaveLength(1));
  const preset = usePresetCatalog.getState().presets[0].preset!;
  expect(preset).toMatchObject({ name: 'My curve', presetType: 'tool', curvePreset: true });
  expect(preset.adjustments).toEqual({ curves: curve(120) });
});
