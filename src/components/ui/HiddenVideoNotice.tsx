import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Invokes } from './AppProperties';

interface HiddenVideoNoticeProps {
  path: string | null;
  recursive: boolean;
  refreshing: boolean;
  card: boolean;
}

export default function HiddenVideoNotice({ path, recursive, refreshing, card }: HiddenVideoNoticeProps) {
  const { t } = useTranslation();
  const [result, setResult] = useState<{ path: string; recursive: boolean; count: number } | null>(null);
  useEffect(() => {
    if (!path || refreshing) return;
    let active = true;
    invoke<number>(Invokes.CountUnshownVideos, { path, recursive })
      .then((count) => {
        if (active) setResult({ path, recursive, count });
      })
      .catch((error) => {
        if (active) setResult(null);
        console.warn('Unable to count hidden videos', error);
      });
    return () => {
      active = false;
    };
  }, [path, recursive, refreshing]);

  if (refreshing || !result || result.path !== path || result.recursive !== recursive || !result.count) return null;
  return (
    <p className="mt-1 text-xs text-text-secondary" role="status" data-hidden-video-count={result.count}>
      {t('library.videosNotShown', { count: result.count })}
      {card && <> · {t('library.cardMode.videoNotice')}</>}
    </p>
  );
}
