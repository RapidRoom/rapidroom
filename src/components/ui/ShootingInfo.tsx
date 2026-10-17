import clsx from 'clsx';
import { useSettingsStore } from '../../store/useSettingsStore';
import { shootingInfoParts } from '../../utils/shootingInfo';

/** Read-only metadata: never part of image adjustments or edit history. */
export default function ShootingInfo({
  exif,
  className,
}: {
  exif?: Record<string, unknown> | null;
  className?: string;
}) {
  const mode = useSettingsStore((s) => s.appSettings?.shootingInfoVisibility) ?? 'always';
  const { lens, details } = shootingInfoParts(exif);
  const full = [lens, details].filter(Boolean).join(' · ');
  if (mode === 'off' || !full) return null;

  return (
    <div
      data-shooting-info
      title={full}
      aria-label={full}
      className={clsx(
        'flex justify-center min-w-0 max-w-full w-full text-xs text-text-secondary whitespace-nowrap overflow-hidden',
        className,
      )}
    >
      <div
        className={clsx(
          'flex min-w-0 max-w-full items-center overflow-hidden',
          mode === 'hover' &&
            'opacity-0 group-hover/shooting-info:opacity-100 group-focus-within/shooting-info:opacity-100',
        )}
      >
        {lens && <span className="min-w-0 truncate shrink-[999]">{lens}</span>}
        {lens && details && (
          <span className="shrink-0 mx-1" aria-hidden="true">
            ·
          </span>
        )}
        {details && <span className="min-w-0 truncate">{details}</span>}
      </div>
    </div>
  );
}
