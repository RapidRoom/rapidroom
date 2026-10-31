import {
  buildParametricCurvePoints,
  type Adjustments,
  type Curves,
  type NestedLookCurve,
  type ParametricCurve,
} from './adjustments';
import type { Preset } from '../components/ui/AppProperties';
import schema from '../../rapidroom/adjustment-schema.json';
import { normalizeLensPolicyVersion } from './lensPolicyVersion';

const parameters: Record<
  string,
  { defaultType: string; uiRanges?: { minimum: number | null; maximum: number | null }[] }
> = schema.parameters;

type Point = { x: number; y: number };
const isPoint = (v: unknown): v is Point =>
  !!v &&
  typeof v === 'object' &&
  'x' in v &&
  'y' in v &&
  typeof v.x === 'number' &&
  typeof v.y === 'number' &&
  Number.isFinite(v.x) &&
  Number.isFinite(v.y);
const object = (v: unknown): Record<string, unknown> | null =>
  v !== null && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
// Match the bounded monotone Hermite evaluator used by the native renderer.
export const presetCurveY = (points: Point[], x: number): number => {
  if (points.length < 2) return x;
  if (x <= points[0].x) return points[0].y;
  if (x >= points[points.length - 1].x) return points[points.length - 1].y;
  for (let i = 0; i < points.length - 1; i++) {
    const p1 = points[i],
      p2 = points[i + 1];
    if (x > p2.x) continue;
    const p0 = points[Math.max(1, i) - 1],
      p3 = points[Math.min(i + 2, points.length - 1)];
    const before = (p1.y - p0.y) / Math.max(0.001, p1.x - p0.x);
    const current = (p2.y - p1.y) / Math.max(0.001, p2.x - p1.x);
    const after = (p3.y - p2.y) / Math.max(0.001, p3.x - p2.x);
    let m1 = i === 0 ? current : before * current <= 0 ? 0 : (before + current) / 2;
    let m2 = i + 1 === points.length - 1 ? current : current * after <= 0 ? 0 : (current + after) / 2;
    if (current !== 0) {
      const magnitude = (m1 / current) ** 2 + (m2 / current) ** 2;
      if (magnitude > 9) {
        const scale = 3 / Math.sqrt(magnitude);
        m1 *= scale;
        m2 *= scale;
      }
    }
    const dx = p2.x - p1.x;
    if (dx <= 0) return p1.y;
    const t = (x - p1.x) / dx,
      t2 = t * t,
      t3 = t2 * t;
    return Math.max(
      0,
      Math.min(
        255,
        (2 * t3 - 3 * t2 + 1) * p1.y + (t3 - 2 * t2 + t) * m1 * dx + (-2 * t3 + 3 * t2) * p2.y + (t3 - t2) * m2 * dx,
      ),
    );
  }
  return points[points.length - 1].y;
};

const DEFAULT_POINT_CURVE: Point[] = [
  { x: 0, y: 0 },
  { x: 255, y: 255 },
];

/** The renderer stores a baked master curve, so preserve Lightroom's look,
 * parametric, then point-curve order when editable inputs change. */
export function composeParametricCurves(
  parametric: ParametricCurve,
  pointCurves: Curves,
  nestedLookCurves: NestedLookCurve[] = [],
): Curves {
  const channels = ['luma', 'red', 'green', 'blue'] as const;
  return Object.fromEntries(
    channels.map((channel) => {
      const settings = parametric[channel];
      const points = pointCurves[channel] ?? DEFAULT_POINT_CURVE;
      const isIdentity =
        settings.shadows === 0 &&
        settings.darks === 0 &&
        settings.lights === 0 &&
        settings.highlights === 0 &&
        settings.blackLevel === 0 &&
        settings.whiteLevel === 0;
      if (isIdentity && (channel !== 'luma' || nestedLookCurves.length === 0)) return [channel, points];

      const parametricPoints = buildParametricCurvePoints(settings);
      const combined = Array.from({ length: 16 }, (_, index) => {
        const x = Math.round((index * 255) / 15);
        let input = x;
        if (channel === 'luma') {
          for (const look of nestedLookCurves) {
            const amount = Math.max(0, look.amount);
            input += amount * (presetCurveY(look.points, input) - input);
          }
        }
        const y = presetCurveY(points, presetCurveY(parametricPoints, input));
        return { x, y: Math.round(y) };
      });
      return [channel, combined];
    }),
  ) as Curves;
}

// A neutral wheel's hue is invisible: preserve the coloured wheel while its
// saturation fades. Two coloured wheels travel along the shortest hue arc.
const blendGradingHue = (
  target: Record<string, unknown>,
  source: Record<string, unknown>,
  fraction: number,
): unknown => {
  const to = target.hue;
  const from = source.hue;
  if (typeof to !== 'number' || !Number.isFinite(to)) return from;
  if (typeof from !== 'number' || !Number.isFinite(from) || source.saturation === 0) return to;
  if (target.saturation === 0) return from;
  const delta = ((to - from + 540) % 360) - 180;
  return (((from + delta * fraction) % 360) + 360) % 360;
};

