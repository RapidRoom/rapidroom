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
function scopeVisible(maskId: string | null, ui: ReturnType<typeof useUIStore.getState>): boolean {
  const target = maskId === null ? Panel.Adjustments : Panel.Masks;
  return Object.entries(ui.activePanels).some(([region, panel]) => {
    const actual = panel ?? ui.panelLayout[region as keyof typeof ui.panelLayout][0];
    return (
      actual === target &&
      (region.startsWith('left')
        ? ui.uiVisibility.leftPanel
        : region.startsWith('right')
          ? ui.uiVisibility.rightPanel
          : true)
    );
  });
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
  const panelVisible = useUIStore((state) => scopeVisible(scope, state));
  const visibility = useEditorStore(
    (state) =>
      (state.adjustments.sectionVisibility?.color ?? true) && (state.adjustments.sectionVisibility?.colorMixer ?? true),
  );
  const pending = useRef<number | null>(null);
  const generation = useRef(0);
  useEffect(() => {
    const invalidate = () => {
      generation.current++;
      pending.current = null;
    };
    const unsubscribeEditor = useEditorStore.subscribe((next, previous) => {
      if (
        next.isPointColorPickerActive !== previous.isPointColorPickerActive ||
        next.pointColorPickerMaskId !== previous.pointColorPickerMaskId ||
        next.selectedImage?.path !== previous.selectedImage?.path ||
        next.adjustments !== previous.adjustments ||
        next.isWbPickerActive !== previous.isWbPickerActive ||
        next.mixerPickerProperty !== previous.mixerPickerProperty ||
        next.activeMaskContainerId !== previous.activeMaskContainerId ||
        next.selectedPointColorId !== previous.selectedPointColorId
      )
        invalidate();
    });
    const unsubscribeUI = useUIStore.subscribe((next, previous) => {
      const scope = useEditorStore.getState().pointColorPickerMaskId;
      if (next.activeView !== previous.activeView || scopeVisible(scope, next) !== scopeVisible(scope, previous))
        invalidate();
    });
    return () => {
      invalidate();
      unsubscribeEditor();
      unsubscribeUI();
    };
  }, []);
  useEffect(() => {
    if (
      active &&
      (wb || mixer || view !== 'editor' || !panelVisible || !visibility || (scope !== null && selectedMask !== scope))
    )
      useEditorStore.getState().setEditor({ isPointColorPickerActive: false });
  }, [active, wb, mixer, selectedMask, scope, view, panelVisible, visibility]);
  useEffect(
    () => () => {
      useEditorStore.getState().setEditor({ isPointColorPickerActive: false });
    },
    [photo],
  );
  const start = useCallback(
    async (event: KonvaEventObject<MouseEvent | TouchEvent>) => {
      if (!useEditorStore.getState().isPointColorPickerActive || pending.current !== null) return;
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
      const request = ++generation.current;
      pending.current = request;
      try {
        const color = await invoke<PointColorSample>(Invokes.SamplePointColorInput, {
          path,
          maskId,
          adjustments: original,
          x,
          y,
          radius: Math.min(0.1, 8 / Math.max(0.01, zoomScale) / imageRenderSize.width),
        });
        const current = useEditorStore.getState();
        if (
          generation.current !== request ||
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
        if (generation.current === request) toast.error(t('pointColor.sampleFailed'));
      } finally {
        if (pending.current === request) pending.current = null;
      }
    },
    [active, getCanvasPointer, imageRenderSize.width, imageRenderSize.height, zoomScale, setAdjustments, t],
  );
  return { isActive: active, start };
}
