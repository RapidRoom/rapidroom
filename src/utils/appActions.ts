import { KEYBIND_DEFINITIONS, type KeybindDefinition } from './keyboardUtils';

export const APP_ACTION_DEFINITIONS: KeybindDefinition[] = [
  ...KEYBIND_DEFINITIONS,
  {
    action: 'reset_adjustments',
    description: 'commandPalette.resetCurrentPhoto',
    defaultCombo: [],
    section: 'editing',
  },
  { action: 'auto_adjustments', description: 'commandPalette.autoAdjustments', defaultCombo: [], section: 'editing' },
  {
    action: 'white_balance_picker',
    description: 'adjustments.color.wbPickerTooltip',
    defaultCombo: [],
    section: 'editing',
  },
  {
    action: 'color_mixer_picker',
    description: 'adjustments.color.mixerPickerTooltip',
    defaultCombo: [],
    section: 'editing',
  },
];

// Shortcuts and command-palette entries use the same guarded application handlers.
export interface AppAction {
  available(): boolean;
  execute(): void;
}

let handlers: Readonly<Record<string, AppAction>> = {};

export function registerAppActions(actions: Readonly<Record<string, AppAction>>): () => void {
  handlers = actions;
  return () => {
    if (handlers === actions) handlers = {};
  };
}

export function isAppActionAvailable(id: string): boolean {
  return handlers[id]?.available() ?? false;
}

export function executeAppAction(id: string): boolean {
  const handler = handlers[id];
  if (!handler?.available()) return false;
  handler.execute();
  return true;
}
