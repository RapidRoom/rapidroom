import type { Adjustments } from './adjustments';
import { POINT_COLOR_RANGES, type PointColor } from './pointColor';

export type PointColorNumericKey = keyof typeof POINT_COLOR_RANGES;
export interface PointColorTarget {
  maskId: string | null;
  pointId: string;
}

function pointsForScope(adjustments: Adjustments, maskId: string | null): PointColor[] | undefined {
  return maskId === null
    ? adjustments.pointColor
    : adjustments.masks.find((mask) => mask.id === maskId)?.adjustments.pointColor;
}

export function selectedPointColorTarget(
  adjustments: Adjustments,
  maskId: string | null,
  selectedId: string | null,
): PointColorTarget | null {
  const points = pointsForScope(adjustments, maskId);
  const point = points?.find((point) => point.id === selectedId) ?? points?.[0];
  return point ? { maskId, pointId: point.id } : null;
}

export function pointColorNumericValue(
  adjustments: Adjustments,
  target: PointColorTarget,
  key: PointColorNumericKey,
): number | undefined {
  return pointsForScope(adjustments, target.maskId)?.find((point) => point.id === target.pointId)?.[key];
}

function changePoints(
  adjustments: Adjustments,
  target: PointColorTarget,
  change: (point: PointColor) => PointColor | null,
): Adjustments {
  const points = pointsForScope(adjustments, target.maskId);
  if (!points?.some((point) => point.id === target.pointId)) return adjustments;
  const next = points.flatMap((point) => {
    if (point.id !== target.pointId) return [point];
    const changed = change(point);
    return changed ? [changed] : [];
  });
  if (next.length === points.length && next.every((point, index) => point === points[index])) return adjustments;
  if (target.maskId === null) return { ...adjustments, pointColor: next };
  return {
    ...adjustments,
    masks: adjustments.masks.map((mask) =>
      mask.id === target.maskId ? { ...mask, adjustments: { ...mask.adjustments, pointColor: next } } : mask,
    ),
  };
}

export function setPointColorNumericValue(
  adjustments: Adjustments,
  target: PointColorTarget,
  key: PointColorNumericKey,
  requested: number,
): Adjustments {
  if (!Number.isFinite(requested)) return adjustments;
  const [minimum, maximum] = POINT_COLOR_RANGES[key];
  const value = Math.max(minimum, Math.min(maximum, requested));
  return changePoints(adjustments, target, (point) => (point[key] === value ? point : { ...point, [key]: value }));
}

export function removePointColor(adjustments: Adjustments, target: PointColorTarget): Adjustments {
  return changePoints(adjustments, target, () => null);
}

export function pointColorCommandId(target: PointColorTarget, key: PointColorNumericKey): string {
  return `pointColor:${encodeURIComponent(target.maskId ?? '')}:${encodeURIComponent(target.pointId)}:${key}`;
}

export function parsePointColorCommandId(id: string): { target: PointColorTarget; key: PointColorNumericKey } | null {
  const parts = id.split(':');
  if (parts.length !== 4 || parts[0] !== 'pointColor' || !Object.hasOwn(POINT_COLOR_RANGES, parts[3])) return null;
  try {
    const maskId = decodeURIComponent(parts[1]);
    const pointId = decodeURIComponent(parts[2]);
    return pointId ? { target: { maskId: maskId || null, pointId }, key: parts[3] as PointColorNumericKey } : null;
  } catch {
    return null;
  }
}

// An empty/deleted mask selection is unavailable, rather than a global edit.
export function pointColorScope(
  adjustments: Adjustments,
  isMaskContext: boolean,
  selectedMaskId: string | null,
): string | null | undefined {
  if (!isMaskContext) return null;
  return selectedMaskId && adjustments.masks.some((mask) => mask.id === selectedMaskId) ? selectedMaskId : undefined;
}
