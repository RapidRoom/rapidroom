import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import type { InstagramPreview } from './InstagramFraming';
import type { InstagramRatio } from './ExportImportProperties';

interface Props {
  paths: string[];
  index: number;
  ratio: InstagramRatio;
  preview: InstagramPreview | null;
  active: boolean;
  disabled: boolean;
  onSelect: (index: number) => void;
}

export default function InstagramFrameStrip({ paths, index, ratio, preview, active, disabled, onSelect }: Props) {
  const { t } = useTranslation();
  const [thumbnails, setThumbnails] = useState<Record<string, InstagramPreview>>({});
  const cache = useRef(new Map<string, InstagramPreview>());
  const worker = useRef(Promise.resolve());
  const start = Math.max(0, Math.min(index - 2, paths.length - 5));
  const visible = paths.slice(start, start + 5);
  const windowKey = JSON.stringify(visible);
  const path = paths[index];

  useEffect(() => {
    const key = `${ratio}:${path}`;
    cache.current.delete(key);
    if (preview) cache.current.set(key, preview);
    while (cache.current.size > 12) cache.current.delete(cache.current.keys().next().value!);
    setThumbnails(Object.fromEntries(cache.current));
  }, [preview, path, ratio]);

  useEffect(() => {
    let cancelled = false;
    if (!active || !preview) return;
    // A single worker serializes small previews, including across rapid navigation.
    worker.current = worker.current.then(async () => {
      for (const item of JSON.parse(windowKey) as string[]) {
        if (cancelled) return;
        const key = `${ratio}:${item}`;
        if (cache.current.has(key)) continue;
        try {
          const result = await invoke<InstagramPreview>('get_instagram_framing_preview', {
            path: item,
            ratio,
            maxSize: 320,
          });
          if (cancelled) return;
          cache.current.set(key, result);
          while (cache.current.size > 12) cache.current.delete(cache.current.keys().next().value!);
          setThumbnails(Object.fromEntries(cache.current));
        } catch {
          // The selected photo has its own visible error/retry handling.
        }
      }
    });
    return () => {
      cancelled = true;
    };
  }, [active, ratio, windowKey, preview?.jpeg]);

  return (
    <div className="flex gap-1 overflow-x-auto py-1" role="group" aria-label={t('export.instagram.strip')}>
      {visible.map((item, offset) => {
        const itemIndex = start + offset;
        const thumb = itemIndex === index ? preview : thumbnails[`${ratio}:${item}`];
        return (
          <button
            key={item}
            type="button"
            disabled={disabled}
            aria-label={t('export.instagram.selectPhoto', { index: itemIndex + 1, name: item.split(/[\\/]/).pop() })}
            aria-pressed={itemIndex === index}
            data-instagram-strip-index={itemIndex}
            className={`min-w-0 flex-1 rounded border-2 p-0.5 ${itemIndex === index ? 'border-accent' : 'border-transparent'} disabled:opacity-60`}
            onClick={() => onSelect(itemIndex)}
          >
            <span className="relative flex h-14 items-center justify-center overflow-hidden bg-black rounded">
              {thumb ? (
                <svg viewBox={`0 0 ${thumb.width} ${thumb.height}`} className="w-full h-full" aria-hidden="true">
                  <image href={`data:image/jpeg;base64,${thumb.jpeg}`} width={thumb.width} height={thumb.height} />
                  <rect
                    x={thumb.framing.frame.x * thumb.width}
                    y={thumb.framing.frame.y * thumb.height}
                    width={thumb.framing.frame.width * thumb.width}
                    height={thumb.framing.frame.height * thumb.height}
                    fill="none"
                    stroke="white"
                    strokeWidth="1"
                    vectorEffect="non-scaling-stroke"
                  />
                </svg>
              ) : (
                <span className="text-xs text-white">{itemIndex + 1}</span>
              )}
            </span>
            <span className="block truncate text-[10px]">{item.split(/[\\/]/).pop()}</span>
          </button>
        );
      })}
    </div>
  );
}
