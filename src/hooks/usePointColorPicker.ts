import { useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { useTranslation } from 'react-i18next';
import type Konva from 'konva';
import type { KonvaEventObject } from 'konva/lib/Node';
import { v4 as uuid } from 'uuid';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';
import type { Adjustments } from '../utils/adjustments';
import { createPointColor, POINT_COLOR_LIMIT, type PointColorSample } from '../utils/pointColor';
import type { RenderSize } from './useImageRenderSize';
import { Invokes, Panel } from '../components/ui/AppProperties';

interface Options {
  getCanvasPointer(stage: Konva.Stage): { x: number; y: number } | null;
  imageRenderSize: RenderSize;
  zoomScale: number;
  setAdjustments(fn: (prev: Adjustments) => Adjustments): void;
}
export function usePointColorPicker({ getCanvasPointer, imageRenderSize, zoomScale, setAdjustments }: Options) {
  const { t } = useTranslation();
  const active = useEditorStore((state) => state.isPointColorPickerActive);
  const photo = useEditorStore((state) => state.selectedImage?.path);
  const wb = useEditorStore((state) => state.isWbPickerActive);
  const mixer = useEditorStore((state) => state.mixerPickerProperty);
  const selectedMask = useEditorStore((state) => state.activeMaskContainerId);
  const scope = useEditorStore((state) => state.pointColorPickerMaskId);
  const view = useUIStore((state) => state.activeView);
  const panel = useUIStore((state) => state.activePanel);
  const visibility = useEditorStore(
    (state) =>
      (state.adjustments.sectionVisibility?.color ?? true) && (state.adjustments.sectionVisibility?.colorMixer ?? true),
  );
  const pending = useRef(false);
  useEffect(() => {
    if (
      active &&
      (wb ||
        mixer ||
        view !== 'editor' ||
        (scope === null ? panel !== Panel.Adjustments : panel !== Panel.Masks) ||
        !visibility ||
        (scope !== null && selectedMask !== scope))
    )
      useEditorStore.getState().setEditor({ isPointColorPickerActive: false });
  }, [active, wb, mixer, selectedMask, scope, view, panel, visibility]);
  useEffect(
    () => () => {
      useEditorStore.getState().setEditor({ isPointColorPickerActive: false });
    },
    [photo],
  );
  const start = useCallback(
    async (event: KonvaEventObject<MouseEvent | TouchEvent>) => {
      if (!active || pending.current) return;
      const stage = event.target.getStage();
      if (!stage) return;
      const position = getCanvasPointer(stage);
      if (!position || imageRenderSize.width <= 0 || imageRenderSize.height <= 0) return;
      const x = position.x / imageRenderSize.width,
        y = position.y / imageRenderSize.height;
      if (x < 0 || x > 1 || y < 0 || y > 1) return;
      const state = useEditorStore.getState();
      const original = state.adjustments;
      const path = state.selectedImage?.path;
      const maskId = state.pointColorPickerMaskId;
      const selectedId = state.selectedPointColorId;
      if (!path || !state.selectedImage?.isReady) return;
      pending.current = true;
      try {
        const color = await invoke<PointColorSample>(Invokes.SamplePointColorInput, {
          path,
          adjustments: original,
          x,
          y,
          radius: Math.min(0.1, 8 / Math.max(0.01, zoomScale) / imageRenderSize.width),
        });
        const current = useEditorStore.getState();
        if (
          !current.isPointColorPickerActive ||
          current.selectedImage?.path !== path ||
          current.adjustments !== original ||
          current.pointColorPickerMaskId !== maskId
        )
          return;
        const target = maskId ? original.masks.find((mask) => mask.id === maskId)?.adjustments : original;
        if (!target) return;
        const points = target.pointColor ?? [];
        const selected = points.find((point) => point.id === selectedId) ?? points[0];
        const id = points.length >= POINT_COLOR_LIMIT && selected ? selected.id : uuid();
        const point = {
          ...(points.length >= POINT_COLOR_LIMIT && selected ? selected : createPointColor(color, id)),
          color,
          picked: { x, y },
        };
        const next =
          points.length >= POINT_COLOR_LIMIT
            ? points.map((existing) => (existing.id === id ? point : existing))
            : [...points, point];
        setAdjustments((prev) =>
          maskId
            ? {
                ...prev,
                masks: prev.masks.map((mask) =>
                  mask.id === maskId ? { ...mask, adjustments: { ...mask.adjustments, pointColor: next } } : mask,
                ),
              }
            : { ...prev, pointColor: next },
        );
        current.setEditor({ selectedPointColorId: id, isPointColorPickerActive: false });
      } catch {
        toast.error(t('pointColor.sampleFailed'));
      } finally {
        pending.current = false;
      }
    },
    [active, getCanvasPointer, imageRenderSize.width, imageRenderSize.height, zoomScale, setAdjustments, t],
  );
  return { isActive: active, start };
}
