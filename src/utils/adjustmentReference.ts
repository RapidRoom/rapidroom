// Builds the adjustment and mask reference that ships with the RapidRoom skill
// (rapidroom/plugin/skills/rapidroom/references/adjustments.md). Keys and defaults come from
// INITIAL_ADJUSTMENTS, slider ranges from the slider props in the editor's components, mask
// types and defaults from createSubMask. Only the notes are written by hand.
// adjustmentReference.test.ts fails when the committed file differs from what this builds.
import {
  ADJUSTMENT_GROUPS,
  createRelightLight,
  INITIAL_ADJUSTMENTS,
  INITIAL_MASK_ADJUSTMENTS,
  INITIAL_MASK_CONTAINER,
} from './adjustments';
import { ADJUSTMENT_NOTES, MASK_TYPE_NOTES } from './adjustmentReferenceNotes';
import { createSubMask } from './maskUtils';
import { Mask, SubMaskMode } from '../components/panel/right/Masks';
import type { Curves, ParametricCurve } from './adjustments';
import type { ImageDimensions } from '../hooks/useImageRenderSize';
import { RELATIVE_RANGE } from './whiteBalance';

interface MaskPanelConfig {
  parameters?: Array<{
    key: string;
    min: number;
    max: number;
    step: number;
    defaultValue: number;
    multiplier?: number;
  }>;
}

const HSL_BANDS = Object.keys(INITIAL_ADJUSTMENTS.hsl);
const CURVE_CHANNELS = Object.keys(INITIAL_ADJUSTMENTS.curves);
const CALIBRATION_PRIMARIES = ['red', 'green', 'blue'];

export interface SliderRange {
  min: number;
  max: number;
  step: number;
  maskMin?: number;
}

// A slider's `value` prop, mapped to the adjustment path it shows. `<band>`, `<channel>` and
// `<primary>` stand for each HSL band, curve channel and calibration primary.
const VALUE_PATHS: Array<[RegExp, (m: RegExpMatchArray) => string]> = [
  [/^adjustments\.([\wé]+)$/, (m) => m[1]],
  [/^activeLight\.(\w+)$/, (m) => `relightLights.<light>.${m[1]}`],
  [/^displayedWhiteBalance\.(temperature|tint)$/, (m) => m[1]],
  [/^colorGrading\.(\w+)$/, (m) => `colorGrading.${m[1]}`],
  [/^colorCalibration\.(\w+)$/, (m) => `colorCalibration.${m[1]}`],
  [/^currentHsl\.(\w+)$/, (m) => `hsl.<band>.${m[1]}`],
  [/^hsl\[color\]\[property\]$/, () => 'hsl.<band>.<property>'],
  [/^currentValues\.(hue|saturation)$/, (m) => `colorCalibration.<primary>${m[1] === 'hue' ? 'Hue' : 'Saturation'}`],
  [/^activeParametricSettings\.(\w+)$/, (m) => `parametricCurve.<channel>.${m[1]}`],
  [/^exposureValue$/, () => 'exposure'],
  [/^displayRotation$/, () => 'rotation'],
  [/^lutIntensity$/, () => 'lutIntensity'],
];

