import { useTranslation } from 'react-i18next';
import { useLibraryStore } from '../../store/useLibraryStore';
import { stackBadgeInfo, toggleStack } from '../../utils/photoStacks';

export default function StackBadge({ path, hasPrecedingBadge = true }: { path: string; hasPrecedingBadge?: boolean }) {
  const { t } = useTranslation();
  const info = useLibraryStore((s) => stackBadgeInfo(s.imageList, path));
  const expanded = useLibraryStore((s) => !!info && s.expandedStacks.has(info.key));
  if (!info) return null;
  const { key, count } = info;
  return (
    <button
      type="button"
      className={`${hasPrecedingBadge ? 'ml-1.5 ' : ''}rounded bg-black/70 px-1 text-xs text-white pointer-events-auto`}
      aria-label={t('stacks.badge', { count })}
      aria-expanded={expanded}
      onPointerDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        toggleStack(key);
      }}
    >
      {count}
    </button>
  );
}
