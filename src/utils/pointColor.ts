export interface PointColorSample {
  lightness: number;
  chroma: number;
  hue: number;
}
export interface PointColor {
  id: string;
  color: PointColorSample;
  picked?: { x: number; y: number };
  hueShift: number;
  saturationShift: number;
  luminanceShift: number;
  hueRange: number;
  chromaRange: number;
  lightnessRange: number;
  smoothness: number;
}
export const POINT_COLOR_LIMIT = 8;
export const POINT_COLOR_RANGES = {
  hueShift: [-100, 100, 1],
  saturationShift: [-100, 100, 1],
  luminanceShift: [-100, 100, 1],
  hueRange: [0.1, 180, 1],
  chromaRange: [0.001, 0.5, 0.001],
  lightnessRange: [0.001, 1, 0.001],
  smoothness: [10, 100, 1],
} as const;
export function createPointColor(color: PointColorSample, id: string): PointColor {
  return {
    id,
    color,
    hueShift: 0,
    saturationShift: 0,
    luminanceShift: 0,
    hueRange: 40,
    chromaRange: 0.1,
    lightnessRange: 0.25,
    smoothness: 50,
  };
}
export function normalizePointColors(value: unknown): PointColor[] {
  if (!Array.isArray(value)) return [];
  const ids = new Set<string>();
  return value.slice(0, POINT_COLOR_LIMIT).flatMap((point) => {
    if (
      !point ||
      typeof point !== 'object' ||
      typeof point.id !== 'string' ||
      !point.id ||
      ids.has(point.id) ||
      !point.color ||
      typeof point.color !== 'object'
    )
      return [];
    const { lightness, chroma, hue } = point.color;
    if (![lightness, chroma, hue].every((value) => typeof value === 'number' && Number.isFinite(value))) return [];
    ids.add(point.id);
    const result = {
      ...point,
      ...createPointColor(
        {
          lightness: Math.max(0, Math.min(4, lightness)),
          chroma: Math.max(0, Math.min(2, chroma)),
          hue: ((hue % 360) + 360) % 360,
        },
        point.id,
      ),
    };
    for (const [key, [min, max]] of Object.entries(POINT_COLOR_RANGES)) {
      if (typeof point[key] === 'number' && Number.isFinite(point[key]))
        result[key] = Math.max(min, Math.min(max, point[key]));
    }
    return [result as PointColor];
  });
}
export function pointColorSwatch(color: PointColorSample): string {
  const radians = (color.hue * Math.PI) / 180;
  return `oklch(${color.lightness} ${color.chroma} ${radians}rad)`;
}
