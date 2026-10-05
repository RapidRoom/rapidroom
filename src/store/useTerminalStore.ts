import { create } from 'zustand';
import type { TerminalSettings, TerminalTab } from '../components/ui/AppProperties';
import { normalizeTerminalSettings, TERMINAL_TAB_LIMIT } from '../utils/terminalSettings';
import { useSettingsStore } from './useSettingsStore';

interface TerminalState {
  hydrated: boolean;
  tabs: TerminalTab[];
  activeTab: string | null;
  errors: Record<string, string>;
  hydrate: () => void;
  addTab: (path: string) => TerminalTab;
  selectTab: (id: string) => void;
  removeTab: (id: string) => void;
  updateDirectory: (id: string, path: string) => void;
  setError: (id: string, error: string) => void;
}

export function saveTerminalSettings(update: Partial<TerminalSettings>): Promise<void> {
  const settings = useSettingsStore.getState();
  if (!settings.appSettings) return Promise.reject(new Error('Settings are still loading'));
  return settings.handleSettingsChange({
    ...settings.appSettings,
    terminalSettings: { ...normalizeTerminalSettings(settings.appSettings.terminalSettings), ...update },
  });
}

function persistTabs(): void {
  const { tabs, activeTab } = useTerminalStore.getState();
  void saveTerminalSettings({ tabs, activeTab });
}

export const useTerminalStore = create<TerminalState>((set, get) => ({
  hydrated: false,
  tabs: [],
  activeTab: null,
  errors: {},
  hydrate: () => {
    if (get().hydrated) return;
    const settings = normalizeTerminalSettings(useSettingsStore.getState().appSettings?.terminalSettings);
    set({ hydrated: true, tabs: settings.tabs, activeTab: settings.activeTab });
  },
  addTab: (path) => {
    get().hydrate();
    if (get().tabs.length >= TERMINAL_TAB_LIMIT) throw new Error('Close a terminal tab before opening another');
    const tab = {
      id: crypto.randomUUID(),
      title:
        path
          .replace(/[\\/]+$/, '')
          .split(/[\\/]/)
          .pop() || 'Terminal',
      path,
    };
    set({ tabs: [...get().tabs, tab], activeTab: tab.id });
    persistTabs();
    return tab;
  },
  selectTab: (id) => {
    if (!get().tabs.some((tab) => tab.id === id)) return;
    set({ activeTab: id });
    persistTabs();
  },
  removeTab: (id) => {
    const tabs = get().tabs.filter((tab) => tab.id !== id);
    const errors = { ...get().errors };
    delete errors[id];
    set({ tabs, errors, activeTab: get().activeTab === id ? (tabs.at(-1)?.id ?? null) : get().activeTab });
    persistTabs();
  },
  updateDirectory: (id, path) => {
    const old = get().tabs.find((tab) => tab.id === id);
    if (!old || old.path === path) return;
    set({ tabs: get().tabs.map((tab) => (tab.id === id ? { ...tab, path } : tab)) });
    persistTabs();
  },
  setError: (id, error) => set({ errors: { ...get().errors, [id]: error } }),
}));
