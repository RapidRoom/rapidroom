import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useLibraryStore } from '../../store/useLibraryStore';

export interface PhotoCount {
  count: number;
  complete: boolean;
}

export default function FolderPhotoCount({ path, modified }: { path: string; modified?: number }) {
  const { t } = useTranslation();
  const element = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false);
  const [count, setCount] = useState<PhotoCount | null>(null);
  const trees = useLibraryStore((state) => state.folderTrees);
  const pinnedTrees = useLibraryStore((state) => state.pinnedFolderTrees);

  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting), { rootMargin: '80px' });
    if (element.current) observer.observe(element.current);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    invoke<PhotoCount>('count_folder_photos', { path, recursive: false })
      .then((result) => {
        if (!cancelled) setCount(result);
      })
      .catch(() => {
        if (!cancelled) setCount(null);
      });
    return () => {
      cancelled = true;
    };
  }, [path, modified, visible, trees, pinnedTrees]);

  return (
    <span
      ref={element}
      className="ml-auto min-w-8 shrink-0 text-right tabular-nums text-xs text-text-secondary"
      title={t('library.photoCounts.folderHint')}
    >
      {count ? `${count.count.toLocaleString()}${count.complete ? '' : '+'}` : '…'}
    </span>
  );
}
