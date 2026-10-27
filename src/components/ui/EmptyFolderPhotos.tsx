import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { PhotoCount } from './FolderPhotoCount';
import Button from './Button';
import { useLibraryStore } from '../../store/useLibraryStore';

export default function EmptyFolderPhotos({ path, onShowSubfolders }: { path: string; onShowSubfolders: () => void }) {
  const { t } = useTranslation();
  const trees = useLibraryStore((state) => state.folderTrees);
  const [result, setResult] = useState<{ path: string; value: PhotoCount } | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    invoke<PhotoCount>('count_folder_photos', { path, recursive: true })
      .then((value) => {
        if (!cancelled) setResult({ path, value });
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [path, trees]);
  const count = result?.path === path ? result.value : null;
  return (
    <div className="flex flex-col items-center text-center gap-4 max-w-xl" role="status">
      <p>{t('library.photoCounts.emptyFolder')}</p>
      {count && count.count > 0 && (
        <p className="text-text-secondary">
          {t(count.complete ? 'library.photoCounts.descendants' : 'library.photoCounts.descendantsPartial', {
            count: count.count,
          })}
        </p>
      )}
      {failed && <p className="text-text-secondary">{t('library.photoCounts.checkFailed')}</p>}
      {!count && !failed && <p className="text-text-secondary">{t('library.photoCounts.checking')}</p>}
      {count && !count.complete && count.count === 0 && (
        <p className="text-text-secondary">{t('library.photoCounts.scanLimited')}</p>
      )}
      {count && (count.count > 0 || !count.complete) && (
        <Button onClick={onShowSubfolders}>{t('library.photoCounts.showSubfolders')}</Button>
      )}
    </div>
  );
}