const readBalanced = (source: string, start: number): number => {
  let depth = 0;
  let quote: string | null = null;
  for (let i = start; i < source.length; i++) {
    const c = source[i];
    if (quote) {
      if (c === '\\') i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'" || c === '`') quote = c;
    else if (c === '{') depth++;
    else if (c === '}' && --depth === 0) return i + 1;
  }
  throw new Error(`Unbalanced braces at ${start}`);
};

// The props of every <Slider .../> element, as raw expression text.
export const parseSliderProps = (source: string): Array<Record<string, string>> => {
  const sliders: Array<Record<string, string>> = [];
  const tag = /<Slider\s/g;
  let match;
  while ((match = tag.exec(source))) {
    const props: Record<string, string> = {};
    let i = match.index + match[0].length;
    while (i < source.length) {
      while (/\s/.test(source[i])) i++;
      if (source.startsWith('/>', i)) break;
      const name = /^[\w-]+/.exec(source.slice(i));
      if (!name) throw new Error(`Unexpected "${source.slice(i, i + 20)}" in <Slider>`);
      i += name[0].length;
      if (source[i] !== '=') {
        props[name[0]] = 'true';
        continue;
      }
      i++;
      if (source[i] === '{') {
        const end = readBalanced(source, i);
        props[name[0]] = source.slice(i + 1, end - 1).trim();
        i = end;
      } else {
        const end = source.indexOf(source[i], i + 1);
        props[name[0]] = source.slice(i + 1, end);
        i = end + 1;
      }
    }
    sliders.push(props);
  }
  return sliders;
};

const parseBound = (expression: string): { value: number; maskValue?: number } => {
  const forMask = /^isForMask\s*\?\s*(-?[\d.]+)\s*:\s*(-?[\d.]+)$/.exec(expression);
  if (forMask) return { value: Number(forMask[2]), maskValue: Number(forMask[1]) };
  // These sliders display both modes; relative scalar keys keep their relative range.
  const relative = /^kelvinAsShot\s*\?[^:]+:\s*(.+)$/.exec(expression)?.[1] ?? expression;
  const value =
    relative === 'tintRange' || relative === 'RELATIVE_RANGE'
      ? RELATIVE_RANGE
      : relative === '-RELATIVE_RANGE' || relative === '-tintRange'
        ? -RELATIVE_RANGE
        : Number(relative);
  if (Number.isNaN(value)) throw new Error(`Can't read slider bound "${expression}"`);
  return { value };
};

export const sliderPath = (valueExpression: string): string | null => {
  const bare = valueExpression
    .replace(/\s*(\|\||\?\?)\s*[-\d.]+$/, '')
    .replace(/^\((.*)\)$/, '$1')
    .trim();
  for (const [pattern, toPath] of VALUE_PATHS) {
    const m = bare.match(pattern);
    if (m) return toPath(m);
  }
  return null;
};

// Slider ranges by adjustment path. Sliders whose value can't be mapped are returned in
// `unmapped`, so a new slider can't go unnoticed.
export const collectSliderRanges = (sources: Record<string, string>) => {
  const ranges: Record<string, SliderRange> = {};
  const unmapped: string[] = [];
  for (const [file, source] of Object.entries(sources)) {
    for (const props of parseSliderProps(source)) {
      const path = props.value ? sliderPath(props.value) : null;
      if (!path) {
        unmapped.push(`${file}: value={${props.value}}`);
        continue;
      }
      const min = parseBound(props.min);
      const range: SliderRange = {
        min: min.value,
        max: parseBound(props.max).value,
        step: parseBound(props.step ?? '1').value,
      };
      if (min.maskValue !== undefined) range.maskMin = min.maskValue;
      const paths =
        path === 'hsl.<band>.<property>'
          ? ['hue', 'saturation', 'luminance'].map((property) => `hsl.<band>.${property}`)
          : [path];
      for (const key of paths) {
        const existing = ranges[key];
        if (existing && JSON.stringify(existing) !== JSON.stringify(range)) {
          throw new Error(`Two sliders for ${key} disagree: ${JSON.stringify(existing)} vs ${JSON.stringify(range)}`);
        }
        ranges[key] = range;
      }
    }
  }
  return { ranges, unmapped };
};

// The GUI parameters of each mask type, from SUB_MASK_CONFIG in MasksPanel.tsx.
export const parseMaskConfig = (source: string): Record<string, MaskPanelConfig> => {
  const start = source.indexOf('const SUB_MASK_CONFIG');
  if (start < 0) throw new Error('SUB_MASK_CONFIG not found in MasksPanel.tsx');
  const open = source.indexOf('{', source.indexOf('=', start));
  const literal = source
    .slice(open, readBalanced(source, open))
    .replace(/\[Mask\.(\w+)\]/g, (_, member) => JSON.stringify(Mask[member as keyof typeof Mask]));
  return new Function(`return (${literal});`)();
};

interface Row {
  path: string;
  type: string;
  defaultValue: unknown;
  range?: SliderRange;
  inMasks: boolean;
}

const typeOf = (value: unknown, path: string): string => {
  if (value === null) {
    if (/^(lutPath|lutName|lutData|lensMaker|lensModel|lensBlurDepthMap|relightNormalMap|fogDepthMap)$/.test(path))
      return 'string | null';
    if (path === 'aspectRatio') return 'number | null';
    return 'object | null';
  }
  if (Array.isArray(value)) return 'array';
  return typeof value;
};

const rowsFor = (path: string, value: unknown, inMasks: boolean): Row[] => {
  const row = (p: string, v: unknown): Row => ({ path: p, type: typeOf(v, p), defaultValue: v, inMasks });
  if (path === 'relightLights') {
    const light = { ...createRelightLight(0.5, 0.5), id: '<uuid>' };
    return [row(path, value), ...Object.entries(light).map(([key, item]) => row(`relightLights.<light>.${key}`, item))];
  }
  if (path === 'hsl') {
    return ['hue', 'saturation', 'luminance'].map((k) => row(`hsl.<band>.${k}`, 0));
  }
  if (path === 'parametricCurve') {
    return Object.entries((value as ParametricCurve).luma).map(([k, v]) => row(`parametricCurve.<channel>.${k}`, v));
  }
  if (path === 'curves' || path === 'pointCurves') {
    return [row(`${path}.<channel>`, (value as Curves).luma)];
  }
  if (path === 'colorCalibration') {
    const rows = [row('colorCalibration.shadowsTint', 0)];
    return rows.concat(['Hue', 'Saturation'].map((k) => row(`colorCalibration.<primary>${k}`, 0)));
  }
  if (path === 'colorGrading') {
    return Object.entries(value as object).flatMap(([k, v]) =>
      typeof v === 'number'
        ? [row(`colorGrading.${k}`, v)]
        : Object.entries(v).map(([c, d]) => row(`colorGrading.${k}.${c}`, d)),
    );
  }
  if (value && typeof value === 'object' && !Array.isArray(value) && path !== 'crop') {
    return Object.entries(value).map(([k, v]) => row(`${path}.${k}`, v));
  }
  return [row(path, value)];
};

export const referenceRows = (ranges: Record<string, SliderRange>): Row[] =>
  Object.entries(INITIAL_ADJUSTMENTS).flatMap(([key, value]) =>
    rowsFor(key, value, key in INITIAL_MASK_ADJUSTMENTS).map((row) => ({ ...row, range: ranges[row.path] })),
  );

const formatNumber = (n: number) => String(Number(n.toFixed(4)));

const formatDefault = (value: unknown): string => {
  if (typeof value === 'number') return formatNumber(value);
  const json = JSON.stringify(value);
  return json.length > 40 ? `${json.slice(0, 37)}…` : json;
};

const formatRange = (row: Row): string => {
  if (!row.range) return '';
  const { min, max, maskMin } = row.range;
  const text = `${formatNumber(min)} to ${formatNumber(max)}`;
  return maskMin === undefined ? text : `${text} (masks: ${formatNumber(maskMin)} to ${formatNumber(max)})`;
};

const cell = (text: string) => text.replace(/\|/g, '\\|').replace(/\n/g, ' ');

type Messages = { [key: string]: string | Messages };

const lookup = (messages: Messages, key: string): string => {
  const found = key
    .split('.')
    .reduce<string | Messages | undefined>(
      (node, part) => (typeof node === 'object' ? node[part] : undefined),
      messages,
    );
  return typeof found === 'string' ? found : key;
};

const table = (rows: Row[]): string[] => [
  '| Key | Type | Default | Slider range | Step | Masks | Notes |',
  '| --- | --- | --- | --- | --- | --- | --- |',
  ...rows.map((row) =>
    [
      '',
      `\`${row.path}\``,
      cell(row.type),
      `\`${cell(formatDefault(row.defaultValue))}\``,
      formatRange(row),
      row.range ? formatNumber(row.range.step) : '',
      row.inMasks ? 'yes' : '',
      cell(ADJUSTMENT_NOTES[row.path] ?? ''),
      '',
    ]
      .join(' | ')
      .trim(),
  ),
];

const maskSection = (maskConfig: Record<string, MaskPanelConfig>): string[] => {
  const lines = [
    '## Masks',
    '',
    'Masks live in the `masks` array. Each entry is a mask container with its own adjustments (the keys marked **Masks** above). They are added to the global values where the mask applies, so a mask `exposure` of −0.5 darkens that area by about 0.6 EV on top of the global edit.',
    '',
    '- Sub-masks are combined in order: each is inverted if set and scaled by its opacity, then `additive` takes the maximum, `subtractive` subtracts and `intersect` takes the minimum. The container’s `invert` and `opacity` apply last.',
    '- Positions and sizes are in full-resolution pixels of the image after rotation and flips, before the crop.',
    '- Over MCP today the `masks` array is accepted but not checked item by item, and writing it replaces every mask. Read the current masks first and keep the ones you aren’t changing. Mask tools, including AI masks, are [coming](tools.md#coming) (#120). Writing masks this way is untested: check the preview, and keep the user’s masks safe.',
    '',
    'A new container looks like this (`id` is a UUID):',
    '',
    '```json',
    JSON.stringify({ ...INITIAL_MASK_CONTAINER, adjustments: '{ …mask adjustments… }' }, null, 2),
    '```',
    '',
    `Sub-mask fields: \`id\`, \`type\`, \`mode\` (${Object.values(SubMaskMode)
      .map((m) => `\`${m}\``)
      .join(', ')}), \`invert\`, \`opacity\` (0 to 100), \`visible\`, \`parameters\`.`,
    '',
    'Default parameters below are for a 6000 × 4000 image; positions and radii are in image pixels.',
    '',
    '| Type | Default parameters | Sliders in the mask panel | Notes |',
    '| --- | --- | --- | --- |',
  ];
  for (const type of Object.values(Mask)) {
    const { parameters } = createSubMask(type, { width: 6000, height: 4000 } as ImageDimensions);
    const sliders = (maskConfig[type]?.parameters ?? [])
      .map(
        (p) =>
          `\`${p.key}\` ${p.min} to ${p.max}${p.multiplier ? ` (stored ÷ ${p.multiplier})` : ''}, default ${p.defaultValue}`,
      )
      .join('; ');
    lines.push(
      `| \`${type}\` | \`${cell(JSON.stringify(parameters ?? {}))}\` | ${cell(sliders)} | ${cell(MASK_TYPE_NOTES[type] ?? '')} |`,
    );
  }
  return lines;
};

