import { describe, expect, it } from 'vitest';
import { createLightroomWorkspace, lightroomDefaultCombo } from './lightroomWorkspace';
import { KEYBIND_DEFINITIONS, getDefaultCombo } from './keyboardUtils';
import { reconcileWorkspace } from '../store/useUIStore';
import { Panel } from '../components/ui/AppProperties';

describe('optional Lightroom workspace', () => {
  it('rearranges each supported panel once and leaves the original workspace available for restoration', () => {
    const original = reconcileWorkspace(undefined, true, true);
    const snapshot = structuredClone(original);
    const workspace = createLightroomWorkspace(original);
    expect(original).toEqual(snapshot);
    expect(workspace.leftPanelWidth).toBe(245);
    expect(workspace.rightPanelWidth).toBe(330);
    expect(workspace.activePanels.rightTop).toBe(Panel.Adjustments);
    const panels = Object.values(workspace.panelLayout).flat();
    expect(new Set(panels).size).toBe(panels.length);
    expect(new Set(panels)).toEqual(new Set(Object.values(original.panelLayout).flat()));
    expect(workspace.panelLayout.rightBottom).toContain(Panel.Metadata);
    expect(workspace.panelLayout.bottom).toEqual([Panel.Terminal]);
  });
  it('leaves unsupported optional panels out', () => {
    const original = reconcileWorkspace(undefined, false, false);
    const panels = Object.values(createLightroomWorkspace(original).panelLayout).flat();
    expect(panels).not.toContain(Panel.Terminal);
    expect(panels).not.toContain(Panel.Tethering);
  });
});
describe('optional Lightroom keymap', () => {
  it('preserves normal defaults and assigns module, editing, rating and view keys without duplicates', () => {
    const effective = new Map<string, string>();
    for (const def of KEYBIND_DEFINITIONS) {
      const normal = getDefaultCombo(def, 'linux');
      expect(lightroomDefaultCombo(def, normal, false)).toEqual(normal);
      const combo = lightroomDefaultCombo(def, normal, true);
      if (!combo.length) continue;
      const key = combo.join('+');
      expect(effective.get(key), `duplicate ${key}: ${def.action}`).toBeUndefined();
      effective.set(key, def.action);
    }
    expect(effective.get('KeyG')).toBe('open_library');
    expect(effective.get('KeyD')).toBe('open_develop');
    expect(effective.get('KeyK')).toBe('toggle_masks');
    expect(effective.get('KeyY')).toBe('toggle_before_after');
    expect(effective.get('KeyJ')).toBe('toggle_clipping');
    expect(effective.get('Backslash')).toBe('show_original');
    expect(effective.get('Digit5')).toBe('rate_5');
  });
});
