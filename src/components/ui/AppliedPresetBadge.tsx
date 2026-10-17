import { Check } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAppliedPreset } from '../../hooks/useAppliedPreset';

export default function AppliedPresetBadge() {
  const record = useAppliedPreset();
  const { t } = useTranslation();
  if (!record) return null;
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