export const buildAdjustmentReference = (inputs: {
  sliderSources: Record<string, string>;
  maskPanelSource: string;
  enMessages: Messages;
}): { markdown: string; unmapped: string[]; rows: Row[] } => {
  const { ranges, unmapped } = collectSliderRanges(inputs.sliderSources);
  const rows = referenceRows(ranges);
  const byKey = new Map<string, Row[]>();
  for (const row of rows) {
    const key = row.path.split('.')[0];
    byKey.set(key, [...(byKey.get(key) ?? []), row]);
  }

  const lines = [
    '<!-- Generated by src/utils/adjustmentReference.ts from the RapidRoom source. Do not edit by hand: change the code or the notes in src/utils/adjustmentReferenceNotes.ts, then run `npx vitest run src/utils/adjustmentReference.test.ts -u`. -->',
    '',
    '# Adjustment and mask reference',
    '',
    'Every key of an edit, as `get_image_state` returns it and `update_adjustments` takes it. Keys, defaults and slider ranges are generated from the RapidRoom source (`INITIAL_ADJUSTMENTS` in `src/utils/adjustments.ts` and the editor sliders), so they match this version of RapidRoom.',
    '',
    '- **Slider range** is what the editor slider shows. The MCP server validates against its own schema (`tools/list`), which is sometimes wider; stay inside the slider range so the user can see and adjust your value. Blank means there is no slider for it.',
    `- \`<band>\` is one of ${HSL_BANDS.map((b) => `\`${b}\``).join(', ')}. \`<channel>\` is one of ${CURVE_CHANNELS.map((c) => `\`${c}\``).join(', ')}. \`<primary>\` is one of ${CALIBRATION_PRIMARIES.map((p) => `\`${p}\``).join(', ')} (for example \`colorCalibration.redHue\`).`,
    '- `relightLights.<light>` describes one item in the `relightLights` array; its defaults apply to a newly placed light, while the array itself defaults to empty. Preserve other lights when replacing the array.',
    '- **Masks: yes** means a mask container has the key too, under `masks[i].adjustments`.',
    '- `update_adjustments` merges nested objects, so `{"hsl": {"blues": {"saturation": -20}}}` changes one value.',
    '',
  ];

  const seen = new Set<string>();
  for (const [section, groups] of Object.entries(ADJUSTMENT_GROUPS)) {
    if (section === 'masks') continue;
    lines.push(`## ${section[0].toUpperCase()}${section.slice(1)}`, '');
    for (const group of groups) {
      const groupRows = group.keys.flatMap((key) => {
        seen.add(key);
        return byKey.get(key) ?? [];
      });
      lines.push(`### ${lookup(inputs.enMessages, group.label)}`, '', ...table(groupRows), '');
    }
  }
  const rest = [...byKey.keys()].filter((key) => !seen.has(key));
  lines.push(
    '## Other keys',
    '',
    'Not part of Copy & Paste; most are set by the editor itself.',
    '',
    ...table(rest.flatMap((key) => byKey.get(key) ?? [])),
    '',
  );
  lines.push(...maskSection(parseMaskConfig(inputs.maskPanelSource)), '');

  return { markdown: lines.join('\n'), unmapped, rows };
};
