import { Check } from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import { useAppliedPreset } from '../../hooks/useAppliedPreset';

export default function AppliedPresetBadge({ compact = false }: { compact?: boolean }) {
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
        className={clsx(
          'ml-2 flex items-center gap-1 min-w-0 text-primary text-xs',
          compact
            ? 'shrink-[999] max-w-28 @max-[14rem]/editor-info:ml-1 @max-[14rem]/editor-info:gap-0 @max-[14rem]/editor-info:shrink-0'
            : 'max-w-80',
        )}
      >
        <Check size={12} className="shrink-0" aria-label={t('editor.presets.applied')} />
        <span className={clsx('truncate', compact && '@max-[14rem]/editor-info:hidden')}>
          {t('editor.presets.appliedFolder')}: {label}
        </span>
      </span>
    );
  }
  return (
    <span
      data-applied-preset={record.id}
      title={`${t('editor.presets.applied')}: ${record.name}${record.amount !== 100 ? ` · ${record.amount}%` : ''}`}
      className={clsx(
        'ml-2 flex items-center gap-1 min-w-0 text-primary text-xs',
        compact
          ? 'shrink-[999] max-w-28 @max-[14rem]/editor-info:ml-1 @max-[14rem]/editor-info:gap-0 @max-[14rem]/editor-info:shrink-0'
          : 'max-w-40',
      )}
    >
      <Check size={12} className="shrink-0" aria-label={t('editor.presets.applied')} />
      <span className={clsx('truncate', compact && '@max-[14rem]/editor-info:hidden')}>{record.name}</span>
      {record.amount !== 100 && (
        <span className={clsx('shrink-0', compact && '@max-[14rem]/editor-info:hidden')}>{record.amount}%</span>
      )}
    </span>
  );
}
