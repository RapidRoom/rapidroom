import { describe, expect, it } from 'vitest';
import { INITIAL_ADJUSTMENTS, createResetAdjustmentsForImage } from './adjustments';
import {
  PALETTE_PARAMETERS,
  getParameterValue,
  parseTypedValue,
  searchScore,
  setParameterValue,
} from './commandPalette';
import { executeAppAction, isAppActionAvailable, registerAppActions } from './appActions';

it('exposes every Basic control with the shared native MCP range', () => {
  for (const id of ['exposure', 'brightness', 'contrast', 'highlights', 'shadows', 'whites', 'blacks']) {
    const entry = PALETTE_PARAMETERS.find((entry) => entry.id === id);
    expect(entry).toBeDefined();
    expect(entry!.minimum).toBeLessThan(0);
    expect(entry!.maximum).toBeGreaterThan(0);
  }
  expect(PALETTE_PARAMETERS.find((entry) => entry.id === 'exposure')).toMatchObject({
    minimum: -5,
    maximum: 5,
    step: 0.01,
  });
});

it('finds controls by Lightroom aliases and fuzzy spelling', () => {
  for (const [query, id] of [
    ['presence', 'clarity'],
    ['texture', 'structure'],
    ['color mixer', 'hsl.oranges.hue'],
    ['hsl', 'hsl.blues.saturation'],
  ]) {
    const entry = PALETTE_PARAMETERS.find((entry) => entry.id === id)!;
    expect(searchScore(query, entry.title, [...entry.aliases, entry.section])).toBeGreaterThan(0);
  }
  expect(searchScore('expsre', 'Exposure')).toBeGreaterThan(0);
  expect(searchScore('Exposure', 'Exposure')).toBeGreaterThan(searchScore('Exposure', 'Reset Exposure'));
});

it('nested changes preserve the original and sibling channels', () => {
  const entry = PALETTE_PARAMETERS.find((entry) => entry.id === 'hsl.oranges.saturation')!;
  const edited = setParameterValue(INITIAL_ADJUSTMENTS, entry, 30);
  expect(getParameterValue(edited, entry.id)).toBe(30);
  expect(getParameterValue(INITIAL_ADJUSTMENTS, entry.id)).toBe(0);
  expect(edited.hsl.blues).toBe(INITIAL_ADJUSTMENTS.hsl.blues);
  expect(edited.hsl.oranges.hue).toBe(0);
  expect(setParameterValue(edited, entry, NaN)).toBe(edited);
  expect(getParameterValue(setParameterValue(edited, entry, 10000), entry.id)).toBe(entry.maximum);
});

describe('typed values', () => {
  it.each([
    ['exposure 0.7', { name: 'exposure', value: 0.7, relative: false }],
    ['Orange saturation -12', { name: 'Orange saturation', value: -12, relative: false }],
    ['exposure x+0.3', { name: 'exposure', value: 0.3, relative: true }],
    ['exposure x-0.3', { name: 'exposure', value: -0.3, relative: true }],
  ])('parses %s', (query, result) => {
    expect(parseTypedValue(query)).toEqual(result);
  });
  it.each(['exposure NaN', 'exposure Infinity', 'temperature 5600k', 'crop 4:3', '0.7'])(
    'rejects unsupported numeric input %s',
    (query) => {
      expect(parseTypedValue(query)).toBeNull();
    },
  );
});

it('application actions share guards and dispose safely across re-registration', () => {
  let enabled = false;
  let count = 0;
  const dispose = registerAppActions({
    undo: {
      available: () => enabled,
      execute: () => {
        count++;
      },
    },
  });
  expect(isAppActionAvailable('undo')).toBe(false);
  expect(executeAppAction('undo')).toBe(false);
  enabled = true;
  expect(executeAppAction('undo')).toBe(true);
  expect(count).toBe(1);
  const disposeNext = registerAppActions({
    redo: {
      available: () => true,
      execute: () => {
        count++;
      },
    },
  });
  dispose();
  expect(executeAppAction('redo')).toBe(true);
  expect(executeAppAction('missing')).toBe(false);
  disposeNext();
  expect(executeAppAction('redo')).toBe(false);
});

it('reset uses current-photo geometry and does not reuse mutable default arrays', () => {
  const reset = createResetAdjustmentsForImage({ width: 6000, height: 4000 });
  expect(reset.aspectRatio).toBe(1.5);
  expect(reset.exposure).toBe(0);
  expect(reset.aiPatches).toEqual([]);
  expect(reset.aiPatches).not.toBe(INITIAL_ADJUSTMENTS.aiPatches);
  expect(createResetAdjustmentsForImage(null).aspectRatio).toBe(INITIAL_ADJUSTMENTS.aspectRatio);
});
