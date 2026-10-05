import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { Invokes, Panel, type PanelRegion } from '../ui/AppProperties';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { saveTerminalSettings, useTerminalStore } from '../../store/useTerminalStore';
import { useUIStore } from '../../store/useUIStore';
import { normalizeTerminalSettings } from '../../utils/terminalSettings';
import { ensureMcpControl } from '../../utils/mcpControl';

export default function AgentLauncher() {
  const { t } = useTranslation();
  const path = useLibraryStore((state) => state.currentFolderPath ?? state.rootPaths[0]);
  const saved = useSettingsStore((state) => state.appSettings?.terminalSettings);
  const preferences = normalizeTerminalSettings(saved);
  const start = async (agent: 'claude' | 'codex') => {
    if (!path) return;
    try {
      if (
        !(await ensureMcpControl({
          title: t('settings.general.mcpControl'),
          message: t('settings.general.mcpControlDescription'),
          confirm: t('terminal.enableAndStart'),
        }))
      )
        return;
      if (preferences.startIn === 'external') {
        await invoke(Invokes.LaunchTerminalAgent, { path, agent });
        return;
      }
      const terminal = useTerminalStore.getState().addTab(path);
      const ui = useUIStore.getState();
      const region =
        (Object.keys(ui.panelLayout) as PanelRegion[]).find((candidate) =>
          ui.panelLayout[candidate].includes(Panel.Terminal),
        ) ?? 'bottom';
      ui.setActivePanel(region, Panel.Terminal);
      const { writeTerminal } = await import('../../utils/terminalRuntime');
      const command = await invoke<string>(Invokes.TerminalAgentCommand, { agent });
      await writeTerminal(terminal.id, `${command}\r`);
    } catch (error) {
      toast.error(String(error));
    }
  };
  return (
    <div data-agent-launcher className="flex items-center gap-2 text-xs px-2">
      <label className="flex items-center gap-1">
        <span className="sr-only">{t('terminal.startIn')}</span>
        <select
          aria-label={t('terminal.startIn')}
          className="appearance-none bg-surface text-text-primary border border-border-color rounded px-2 py-1 cursor-pointer"
          value={preferences.startIn}
          onChange={(event) => {
            void saveTerminalSettings({ startIn: event.target.value === 'external' ? 'external' : 'built-in' });
          }}
        >
          <option value="built-in">{t('terminal.builtIn')}</option>
          <option value="external">{t('terminal.external')}</option>
        </select>
      </label>
      <button
        data-start-agent="claude"
        disabled={!path}
        className="bg-surface rounded px-2 py-1 disabled:opacity-40"
        onClick={() => {
          void start('claude');
        }}
      >
        {t('terminal.claude')}
      </button>
      <button
        data-start-agent="codex"
        disabled={!path}
        className="bg-surface rounded px-2 py-1 disabled:opacity-40"
        onClick={() => {
          void start('codex');
        }}
      >
        {t('terminal.codex')}
      </button>
    </div>
  );
}
