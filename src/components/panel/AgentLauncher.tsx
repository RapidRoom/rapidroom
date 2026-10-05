import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { Bot, Code2 } from 'lucide-react';
import { Invokes, Panel, type PanelRegion } from '../ui/AppProperties';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { saveTerminalSettings, useTerminalStore } from '../../store/useTerminalStore';
import { useUIStore } from '../../store/useUIStore';
import { normalizeTerminalSettings } from '../../utils/terminalSettings';
import { ensureMcpControl } from '../../utils/mcpControl';

export default function AgentLauncher({
  compact = false,
  showDestination = true,
}: {
  compact?: boolean;
  showDestination?: boolean;
}) {
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
    <div
      data-agent-launcher
      className={compact ? 'flex shrink-0 items-center gap-1 text-xs' : 'flex items-center gap-2 text-xs px-2'}
    >
      {showDestination && (
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
      )}
      <button
        data-start-agent="claude"
        aria-label={t('terminal.claude')}
        data-tooltip={t('terminal.claude')}
        disabled={!path}
        className={
          compact
            ? 'flex items-center gap-1 h-6 px-1 rounded bg-surface disabled:opacity-40'
            : 'bg-surface rounded px-2 py-1 disabled:opacity-40'
        }
        onClick={() => {
          void start('claude');
        }}
      >
        {compact && <Bot size={16} aria-hidden />}
        <span className={compact ? 'hidden @min-[480px]/terminal:inline' : undefined}>{t('terminal.claude')}</span>
      </button>
      <button
        data-start-agent="codex"
        aria-label={t('terminal.codex')}
        data-tooltip={t('terminal.codex')}
        disabled={!path}
        className={
          compact
            ? 'flex items-center gap-1 h-6 px-1 rounded bg-surface disabled:opacity-40'
            : 'bg-surface rounded px-2 py-1 disabled:opacity-40'
        }
        onClick={() => {
          void start('codex');
        }}
      >
        {compact && <Code2 size={16} aria-hidden />}
        <span className={compact ? 'hidden @min-[480px]/terminal:inline' : undefined}>{t('terminal.codex')}</span>
      </button>
    </div>
  );
}
