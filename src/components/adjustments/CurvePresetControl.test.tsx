// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { Theme, type Preset, type SelectedImage } from '../ui/AppProperties';
import { INITIAL_ADJUSTMENTS } from '../../utils/adjustments';
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

async function choosePreset(name: string) {
  await click(container.querySelector('button[aria-haspopup="listbox"]')!);
  const option = [...container.querySelectorAll('[role="option"]')].find((item) => item.textContent === name)!;
  await click(option);
}

it.each([false, true])(
  'applies only saved curves and renames or deletes the selected preset (nested=%s)',
  async (nested) => {
    const savedCurve: Preset = {
      id: 'signature',
      name: 'Signature',
      presetType: 'tool',
      curvePreset: true,
      adjustments: { curves: curve(170), curveMode: 'point' },
    };
    const curveItem = nested
      ? { folder: { id: 'curves', name: 'Curves', children: [savedCurve] } }
      : { preset: savedCurve };
    usePresetCatalog.setState({
      presets: [
        curveItem,
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
    const renamed = usePresetCatalog.getState().presets[0];
    expect(nested ? renamed.folder?.children[0].name : renamed.preset?.name).toBe('Renamed curve');

    await click(container.querySelector('button[aria-label="adjustments.curves.presets.delete"]')!);
    const remaining = usePresetCatalog.getState().presets;
    expect(remaining).toHaveLength(nested ? 2 : 1);
    if (nested) expect(remaining[0].folder?.children).toHaveLength(0);
    expect(remaining.at(-1)?.preset?.id).toBe('exposure');
  },
);

function Harness() {
  const adjustments = useEditorStore((state) => state.adjustments);
  return (
    <CurvePresetControl
      adjustments={adjustments}
      setAdjustments={(update) =>
        useEditorStore.setState({ adjustments: update(useEditorStore.getState().adjustments) })
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
  expect(preset.adjustments).toEqual({
    curves: curve(120),
    pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
    parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
    curveMode: 'point',
  });
});

it('applies a point preset over a parametric target without retaining the target curve state', async () => {
  const pointCurves = curve(170);
  const savedCurve: Preset = {
    id: 'point',
    name: 'Point look',
    presetType: 'tool',
    curvePreset: true,
    adjustments: {
      curves: pointCurves,
      pointCurves,
      parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
      curveMode: 'point',
    },
  };
  usePresetCatalog.setState({ presets: [{ preset: savedCurve }] });
  await act(async () =>
    useEditorStore.setState({
      adjustments: {
        ...INITIAL_ADJUSTMENTS,
        exposure: 2,
        curveMode: 'parametric',
        curves: curve(90),
        pointCurves: curve(80),
        parametricCurve: {
          ...INITIAL_ADJUSTMENTS.parametricCurve!,
          luma: { ...INITIAL_ADJUSTMENTS.parametricCurve!.luma, highlights: 65 },
        },
      },
    }),
  );

  await choosePreset('Point look');
  expect(useEditorStore.getState().adjustments).toMatchObject({
    exposure: 2,
    curveMode: 'point',
    curves: pointCurves,
    pointCurves,
    parametricCurve: INITIAL_ADJUSTMENTS.parametricCurve,
  });
});

it('clears the selected name after a curve edit but not an unrelated adjustment', async () => {
  const appliedCurve = curve(170);
  const savedCurve: Preset = {
    id: 'signature',
    name: 'Signature',
    presetType: 'tool',
    curvePreset: true,
    adjustments: {
      curves: {
        red: appliedCurve.red,
        luma: appliedCurve.luma,
        green: appliedCurve.green,
        blue: appliedCurve.blue,
      },
      curveMode: 'point',
    },
  };
  usePresetCatalog.setState({ presets: [{ preset: savedCurve }] });
  await choosePreset('Signature');
  const dropdown = container.querySelector('button[aria-haspopup="listbox"]')!;
  expect(dropdown.textContent).toContain('Signature');

  await act(async () =>
    useEditorStore.setState({ adjustments: { ...useEditorStore.getState().adjustments, exposure: 4 } }),
  );
  expect(dropdown.textContent).toContain('Signature');

  await act(async () =>
    useEditorStore.setState({ adjustments: { ...useEditorStore.getState().adjustments, curves: curve(120) } }),
  );
  expect(dropdown.textContent).toContain('adjustments.curves.presets.select');
  expect(container.querySelector('button[aria-label="adjustments.curves.presets.rename"]')).toBeNull();
});
