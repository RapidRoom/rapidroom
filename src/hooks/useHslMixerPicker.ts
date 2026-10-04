import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type Konva from 'konva';
import type { KonvaEventObject } from 'konva/lib/Node';
import { useEditorStore } from '../store/useEditorStore';
import { Adjustments, HueSatLum, INITIAL_ADJUSTMENTS } from '../utils/adjustments';
import { HslMixerBand, HslMixerProperty, HslPresence, sampleHslPresence } from '../utils/hslMixer';
import { RenderSize } from './useImageRenderSize';
import { Invokes } from '../components/ui/AppProperties';
import { FINE_ADJUSTMENT_MULTIPLIER } from '../components/ui/Slider';

const SAMPLE_SCREEN_SIZE = 16;
const DRAG_GAIN = 0.5;

interface Coord {
  x: number;
  y: number;
}

interface PickerDrag {
  path: string | undefined;
  property: HslMixerProperty;
  lastY: number;
  offset: number;
  appliedKey: string;
  baseKey: string;
  base: Record<string, HueSatLum>;
  presence: HslPresence | null;
  offsetRange: [number, number];
}

interface HslMixerPickerOptions {
  getCanvasPointer(stage: Konva.Stage): Coord | null;
  imageRenderSize: RenderSize;
  previewUrl: string | null;
  useDisplayTexture: boolean;
  zoomScale: number;
  setAdjustments(fn: (prev: Adjustments) => Adjustments): void;
}

const clampOffset = ({ offsetRange: [min, max] }: PickerDrag, offset: number) => Math.max(min, Math.min(max, offset));

const getClientY = (e: MouseEvent | TouchEvent) => ('touches' in e ? e.touches[0]?.clientY : e.clientY);

const getDragValues = ({ presence, base, property, offset }: PickerDrag): Array<[string, number]> =>
  Object.entries(presence ?? {}).map(([band, weight]) => [
    band,
    offset === 0
      ? base[band][property]
      : Math.max(-100, Math.min(100, Math.round(base[band][property] + offset * weight))),
  ]);

