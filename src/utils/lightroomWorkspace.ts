import { Panel, type WorkspaceState } from '../components/ui/AppProperties';
import type { KeybindDefinition } from './keyboardUtils';

export function createLightroomWorkspace(current: WorkspaceState): WorkspaceState {
  const panels = new Set(Object.values(current.panelLayout).flat());
  return {
    ...current,
    leftPanelWidth: 245,
    bottomPanelHeight: 128,
    rightPanelWidth: 330,
    leftTopHeight: 450,
    rightTopHeight: 450,
    panelLayout: {
      leftTop: [Panel.FolderTree, Panel.Presets],
      leftBottom: [],
      // Develop needs the whole sidebar height; secondary tools remain tabs.
      rightTop: [
        Panel.Adjustments,
        Panel.Crop,
        Panel.Masks,
        Panel.Ai,
        Panel.Metadata,
        Panel.Export,
        ...(panels.has(Panel.Tethering) ? [Panel.Tethering] : []),
      ],
      rightBottom: [],
      bottom: panels.has(Panel.Terminal) ? [Panel.Terminal] : [],
    },
    activePanels: {
      leftTop: Panel.FolderTree,
      leftBottom: null,
      rightTop: Panel.Adjustments,
      rightBottom: null,
      bottom: null,
    },
    panelSwitcherPlacement: {
      leftTop: 'bottom',
      leftBottom: 'bottom',
      rightTop: 'top',
      rightBottom: 'bottom',
      bottom: 'top',
    },
  };
}

const lightroomCombos: Record<string, string[]> = {
  toggle_before_after: ['KeyY'],
  open_library: ['KeyG'],
  open_develop: ['KeyD'],
  open_image: ['KeyE'],
  toggle_export: ['ctrl', 'shift', 'KeyE'],
  toggle_adjustments: ['ctrl', 'shift', 'KeyD'],
  toggle_masks: ['KeyK'],
  toggle_ai: ['shift', 'KeyK'],
  cycle_zoom: ['KeyZ'],
  show_original: ['Backslash'],
  toggle_clipping: ['KeyJ'],
  toggle_reference_view: ['shift', 'KeyR'],
  color_label_red: ['Digit6'],
  color_label_yellow: ['Digit7'],
  color_label_green: ['Digit8'],
  color_label_blue: ['Digit9'],
};
export function lightroomDefaultCombo(def: KeybindDefinition, normal: string[], enabled: boolean): string[] {
  return enabled && def.action in lightroomCombos ? lightroomCombos[def.action] : normal;
}
