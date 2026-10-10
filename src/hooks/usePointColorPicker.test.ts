// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { KonvaEventObject } from 'konva/lib/Node';
import '../i18n';
import { mockCommand } from '../test/tauriMock';
import { Invokes, Panel, type SelectedImage } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import { INITIAL_ADJUSTMENTS, INITIAL_MASK_CONTAINER, type Adjustments } from '../utils/adjustments';
import { createPointColor, type PointColorSample } from '../utils/pointColor';
import { usePointColorPicker } from './usePointColorPicker';
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialEditor = useEditorStore.getState(),
  initialUI = useUIStore.getState();
const color = { lightness: 0.7, chroma: 0.1, hue: 30 };
const event = { target: { getStage: () => ({}) }, evt: new MouseEvent('mousedown') } as unknown as KonvaEventObject<
  MouseEvent | TouchEvent
>;
let root: Root, picker: ReturnType<typeof usePointColorPicker>;
const setAdjustments = vi.fn((updater: (prev: Adjustments) => Adjustments) =>
  useEditorStore.getState().setEditor({ adjustments: updater(useEditorStore.getState().adjustments) }),
);
beforeEach(async () => {
  setAdjustments.mockClear();
  mockCommand(Invokes.SamplePointColorInput, () => color);
  useEditorStore.setState({
    adjustments: structuredClone(INITIAL_ADJUSTMENTS),
    selectedImage: {
      path: '/owned/a.raw',
      isReady: true,
      width: 6000,
      height: 4000,
      isRaw: true,
      thumbnailUrl: '',
      exif: null,
    } satisfies SelectedImage,
    isPointColorPickerActive: true,
    pointColorPickerMaskId: null,
    selectedPointColorId: null,
    isWbPickerActive: false,
    mixerPickerProperty: null,
  });
  useUIStore.getState().setUI({ activeView: 'editor', activePanel: Panel.Adjustments });
  function Harness() {
    picker = usePointColorPicker({
      getCanvasPointer: () => ({ x: 50, y: 50 }),
      imageRenderSize: {
        width: 100,
        height: 100,
        scale: 1,
        offsetX: 0,
        offsetY: 0,
        containerWidth: 100,
        containerHeight: 100,
      },
      zoomScale: 1,
      setAdjustments,
    });
    return null;
  }
  root = createRoot(document.createElement('div'));
  await act(async () => root.render(createElement(Harness)));
});
afterEach(async () => {
  await act(async () => root.unmount());
  useEditorStore.setState(initialEditor, true);
  useUIStore.setState(initialUI, true);
});
it('adds a zero-shift swatch through one edit and ends the picker', async () => {
  await act(async () => picker.start(event));
  expect(setAdjustments).toHaveBeenCalledOnce();
  expect(useEditorStore.getState().adjustments.pointColor).toHaveLength(1);
  expect(useEditorStore.getState().adjustments.pointColor![0]).toMatchObject({
    color,
    hueShift: 0,
    picked: { x: 0.5, y: 0.5 },
  });
  expect(useEditorStore.getState().isPointColorPickerActive).toBe(false);
});
it('re-picks the selected swatch at the eight-point limit', async () => {
  const points = Array.from({ length: 8 }, (_, index) => createPointColor({ ...color, hue: 100 }, `point${index}`));
  await act(async () =>
    useEditorStore
      .getState()
      .setEditor({ adjustments: { ...INITIAL_ADJUSTMENTS, pointColor: points }, selectedPointColorId: 'point3' }),
  );
  await act(async () => picker.start(event));
  const result = useEditorStore.getState().adjustments.pointColor!;
  expect(result).toHaveLength(8);
  expect(result[3].color).toEqual(color);
  expect(result[0]).toBe(points[0]);
});
it('does not apply a sample after a competing edit', async () => {
  let resolve!: (value: PointColorSample) => void;
  mockCommand(
    Invokes.SamplePointColorInput,
    () =>
      new Promise<PointColorSample>((done) => {
        resolve = done;
      }),
  );
  let pending!: Promise<void>;
  await act(async () => {
    pending = picker.start(event);
  });
  await act(async () => useEditorStore.getState().setEditor({ adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 1 } }));
  await act(async () => {
    resolve(color);
    await pending;
  });
  expect(setAdjustments).not.toHaveBeenCalled();
  expect(useEditorStore.getState().adjustments.exposure).toBe(1);
});
it('writes only the selected mask swatches', async () => {
  await act(async () => {
    useUIStore.getState().setPanel(Panel.Masks);
    useEditorStore.getState().setEditor({
      activeMaskContainerId: 'mask',
      pointColorPickerMaskId: 'mask',
      adjustments: { ...INITIAL_ADJUSTMENTS, masks: [{ ...structuredClone(INITIAL_MASK_CONTAINER), id: 'mask' }] },
    });
  });
  await act(async () => picker.start(event));
  expect(useEditorStore.getState().adjustments.pointColor).toEqual([]);
  expect(useEditorStore.getState().adjustments.masks[0].adjustments.pointColor).toHaveLength(1);
});
it('ends when a competing picker starts', async () => {
  await act(async () => useEditorStore.getState().setEditor({ isWbPickerActive: true }));
  expect(useEditorStore.getState().isPointColorPickerActive).toBe(false);
});
it('keeps picking from a visible Adjustments dock when another dock is active', async () => {
  await act(async () =>
    useUIStore.getState().setUI({
      activePanel: Panel.Masks,
      activePanels: { ...useUIStore.getState().activePanels, leftTop: Panel.Masks, rightTop: Panel.Adjustments },
      uiVisibility: { ...useUIStore.getState().uiVisibility, leftPanel: true, rightPanel: true },
    }),
  );
  expect(useEditorStore.getState().isPointColorPickerActive).toBe(true);
});
it('does not apply a canceled sample after restarting the picker', async () => {
  let resolve!: (value: PointColorSample) => void;
  mockCommand(
    Invokes.SamplePointColorInput,
    () =>
      new Promise<PointColorSample>((done) => {
        resolve = done;
      }),
  );
  let pending!: Promise<void>;
  await act(async () => {
    pending = picker.start(event);
  });
  await act(async () => useEditorStore.getState().setEditor({ isPointColorPickerActive: false }));
  await act(async () => useEditorStore.getState().setEditor({ isPointColorPickerActive: true }));
  await act(async () => {
    resolve(color);
    await pending;
  });
  expect(setAdjustments).not.toHaveBeenCalled();
  expect(useEditorStore.getState().isPointColorPickerActive).toBe(true);
});
it('permits a new request after cancellation and ignores old success/finally', async () => {
  const resolves: Array<(value: PointColorSample) => void> = [];
  mockCommand(Invokes.SamplePointColorInput, () => new Promise<PointColorSample>((done) => resolves.push(done)));
  let old!: Promise<void>, next!: Promise<void>;
  await act(async () => {
    old = picker.start(event);
  });
  await act(async () => {
    useEditorStore.getState().setEditor({ isPointColorPickerActive: false });
    useEditorStore.getState().setEditor({ isPointColorPickerActive: true });
  });
  await act(async () => {
    next = picker.start(event);
  });
  expect(resolves).toHaveLength(2);
  await act(async () => {
    resolves[0]({ ...color, hue: 100 });
    await old;
  });
  expect(setAdjustments).not.toHaveBeenCalled();
  await act(async () => {
    resolves[1](color);
    await next;
  });
  expect(setAdjustments).toHaveBeenCalledOnce();
  expect(useEditorStore.getState().adjustments.pointColor![0].color).toEqual(color);
});
it('keeps mask picking active in another visible dock and cancels when it hides', async () => {
  await act(async () => {
    useUIStore.getState().setUI({
      activePanel: Panel.Adjustments,
      activePanels: { ...useUIStore.getState().activePanels, leftTop: Panel.Masks, rightTop: Panel.Adjustments },
      uiVisibility: { ...useUIStore.getState().uiVisibility, leftPanel: true, rightPanel: true },
    });
    useEditorStore.getState().setEditor({ activeMaskContainerId: 'mask', pointColorPickerMaskId: 'mask' });
  });
  expect(useEditorStore.getState().isPointColorPickerActive).toBe(true);
  await act(async () =>
    useUIStore.getState().setUI({ uiVisibility: { ...useUIStore.getState().uiVisibility, leftPanel: false } }),
  );
  expect(useEditorStore.getState().isPointColorPickerActive).toBe(false);
});
