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
const curveY = (points: Point[], x: number): number => {
  if (x <= points[0].x) return points[0].y;
  for (let i = 1; i < points.length; i++) {
    if (x <= points[i].x) {
      const left = points[i - 1],
        right = points[i];
      return right.x === left.x ? right.y : left.y + ((right.y - left.y) * (x - left.x)) / (right.x - left.x);
    }
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
        const xs = [...new Set([...target, ...source].map((p) => p.x))].sort((a, b) => a - b);
        return xs.map((x) => ({
          x,
          y: Math.max(0, Math.min(255, curveY(source, x) + (curveY(target, x) - curveY(source, x)) * fraction)),
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
