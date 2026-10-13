import { describe, expect, it } from 'vitest';
import { KEYBIND_DEFINITIONS, getDefaultCombo, normalizeCombo } from './keyboardUtils';

describe('KEYBIND_DEFINITIONS', () => {
  it('has no two actions sharing a default shortcut', () => {
    const seen = new Map<string, string>();
    for (const def of KEYBIND_DEFINITIONS) {
      const combo = def.defaultCombo.join('+');
      expect(seen.get(combo), `${def.action} reuses ${combo}`).toBeUndefined();
      seen.set(combo, def.action);
    }
  });

  it('binds Lights Out to L and Shift+L', () => {
    const combo = (action: string) => KEYBIND_DEFINITIONS.find((def) => def.action === action)?.defaultCombo;
    expect(combo('cycle_lights_out')).toEqual(['KeyL']);
    expect(combo('cycle_lights_out_reverse')).toEqual(['shift', 'KeyL']);
  });
});

const defaultFor = (action: string, os?: string) =>
  getDefaultCombo(
    KEYBIND_DEFINITIONS.find((def) => def.action === action)!,
    os,
  );

describe('flag shortcuts', () => {
  it('match Lightroom: P picks, X rejects, U removes the flag', () => {
    for (const os of ['windows', 'macos', 'linux']) {
      expect(defaultFor('flag_pick', os)).toEqual(['KeyP']);
      expect(defaultFor('flag_reject', os)).toEqual(['KeyX']);
      expect(defaultFor('unflag', os)).toEqual(['KeyU']);
    }
  });

  it('leave no two default shortcuts on the same keys', () => {
    for (const os of ['windows', 'macos', 'linux']) {
      const seen = new Map<string, string>();
      for (const def of KEYBIND_DEFINITIONS) {
        const combo = getDefaultCombo(def, os).join('+');
        if (!combo) continue;
        expect(seen.get(combo), `${os}: ${def.action} and ${seen.get(combo)} both use ${combo}`).toBeUndefined();
        seen.set(combo, def.action);
      }
    }
  });
});

const keyboardEvent = (parts: Partial<KeyboardEvent>): KeyboardEvent =>
  ({ ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, isComposing: false, ...parts }) as KeyboardEvent;

describe('layout-aware punctuation shortcuts', () => {
  it('matches AZERTY Ctrl+- to the default Zoom out binding', () => {
    const event = keyboardEvent({ code: 'Digit6', key: '-', ctrlKey: true });
    expect(normalizeCombo(event, 'linux')).toEqual(defaultFor('zoom_out', 'linux'));
    expect(normalizeCombo(event, 'windows')).toEqual(defaultFor('zoom_out', 'windows'));
  });

  it('records and matches relocated punctuation without changing modifiers', () => {
    for (const [key, expected] of [
      [';', 'Semicolon'],
      [',', 'Comma'],
      ['[', 'BracketLeft'],
      [']', 'BracketRight'],
      ['/', 'Slash'],
      ["'", 'Quote'],
      ['.', 'Period'],
      ['=', 'Equal'],
      ['`', 'Backquote'],
      ['\\', 'Backslash'],
    ]) {
      expect(normalizeCombo(keyboardEvent({ code: 'KeyM', key, ctrlKey: true, shiftKey: true, altKey: true }))).toEqual(
        ['ctrl', 'shift', 'alt', expected],
      );
    }
    expect(normalizeCombo(keyboardEvent({ code: 'Semicolon', key: 'm' }))).toEqual(['KeyM']);
    expect(normalizeCombo(keyboardEvent({ code: 'KeyW', key: 'z', ctrlKey: true }))).toEqual(['ctrl', 'KeyZ']);
  });

  it('preserves QWERTY, number-row ratings, numpad and macOS delete mappings', () => {
    expect(normalizeCombo(keyboardEvent({ code: 'Minus', key: '-', ctrlKey: true }))).toEqual(['ctrl', 'Minus']);
    expect(normalizeCombo(keyboardEvent({ code: 'Digit1', key: '&' }))).toEqual(['Digit1']);
    expect(normalizeCombo(keyboardEvent({ code: 'Numpad1', key: '1' }))).toEqual(['Digit1']);
    expect(normalizeCombo(keyboardEvent({ code: 'NumpadAdd', key: '+' }))).toEqual(['Equal']);
    expect(normalizeCombo(keyboardEvent({ code: 'NumpadSubtract', key: '-' }))).toEqual(['Minus']);
    expect(normalizeCombo(keyboardEvent({ code: 'NumpadDivide', key: '/' }))).toEqual(['NumpadDivide']);
    expect(normalizeCombo(keyboardEvent({ code: 'Backspace', key: 'Backspace', metaKey: true }), 'macos')).toEqual([
      'Delete',
    ]);
  });

  it('ignores dead and composition events without falling back to their physical keys', () => {
    expect(normalizeCombo(keyboardEvent({ code: 'KeyP', key: 'Dead' }))).toEqual([]);
    expect(normalizeCombo(keyboardEvent({ code: 'KeyP', key: 'Process' }))).toEqual([]);
    expect(normalizeCombo(keyboardEvent({ code: 'KeyP', key: 'p', isComposing: true }))).toEqual([]);
  });
});
