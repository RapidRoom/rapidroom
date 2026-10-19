import { Check } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAppliedPreset } from '../../hooks/useAppliedPreset';

export default function AppliedPresetBadge() {
  const record = useAppliedPreset();
  const { t } = useTranslation();
  if (!record) return null;
  if (record.stack?.entries.length) {
    const label = record.stack.entries
      .map(
        (entry, index) =>
          `${index + 1} ${entry.name}${entry.bakedIn ? ` (${t('editor.presets.bakedIn')})` : ` ${entry.amount}`}`,
      )
      .join(' · ');
    return (
      <span
        data-applied-preset={record.id}
        data-applied-style-count={record.stack.entries.length}
        title={`${t('editor.presets.appliedFolder')}: ${label}`}
        className="ml-2 min-w-0 max-w-80 truncate text-primary text-xs"
      >
        {t('editor.presets.appliedFolder')}: {label}
      </span>
    );
  }
  return (
    <span
      data-applied-preset={record.id}
      title={`${t('editor.presets.applied')}: ${record.name}${record.amount !== 100 ? ` · ${record.amount}%` : ''}`}
      className="ml-2 flex items-center gap-1 min-w-0 max-w-40 text-primary text-xs"
    >
      <Check size={12} className="shrink-0" aria-label={t('editor.presets.applied')} />
      <span className="truncate">{record.name}</span>
      {record.amount !== 100 && <span className="shrink-0">{record.amount}%</span>}
    </span>
  );
}
