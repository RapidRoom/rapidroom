import { useTranslation } from 'react-i18next';
import { useLibraryStore } from '../../store/useLibraryStore';
import { isStackedPhoto, stackKey, toggleStack } from '../../utils/photoStacks';

export default function StackBadge({ path }: { path: string }) {
  const { t } = useTranslation();
  const images = useLibraryStore((s) => s.imageList);
  const expanded = useLibraryStore((s) => s.expandedStacks);
  const image = images.find((i) => i.path === path);
  if (!image || !isStackedPhoto(image)) return null;
  const key = stackKey(image);
  const count = images.filter((i) => isStackedPhoto(i) && stackKey(i) === key).length;
  return (
    <button
      type="button"
      className="ml-1.5 rounded bg-black/70 px-1 text-xs text-white pointer-events-auto"
      aria-label={t('stacks.badge', { count })}
      aria-expanded={expanded.has(key)}
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
