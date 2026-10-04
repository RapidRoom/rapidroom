// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { KonvaEventObject } from 'konva/lib/Node';
import { mockCommand } from '../test/tauriMock';
import { useHslMixerPicker } from './useHslMixerPicker';
import { useEditorStore } from '../store/useEditorStore';
import { Invokes, SelectedImage } from '../components/ui/AppProperties';
import { Adjustments, INITIAL_ADJUSTMENTS } from '../utils/adjustments';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const initialEditorState = useEditorStore.getState();
const RENDER_SIZE = {
  containerHeight: 100,
  containerWidth: 100,
  height: 100,
  offsetX: 0,
  offsetY: 0,
  scale: 1,
  width: 100,
};
const RED_PIXELS = Array.from({ length: 16 }, () => [255, 0, 0, 255]).flat();

const image = (path: string) => ({ path, isReady: true }) as SelectedImage;

const withRedSaturation = (saturation: number): Adjustments => ({
  ...INITIAL_ADJUSTMENTS,
  hsl: { ...INITIAL_ADJUSTMENTS.hsl, reds: { ...INITIAL_ADJUSTMENTS.hsl.reds, saturation } },
});

const pressEvent = (clientY: number) =>
  ({ target: { getStage: () => ({}) }, evt: new MouseEvent('mousedown', { clientY }) }) as unknown as KonvaEventObject<
    MouseEvent | TouchEvent
  >;

const move = (clientY: number, altKey = false) =>
  act(async () => {
    window.dispatchEvent(new MouseEvent('mousemove', { clientY, altKey, cancelable: true }));
  });

const release = () =>
  act(async () => {
    window.dispatchEvent(new MouseEvent('mouseup'));
  });

async function mountPicker() {
  // Mirrors Editor's setAdjustments; each call is what would reach the undo history.
  const setAdjustments = vi.fn((fn: (prev: Adjustments) => Adjustments) =>
    useEditorStore.setState((state) => ({ adjustments: fn(state.adjustments) })),
  );
  let picker!: ReturnType<typeof useHslMixerPicker>;
  function Harness() {
    picker = useHslMixerPicker({
      getCanvasPointer: () => ({ x: 50, y: 50 }),
      imageRenderSize: RENDER_SIZE,
      previewUrl: null,
      useDisplayTexture: true,
      zoomScale: 1,
      setAdjustments,
    });
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => {
    root.render(createElement(Harness));
  });
  const press = (clientY: number) =>
    act(async () => {
      picker.start(pressEvent(clientY));
    });
  return { setAdjustments, press, unmount: () => act(async () => root.unmount()) };
}

const redSaturation = () => useEditorStore.getState().adjustments.hsl.reds.saturation;

beforeEach(() => {
  mockCommand(Invokes.SampleDisplayArea, () => RED_PIXELS);
  useEditorStore.setState({
    selectedImage: image('/photos/a.raw'),
    adjustments: withRedSaturation(10),
    mixerPickerProperty: 'saturation',
  });
});

afterEach(() => {
  useEditorStore.setState(initialEditorState, true);
});

describe('useHslMixerPicker', () => {
  it('records a whole drag as one undo step', async () => {
    const { setAdjustments, press, unmount } = await mountPicker();

    await press(100);
    expect(useEditorStore.getState().isSliderDragging).toBe(true);
    await move(80);
    await move(60);
    expect(redSaturation()).toBe(30);
    expect(setAdjustments).not.toHaveBeenCalled();

    await release();
    expect(setAdjustments).toHaveBeenCalledTimes(1);
    expect(redSaturation()).toBe(30);
    expect(useEditorStore.getState().isSliderDragging).toBe(false);
    await unmount();
  });

  it('does not write the sampled preview back before the drag moves', async () => {
    const { setAdjustments, press, unmount } = await mountPicker();
    const before = useEditorStore.getState().adjustments;

    await press(100);
    await release();

    expect(useEditorStore.getState().adjustments).toBe(before);
    expect(setAdjustments).not.toHaveBeenCalled();
    await unmount();
  });

  it('uses fine steps with Alt', async () => {
    const { press, unmount } = await mountPicker();

    await press(100);
    await move(80, true);
    expect(redSaturation()).toBe(12);
    await release();
    await unmount();
  });

  it('stops a drag when the image changes under it', async () => {
    const { setAdjustments, press, unmount } = await mountPicker();

    await press(100);
    await move(80);
    const other = withRedSaturation(-40);
    useEditorStore.setState({ selectedImage: image('/photos/b.raw'), adjustments: other });
    await move(40);
    await release();

    expect(useEditorStore.getState().adjustments).toBe(other);
    expect(setAdjustments).not.toHaveBeenCalled();
    expect(useEditorStore.getState().isSliderDragging).toBe(false);
    await unmount();
  });

  it('removes its window listeners when the picker is turned off mid-drag', async () => {
    const { setAdjustments, press, unmount } = await mountPicker();

    await press(100);
    await move(80);
    await act(async () => {
      useEditorStore.setState({ mixerPickerProperty: null });
    });
    await move(40);

    expect(redSaturation()).toBe(20);
    expect(setAdjustments).toHaveBeenCalledTimes(1);
    expect(useEditorStore.getState().isSliderDragging).toBe(false);
    await unmount();
  });
});