export function useHslMixerPicker({
  getCanvasPointer,
  imageRenderSize,
  previewUrl,
  useDisplayTexture,
  zoomScale,
  setAdjustments,
}: HslMixerPickerOptions) {
  const property = useEditorStore((state) => state.mixerPickerProperty);
  const setEditor = useEditorStore((state) => state.setEditor);
  const [isDragging, setIsDragging] = useState(false);
  const dragRef = useRef<PickerDrag | null>(null);
  const bitmapRef = useRef<{ url: string; bitmap: Promise<ImageBitmap> } | null>(null);
  const detachRef = useRef<(() => void) | null>(null);
  const setAdjustmentsRef = useRef(setAdjustments);

  useEffect(() => {
    setAdjustmentsRef.current = setAdjustments;
  }, [setAdjustments]);

  const getBitmap = useCallback((url: string) => {
    if (bitmapRef.current?.url !== url) {
      bitmapRef.current?.bitmap.then((bitmap) => bitmap.close()).catch(() => {});
      bitmapRef.current = {
        url,
        bitmap: fetch(url)
          .then((res) => res.blob())
          .then((blob) => createImageBitmap(blob)),
      };
    }
    return bitmapRef.current.bitmap;
  }, []);

  const sampleArea = useCallback(
    async (u: number, v: number, radius: number): Promise<Uint8ClampedArray> => {
      if (useDisplayTexture) {
        return Uint8ClampedArray.from(await invoke<number[]>(Invokes.SampleDisplayArea, { x: u, y: v, radius }));
      }
      if (!previewUrl) return new Uint8ClampedArray();
      const bitmap = await getBitmap(previewUrl);
      const half = Math.max(1, radius * bitmap.width);
      const x0 = Math.max(0, Math.floor(u * bitmap.width - half));
      const y0 = Math.max(0, Math.floor(v * bitmap.height - half));
      const w = Math.max(1, Math.min(bitmap.width, Math.ceil(u * bitmap.width + half)) - x0);
      const h = Math.max(1, Math.min(bitmap.height, Math.ceil(v * bitmap.height + half)) - y0);
      const canvas = document.createElement('canvas');
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext('2d', { willReadFrequently: true });
      if (!ctx) return new Uint8ClampedArray();
      ctx.drawImage(bitmap, x0, y0, w, h, 0, 0, w, h);
      return ctx.getImageData(0, 0, w, h).data;
    },
    [useDisplayTexture, previewUrl, getBitmap],
  );

  const end = useCallback(() => {
    detachRef.current?.();
    detachRef.current = null;
    const drag = dragRef.current;
    if (!drag) return;
    dragRef.current = null;
    setIsDragging(false);
    setEditor({ isSliderDragging: false });
    // Drag moves skip the undo history; record the whole gesture as one step.
    if (drag.appliedKey !== drag.baseKey && useEditorStore.getState().selectedImage?.path === drag.path) {
      setAdjustmentsRef.current((prev) => prev);
    }
  }, [setEditor]);

  const applyDrag = useCallback(
    (drag: PickerDrag) => {
      if (!drag.presence) return;
      if (useEditorStore.getState().selectedImage?.path !== drag.path) {
        end();
        return;
      }
      const values = getDragValues(drag);
      const appliedKey = values.join();
      if (appliedKey === drag.appliedKey) return;
      drag.appliedKey = appliedKey;
      const key = drag.property;
      setEditor((state) => {
        const hsl = { ...INITIAL_ADJUSTMENTS.hsl, ...state.adjustments.hsl };
        values.forEach(([band, value]) => {
          hsl[band] = { ...hsl[band], [key]: value };
        });
        return {
          adjustments: { ...state.adjustments, hsl },
          ...(state.showOriginal ? { showOriginal: false, previewOverride: null } : {}),
        };
      });
    },
    [setEditor, end],
  );

  const start = useCallback(
    (e: KonvaEventObject<MouseEvent | TouchEvent>) => {
      const stage = e.target.getStage();
      const pos = stage && getCanvasPointer(stage);
      const clientY = getClientY(e.evt);
      if (!property || !pos || clientY === undefined) return;
      if (pos.x < 0 || pos.y < 0 || pos.x > imageRenderSize.width || pos.y > imageRenderSize.height) return;

      end();
      // The canvas prevents default on press, so a slider still being typed into would keep focus
      // and later commit its stale value over the drag. Commit it now, before taking the base.
      const focused = document.activeElement;
      if (focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement) focused.blur();
      const { adjustments, selectedImage } = useEditorStore.getState();
      const drag: PickerDrag = {
        path: selectedImage?.path,
        property,
        lastY: clientY,
        offset: 0,
        appliedKey: '',
        baseKey: '',
        base: { ...INITIAL_ADJUSTMENTS.hsl, ...adjustments.hsl },
        presence: null,
        offsetRange: [-Infinity, Infinity],
      };
      dragRef.current = drag;
      setIsDragging(true);
      setEditor({ isSliderDragging: true });

      const onMove = (ev: MouseEvent | TouchEvent) => {
        const y = getClientY(ev);
        if (y === undefined) return;
        if (ev.cancelable) ev.preventDefault();
        const multiplier = ev.altKey ? FINE_ADJUSTMENT_MULTIPLIER : 1;
        drag.offset = clampOffset(drag, drag.offset + (drag.lastY - y) * DRAG_GAIN * multiplier);
        drag.lastY = y;
        applyDrag(drag);
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('touchmove', onMove, { passive: false });
      window.addEventListener('mouseup', end);
      window.addEventListener('touchend', end);
      window.addEventListener('touchcancel', end);
      window.addEventListener('blur', end);
      detachRef.current = () => {
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('touchmove', onMove);
        window.removeEventListener('mouseup', end);
        window.removeEventListener('touchend', end);
        window.removeEventListener('touchcancel', end);
        window.removeEventListener('blur', end);
      };

      const radius = SAMPLE_SCREEN_SIZE / 2 / zoomScale / imageRenderSize.width;
      sampleArea(pos.x / imageRenderSize.width, pos.y / imageRenderSize.height, radius)
        .then((pixels) => {
          if (dragRef.current !== drag) return;
          drag.presence = sampleHslPresence(pixels, drag.property);
          drag.baseKey = getDragValues({ ...drag, offset: 0 }).join();
          drag.appliedKey = drag.baseKey;
          const dominant = Object.keys(drag.presence).find((band) => drag.presence?.[band as HslMixerBand] === 1);
          if (dominant) {
            const value = drag.base[dominant][drag.property];
            drag.offsetRange = [-100 - value, 100 - value];
            drag.offset = clampOffset(drag, drag.offset);
          }
          applyDrag(drag);
        })
        .catch((err) => console.error('Failed to sample preview for color mixer picker:', err));
    },
    [property, imageRenderSize, zoomScale, getCanvasPointer, sampleArea, setEditor, applyDrag, end],
  );

  useEffect(() => {
    if (!property) end();
  }, [property, end]);

  useEffect(
    () => () => {
      end();
      bitmapRef.current?.bitmap.then((bitmap) => bitmap.close()).catch(() => {});
    },
    [end],
  );

  return { isActive: property !== null, isDragging, start };
}