export function mixAdjustments(
  preset: Partial<Adjustments>,
  amount: number,
  before: Adjustments,
): Partial<Adjustments> {
  const fraction = Math.max(0, Math.min(200, Number.isFinite(amount) ? amount : 100)) / 100;
  if (fraction === 0) return before;
  const blend = (target: unknown, source: unknown, path: string): unknown => {
    if (typeof target === 'number') {
      if (!Number.isFinite(target)) return source;
      const value =
        typeof source === 'number' && Number.isFinite(source) ? source + (target - source) * fraction : target;
      const range = parameters[path]?.uiRanges?.[0];
      return range ? Math.max(range.minimum ?? -Infinity, Math.min(range.maximum ?? Infinity, value)) : value;
    }
    if (Array.isArray(target)) {
      if (target.length && target.every(isPoint) && Array.isArray(source) && source.length && source.every(isPoint)) {
        if (fraction === 1) return target;
        const knots = [...new Set([...target, ...source].map((p) => p.x))].sort((a, b) => a - b);
        // Keep both endpoints and at most sixteen knots, matching the GPU representation.
        const xs =
          knots.length <= 16
            ? knots
            : Array.from({ length: 16 }, (_, i) => knots[Math.round((i * (knots.length - 1)) / 15)]);
        return xs.map((x) => ({
          x,
          y: Math.max(
            0,
            Math.min(255, presetCurveY(source, x) + (presetCurveY(target, x) - presetCurveY(source, x)) * fraction),
          ),
        }));
      }
      return target;
    }
    const targetObject = object(target);
    if (targetObject) {
      const sourceObject = object(source) ?? {};
      return Object.fromEntries(
        Object.entries({ ...sourceObject, ...targetObject }).map(([key, value]) => [
          key,
          key in targetObject
            ? key === 'hue' && /^colorGrading\.(shadows|midtones|highlights|global)$/.test(path)
              ? blendGradingHue(targetObject, sourceObject, fraction)
              : blend(value, sourceObject[key], path ? `${path}.${key}` : key)
            : value,
        ]),
      );
    }
    return target;
  };
  const hasRetainedLook = (before.nestedLookCurves?.length ?? 0) > 0;
  const pointSource =
    before.curveMode === 'parametric' || hasRetainedLook ? (before.pointCurves ?? before.curves) : before.curves;
  const baseline = { ...before, pointCurves: pointSource };
  const result = blend(preset, baseline, '') as Partial<Adjustments>;
  if (preset.curveMode === 'parametric' && preset.parametricCurve && result.parametricCurve) {
    const pointCurves = preset.pointCurves ? (result.pointCurves ?? pointSource) : pointSource;
    result.pointCurves = pointCurves;
    result.curveMode = 'parametric';
    result.curves = composeParametricCurves(
      result.parametricCurve,
      pointCurves as Curves,
      result.nestedLookCurves ?? before.nestedLookCurves,
    );
  }
  if (preset.exposure !== undefined && preset.cameraExposureCompensation === undefined) {
    result.cameraExposureCompensation = false;
  }
  return normalizeLensPolicyVersion(result, preset, before, fraction * 100);
}

// Style masks are additions to the existing edit. Templates use unit image
// coordinates; the saved edit always receives ordinary editable pixel geometry.
export function mixPresetAdjustments(
  preset: Pick<Preset, 'adjustments' | 'presetType'>,
  amount: number,
  before: Adjustments,
  imageSize: { width: number; height: number } | null,
): Partial<Adjustments> {
  const mixed = mixAdjustments(preset.adjustments, amount, before);
  if (mixed === before || preset.presetType !== 'style' || !preset.adjustments.masks?.length) return mixed;
  const fraction = Math.max(0, Math.min(200, Number.isFinite(amount) ? amount : 100)) / 100;
  const ids = new Set((before.masks ?? []).flatMap((mask) => [mask.id, ...mask.subMasks.map((subMask) => subMask.id)]));
  const uniqueId = (baseId: string) => {
    let id = baseId;
    for (let suffix = 1; ids.has(id); suffix++) id = `${baseId}:${suffix}`;
    ids.add(id);
    return id;
  };
  const masks = preset.adjustments.masks.flatMap((template, index) => {
    if (
      template.relativeToImage &&
      (!imageSize ||
        !Number.isFinite(imageSize.width) ||
        !Number.isFinite(imageSize.height) ||
        imageSize.width <= 0 ||
        imageSize.height <= 0)
    )
      return [];
    const { relativeToImage, ...mask } = template;
    const baseId = typeof mask.id === 'string' ? mask.id : `style-mask-${index}`;
    const id = uniqueId(baseId);
    return [
      {
        ...mask,
        id,
        opacity: Math.min(100, Math.max(0, (template.opacity ?? 100) * fraction)),
        subMasks: template.subMasks.map((subMask, subIndex) => {
          const id = uniqueId(subMask.id ?? `${baseId}:component-${subIndex}`);
          if (!relativeToImage || !imageSize || !['radial', 'linear'].includes(subMask.type)) return { ...subMask, id };
          const parameters = { ...subMask.parameters };
          for (const key of ['centerX', 'radiusX', 'startX', 'endX'] as const) {
            const value = parameters[key];
            if (typeof value === 'number') parameters[key] = value * imageSize.width;
          }
          for (const key of ['centerY', 'radiusY', 'startY', 'endY'] as const) {
            const value = parameters[key];
            if (typeof value === 'number') parameters[key] = value * imageSize.height;
          }
          return { ...subMask, id, parameters };
        }),
      },
    ];
  });
  return { ...mixed, masks: [...(before.masks ?? []), ...masks] };
}
