import { describe, expect, it } from 'vitest';
import { KEYBIND_DEFINITIONS } from './keyboardUtils';

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
