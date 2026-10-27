import clsx from 'clsx';
import { ImageFlag } from './AppProperties';
import { FLAG_FILL_CLASSES, FLAG_ICONS } from '../../utils/imageFlags';

interface FlagBadgesProps {
  flag: ImageFlag | null;
  hasPrecedingBadge: boolean;
}

export default function FlagBadges({ flag, hasPrecedingBadge }: FlagBadgesProps) {
  return (
    <>
      {Object.values(ImageFlag).map((option) => {
        const Icon = FLAG_ICONS[option];
        const isActive = flag === option;
        return (
          <div
            key={option}
            className={clsx(
              'text-white flex items-center shrink-0 transition-all duration-200 ease-out overflow-hidden rounded-sm',
              isActive
                ? clsx('max-w-4 p-0.5 opacity-100 scale-100', FLAG_FILL_CLASSES[option])
                : 'max-w-0 p-0 opacity-0 scale-75 pointer-events-none',
              isActive && hasPrecedingBadge ? 'ml-1.5' : 'ml-0',
            )}
          >
            <Icon size={10} className={clsx(option === ImageFlag.Pick && 'fill-white')} />
          </div>
        );
      })}
    </>
  );
}
