import type { Adjustments } from './adjustments';
import schema from '../../rapidroom/adjustment-schema.json';

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
          key in targetObject ? blend(value, sourceObject[key], path ? `${path}.${key}` : key) : value,
        ]),
      );
    }
    return target;
  };
  return blend(preset, before, '') as Partial<Adjustments>;
}
