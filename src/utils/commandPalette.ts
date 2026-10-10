import schemaData from '../../rapidroom/adjustment-schema.json';
import type { Adjustments } from './adjustments';

interface Range {
  minimum: number | null;
  maximum: number | null;
  step: number | null;
  source: string;
}
interface SchemaParameter {
  defaultType: string;
  default: unknown;
  uiRanges?: Range[];
}
interface SchemaControl {
  adjustmentKeys: string[];
  labelExpression: string;
}
interface Schema {
  parameters: Record<string, SchemaParameter>;
  controls: SchemaControl[];
}
const schema = schemaData as unknown as Schema;

export interface PaletteParameter {
  id: string;
  title: string;
  labelKey?: string;
  section: string;
  aliases: string[];
  minimum: number;
  maximum: number;
  step: number;
  defaultValue: number;
}

const aliases: Record<string, string[]> = {
  exposure: ['brightness ev basic'],
  brightness: ['basic'],
  contrast: ['basic'],
  highlights: ['basic'],
  shadows: ['basic'],
  whites: ['basic'],
  blacks: ['basic'],
  temperature: ['temp white balance wb warmth'],
  tint: ['white balance wb'],
  vibrance: ['presence basic'],
  saturation: ['presence basic'],
  clarity: ['presence basic'],
  dehaze: ['presence basic'],
  structure: ['texture presence basic'],
  sharpness: ['sharpening detail'],
  lumaNoiseReduction: ['luminance noise reduction detail'],
  colorNoiseReduction: ['colour noise reduction detail'],
};

export const humanize = (id: string): string =>
  id
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/[._]/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase());

export const PALETTE_PARAMETERS: PaletteParameter[] = Object.entries(schema.parameters).flatMap(([id, parameter]) => {
  if (parameter.defaultType !== 'number' || typeof parameter.default !== 'number') return [];
  const range = parameter.uiRanges?.find(
    (range) => range.minimum !== null && range.maximum !== null && range.step !== null,
  );
  if (!range || range.minimum === null || range.maximum === null || range.step === null) return [];
  const parts = id.split('.');
  const nested = parts.length > 1;
  const section =
    parts[0] === 'hsl'
      ? 'Color Mixer'
      : parts[0] === 'colorGrading'
        ? 'Color Grading'
        : parts[0] === 'parametricCurve'
          ? 'Tone Curve'
          : (range.source.split('/').at(-1)?.replace('.tsx', '') ?? '');
  const title = nested
    ? humanize(parts.slice(1).join(' ')).replace(
        /\b(Reds|Oranges|Yellows|Greens|Aquas|Blues|Purples|Magentas)\b/g,
        (band) => band.slice(0, -1),
      )
    : humanize(id);
  const control = schema.controls.find((control) => control.adjustmentKeys.includes(id));
  const labelKey =
    nested || id === 'lutIntensity' ? undefined : /t\(['"]([^'"]+)['"]\)/.exec(control?.labelExpression ?? '')?.[1];
  return [
    {
      id,
      title,
      labelKey,
      section,
      aliases: [...(aliases[id] ?? []), ...(parts[0] === 'hsl' ? ['hsl colour mixer color mixer'] : [])],
      minimum: range.minimum,
      maximum: range.maximum,
      step: range.step,
      defaultValue: parameter.default,
    },
  ];
});

export function getParameterValue(adjustments: Adjustments, id: string): number {
  let value: unknown = adjustments;
  for (const key of id.split('.')) {
    if (!value || typeof value !== 'object') return 0;
    value = (value as Record<string, unknown>)[key];
  }
  return typeof value === 'number' ? value : 0;
}

export function setParameterValue(adjustments: Adjustments, parameter: PaletteParameter, value: number): Adjustments {
  if (!Number.isFinite(value)) return adjustments;
  const next = Math.min(parameter.maximum, Math.max(parameter.minimum, value));
  if (getParameterValue(adjustments, parameter.id) === next) return adjustments;
  const result = { ...adjustments };
  let target = result as unknown as Record<string, unknown>;
  const parts = parameter.id.split('.');
  for (const key of parts.slice(0, -1)) {
    const child = target[key];
    if (!child || typeof child !== 'object' || Array.isArray(child)) return adjustments;
    const copy = { ...(child as Record<string, unknown>) };
    target[key] = copy;
    target = copy;
  }
  target[parts.at(-1)!] = next;
  return result;
}

export function searchScore(query: string, title: string, terms: string[] = []): number {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return 1;
  const name = title.toLocaleLowerCase();
  if (name === needle) return 100;
  const haystack = [name, ...terms.map((term) => term.toLocaleLowerCase())].join(' ');
  if (haystack.split(/\s+/).includes(needle)) return 80;
  if (haystack.split(/\s+/).some((word) => word.startsWith(needle))) return 60;
  if (haystack.includes(needle)) return 40;
  let at = 0;
  for (const letter of haystack) if (needle[at] === letter) at++;
  return at === needle.length ? 10 : 0;
}

export function parseTypedValue(query: string): { name: string; value: number; relative: boolean } | null {
  const match = /^(.*?)\s+(x[+-])?([+-]?(?:\d+(?:\.\d*)?|\.\d+))\s*$/i.exec(query.trim());
  if (!match || !match[1]) return null;
  const value = Number(match[3]) * (match[2]?.toLowerCase() === 'x-' ? -1 : 1);
  return Number.isFinite(value) ? { name: match[1].trim(), value, relative: !!match[2] } : null;
}
