import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import Switch from '../ui/Switch';
import { useSettingsStore } from '../../store/useSettingsStore';
import { readMcpControl, setMcpControl } from '../../utils/mcpControl';

export default function McpControlSettings() {
  const { t } = useTranslation();
  const enabled = useSettingsStore((state) => state.appSettings?.mcpEnabled ?? false);
  const [available, setAvailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    void readMcpControl()
      .then((status) => setAvailable(status.available))
      .catch((failure: unknown) => setError(String(failure)));
  }, []);
  if (!available) return null;
  return (
    <div data-mcp-control className="space-y-2">
      <Switch
        checked={enabled}
        disabled={busy}
        label={t('settings.general.mcpControl')}
        onChange={async (value) => {
          setBusy(true);
          setError('');
          try {
            await setMcpControl(value);
          } catch (failure) {
            setError(String(failure));
          } finally {
            setBusy(false);
          }
        }}
      />
      <p className="text-xs text-text-secondary">{t('settings.general.mcpControlDescription')}</p>
      {error && (
        <p role="alert" className="text-xs text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
