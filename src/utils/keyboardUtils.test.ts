import { describe, expect, it } from 'vitest';
import { KEYBIND_DEFINITIONS, getDefaultCombo } from './keyboardUtils';

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
