import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Settings, X, ChevronDown } from 'lucide-react';
import { toast } from 'react-toastify';
import { useShallow } from 'zustand/react/shallow';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { useTerminalStore, saveTerminalSettings } from '../../store/useTerminalStore';
import { closeTerminal, mountTerminal } from '../../utils/terminalRuntime';
import { normalizeTerminalSettings } from '../../utils/terminalSettings';
import { useUIStore } from '../../store/useUIStore';
import { Panel, type PanelRegion } from '../ui/AppProperties';
import PanelSwitcher from './PanelSwitcher';
import AgentLauncher from './AgentLauncher';

export default function TerminalPanel() {
  const { t } = useTranslation();
  const host = useRef<HTMLDivElement>(null);
  const [showPreferences, setShowPreferences] = useState(false);
  const settings = useSettingsStore((state) => state.appSettings?.terminalSettings);
  const preferences = normalizeTerminalSettings(settings);
  const { tabs, activeTab, errors, hydrate, addTab, selectTab } = useTerminalStore(
    useShallow((state) => ({
      tabs: state.tabs,
      activeTab: state.activeTab,
      errors: state.errors,
      hydrate: state.hydrate,
      addTab: state.addTab,
      selectTab: state.selectTab,
    })),
  );
  const region = useUIStore(
    (state) =>
      (Object.keys(state.panelLayout) as PanelRegion[]).find((key) =>
        state.panelLayout[key].includes(Panel.Terminal),
      ) ?? 'bottom',
  );
  const collapse = () =>
    useUIStore.getState().setUI((state) =>
      region === 'bottom'
        ? { activePanels: { ...state.activePanels, bottom: null } }
        : {
            uiVisibility: { ...state.uiVisibility, [region.startsWith('left') ? 'leftPanel' : 'rightPanel']: false },
          },
    );
  const path = useLibraryStore((state) => state.currentFolderPath ?? state.rootPaths[0]);
  const active = tabs.find((tab) => tab.id === activeTab);

  useEffect(hydrate, [hydrate]);
  useEffect(() => {
    if (active && host.current) return mountTerminal(active, host.current);
  }, [active?.id, preferences.fontSize, preferences.fontFamily]);

  const add = () => {
    if (!path) return;
    try {
      addTab(path);
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <section
      data-terminal-panel
      className="@container/terminal flex flex-col h-full min-h-0 w-full bg-bg-secondary text-text-primary rounded-lg overflow-hidden"
    >
      <div data-terminal-header className="flex items-center gap-1 h-8 px-1 border-b border-border-color shrink-0">
        {region === 'bottom' && <PanelSwitcher region="bottom" side="bottom" placement="top" compact />}
        <div role="tablist" aria-label={t('terminal.tabs')} className="flex flex-1 min-w-0 overflow-x-auto">
          {tabs.map((tab) => (
            <div key={tab.id} className="group/tab flex shrink-0 items-center">
              <button
                role="tab"
                aria-selected={tab.id === activeTab}
                title={tab.path}
                className={`max-w-56 truncate px-1 h-6 text-xs rounded ${tab.id === activeTab ? 'bg-surface' : ''}`}
                onClick={() => selectTab(tab.id)}
              >
                {tab.title}
              </button>
              <button
                aria-label={t('terminal.closeTab', { title: tab.title })}
                className="p-1 opacity-0 group-hover/tab:opacity-100 focus:opacity-100"
                onClick={() => {
                  void closeTerminal(tab.id).catch((error) => toast.error(String(error)));
                }}
              >
                <X size={14} />
              </button>
            </div>
          ))}
        </div>
        <button
          aria-label={t('terminal.newTab')}
          disabled={!path || tabs.length >= 16}
          onClick={add}
          className="p-1 shrink-0 disabled:opacity-40"
        >
          <Plus size={16} />
        </button>
        <AgentLauncher compact showDestination={false} />
        <button
          aria-label={t('terminal.preferences')}
          aria-expanded={showPreferences}
          onClick={() => setShowPreferences((value) => !value)}
          className="p-1"
        >
          <Settings size={16} />
        </button>
        <button
          aria-label={t(region === 'bottom' ? 'terminal.collapseDock' : 'terminal.collapsePanel')}
          data-terminal-collapse
          className="p-1 shrink-0"
          onClick={collapse}
        >
          <ChevronDown size={16} />
        </button>
      </div>
      {showPreferences && (
        <div className="p-2 text-sm flex flex-wrap gap-3 border-b border-border-color shrink-0">
          <label>
            {t('terminal.fontSize')}{' '}
            <input
              aria-label={t('terminal.fontSize')}
              className="w-16 bg-surface p-1"
              type="number"
              min={10}
              max={32}
              value={preferences.fontSize}
              onChange={(event) => {
                void saveTerminalSettings({ fontSize: Math.min(32, Math.max(10, Number(event.target.value) || 14)) });
              }}
            />
          </label>
          <label>
            {t('terminal.fontFamily')}{' '}
            <input
              aria-label={t('terminal.fontFamily')}
              className="appearance-none bg-surface text-text-primary border border-border-color rounded p-1 cursor-pointer"
              value={preferences.fontFamily}
              onChange={(event) => {
                void saveTerminalSettings({ fontFamily: event.target.value.slice(0, 200) });
              }}
            />
          </label>
          <label>
            {t('terminal.shell')}{' '}
            <input
              aria-label={t('terminal.shell')}
              placeholder={t('terminal.defaultShell')}
              className="appearance-none bg-surface text-text-primary border border-border-color rounded p-1 cursor-pointer"
              value={preferences.shell}
              onChange={(event) => {
                void saveTerminalSettings({ shell: event.target.value });
              }}
            />
          </label>
          <label>
            {t('terminal.startIn')}{' '}
            <select
              aria-label={t('terminal.startIn')}
              className="appearance-none bg-surface text-text-primary border border-border-color rounded p-1 cursor-pointer"
              value={preferences.startIn}
              onChange={(event) => {
                void saveTerminalSettings({ startIn: event.target.value === 'external' ? 'external' : 'built-in' });
              }}
            >
              <option value="built-in">{t('terminal.builtIn')}</option>
              <option value="external">{t('terminal.external')}</option>
            </select>
          </label>
          <p className="w-full text-text-secondary">{t('terminal.shellNote')}</p>
        </div>
      )}
      {active && errors[active.id] && (
        <p role="alert" className="p-2 text-sm text-red-400 shrink-0">
          {errors[active.id]}
        </p>
      )}
      {active ? (
        <div ref={host} data-terminal-screen className="flex-1 min-h-0 p-1 bg-[#181818]" />
      ) : (
        <div className="flex-1 flex items-center justify-center p-3">
          <button
            data-terminal-open
            onClick={add}
            disabled={!path}
            className="bg-surface rounded px-3 py-2 disabled:opacity-40"
          >
            {path ? t('terminal.newTab') : t('terminal.chooseFolder')}
          </button>
        </div>
      )}
    </section>
  );
}
