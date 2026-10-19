import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ImageOff, Loader2, Replace, Trash2, X } from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import { Invokes } from '../../ui/AppProperties';
import { useUIStore } from '../../../store/useUIStore';
import { useProcessStore } from '../../../store/useProcessStore';
import { ReferenceViewImage } from '../../../utils/referenceView';
import type { Adjustments } from '../../../utils/adjustments';

interface ReferencePaneProps {
  isChooserOpen: boolean;
  mode?: 'side-by-side' | 'wipe';
  activePath?: string | null;
  activeAdjustments?: Adjustments;
  originalAdjustments?: Adjustments;
  reference: ReferenceViewImage | null;
  wipePosition?: number;
  wipeSwapped?: boolean;
  onSetWipePosition?(position: number): void;
  onSwapWipeSides?(): void;
  onCancelChooser(): void;
  onChoose(): void;
  onClear(): void;
  onExit(): void;
}

interface PaneTransform {
  scale: number;
  x: number;
  y: number;
}

const FIT: PaneTransform = { scale: 1, x: 0, y: 0 };

const blurOnKey = (e: React.KeyboardEvent<HTMLButtonElement>) => {
  if (e.key === 'Tab') return;
  e.currentTarget.blur();
};

export default function ReferencePane({
  isChooserOpen,
  mode = 'side-by-side',
  activePath = null,
  activeAdjustments,
  originalAdjustments,
  reference,
  wipePosition = 50,
  wipeSwapped = false,
  onSetWipePosition,
  onSwapWipeSides,
  onCancelChooser,
  onChoose,
  onClear,
  onExit,
}: ReferencePaneProps) {
  const { t } = useTranslation();
  const isLightsOut = useUIStore((state) => state.lightsOutMode !== 'off');
  const referencePath = reference?.path ?? null;
  const thumbnailUrl = useProcessStore((s) => (referencePath ? s.thumbnails[referencePath] : undefined));
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [activePreviewUrl, setActivePreviewUrl] = useState<string | null>(null);
  const [originalPreviewUrl, setOriginalPreviewUrl] = useState<string | null>(null);
  const [status, setStatus] = useState<'idle' | 'loading' | 'ready' | 'error'>('idle');
  const [transform, setTransform] = useState<PaneTransform>(FIT);
  const containerRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const dragRef = useRef<{ pointerId: number; startX: number; startY: number; origin: PaneTransform } | null>(null);

  useEffect(() => {
    setTransform(FIT);
    setPreviewUrl(null);
    if (!referencePath) {
      setStatus('idle');
      return;
    }

    let active = true;
    let blobUrl: string | null = null;
    setStatus('loading');

    const render = async () => {
      try {
        const adjustments = await invoke<Adjustments | null>(Invokes.ReadCompareAdjustments, {
          path: referencePath,
        }).catch(() => null);
        if (!active) return;
        const bytes = await invoke<Uint8Array>(Invokes.GeneratePreviewForPath, {
          path: referencePath,
          jsAdjustments: adjustments ?? {},
        });
        if (!active) return;
        blobUrl = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }));
        setPreviewUrl(blobUrl);
        setStatus('ready');
      } catch (err) {
        console.error('Failed to render reference image:', err);
        if (active) setStatus('error');
      }
    };
    render();

    return () => {
      active = false;
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  }, [referencePath]);

  const renderActivePreview = useCallback(
    async (adjustments: Adjustments) => {
      if (!activePath) return null;
      const bytes = await invoke<Uint8Array>(Invokes.GeneratePreviewForPath, {
        path: activePath,
        jsAdjustments: adjustments,
      });
      return URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }));
    },
    [activePath],
  );

  useEffect(() => {
    if (mode !== 'wipe' || !activeAdjustments) {
      setActivePreviewUrl(null);
      return;
    }
    let active = true;
    let blobUrl: string | null = null;
    renderActivePreview(activeAdjustments)
      .then((url) => {
        if (!active) return url && URL.revokeObjectURL(url);
        blobUrl = url;
        setActivePreviewUrl(url);
      })
      .catch((err) => {
        console.error('Failed to render current comparison image:', err);
        if (active) setActivePreviewUrl(null);
      });
    return () => {
      active = false;
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  }, [activeAdjustments, mode, renderActivePreview]);

  useEffect(() => {
    if (mode !== 'wipe' || referencePath || !originalAdjustments) {
      setOriginalPreviewUrl(null);
      return;
    }
    let active = true;
    let blobUrl: string | null = null;
    renderActivePreview(originalAdjustments)
      .then((url) => {
        if (!active) return url && URL.revokeObjectURL(url);
        blobUrl = url;
        setOriginalPreviewUrl(url);
      })
      .catch((err) => {
        console.error('Failed to render original comparison image:', err);
        if (active) setOriginalPreviewUrl(null);
      });
    return () => {
      active = false;
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  }, [mode, originalAdjustments, referencePath, renderActivePreview]);

  const getMaxScale = useCallback(() => {
    const container = containerRef.current;
    const img = imageRef.current;
    if (!container || !img?.naturalWidth || !img.naturalHeight) return 8;
    const fitScale = Math.min(container.clientWidth / img.naturalWidth, container.clientHeight / img.naturalHeight);
    return Math.max(8, 2 / fitScale);
  }, []);

  const clampTransform = useCallback((next: PaneTransform): PaneTransform => {
    const container = containerRef.current;
    if (!container || next.scale <= 1) return FIT;
    const minX = container.clientWidth * (1 - next.scale);
    const minY = container.clientHeight * (1 - next.scale);
    return {
      scale: next.scale,
      x: Math.min(0, Math.max(minX, next.x)),
      y: Math.min(0, Math.max(minY, next.y)),
    };
  }, []);

  const zoomAt = useCallback(
    (clientX: number, clientY: number, getScale: (current: number) => number) => {
      const container = containerRef.current;
      if (!container) return;
      const rect = container.getBoundingClientRect();
      const px = clientX - rect.left;
      const py = clientY - rect.top;
      setTransform((current) => {
        const scale = Math.min(getMaxScale(), Math.max(1, getScale(current.scale)));
        const ratio = scale / current.scale;
        return clampTransform({ scale, x: px - (px - current.x) * ratio, y: py - (py - current.y) * ratio });
      });
    },
    [clampTransform, getMaxScale],
  );

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const handleWheel = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const factor = Math.exp(-e.deltaY * 0.0015);
      zoomAt(e.clientX, e.clientY, (scale) => scale * factor);
    };
    container.addEventListener('wheel', handleWheel, { passive: false });
    return () => container.removeEventListener('wheel', handleWheel);
  }, [zoomAt]);

  const handleDoubleClick = (e: React.MouseEvent) => {
    if (transform.scale > 1.01) {
      setTransform(FIT);
      return;
    }
    const container = containerRef.current;
    const img = imageRef.current;
    let target = 2;
    if (container && img?.naturalWidth && img.naturalHeight) {
      const fitScale = Math.min(container.clientWidth / img.naturalWidth, container.clientHeight / img.naturalHeight);
      target = Math.max(2, 1 / fitScale);
    }
    zoomAt(e.clientX, e.clientY, () => target);
  };

  const handlePointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0 || transform.scale <= 1) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { pointerId: e.pointerId, startX: e.clientX, startY: e.clientY, origin: transform };
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    setTransform(
      clampTransform({
        scale: drag.origin.scale,
        x: drag.origin.x + e.clientX - drag.startX,
        y: drag.origin.y + e.clientY - drag.startY,
      }),
    );
  };

  const handlePointerUp = (e: React.PointerEvent) => {
    if (dragRef.current?.pointerId === e.pointerId) dragRef.current = null;
  };

  const displayUrl = previewUrl ?? (status === 'loading' ? thumbnailUrl : undefined);
  const isZoomed = transform.scale > 1.01;
  const comparisonReference = previewUrl ?? originalPreviewUrl;
  const baseUrl = wipeSwapped ? comparisonReference : activePreviewUrl;
  const revealUrl = wipeSwapped ? activePreviewUrl : comparisonReference;
  const moveWipe = (clientX: number) => {
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect || !onSetWipePosition) return;
    onSetWipePosition(((clientX - rect.left) / rect.width) * 100);
  };
  const handleWipeKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (mode !== 'wipe') return;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      onSetWipePosition?.(wipePosition + (e.key === 'ArrowLeft' ? -5 : 5));
    } else if (e.key.toLowerCase() === 'a') {
      e.preventDefault();
      onSwapWipeSides?.();
    }
  };

  return (
    <div
      className={clsx(
        'lights-out-content relative z-10 flex-1 basis-0 min-w-0 overflow-hidden rounded-lg border',
        isLightsOut ? 'bg-black border-transparent' : 'bg-bg-secondary border-dashed border-text-secondary/40',
      )}
      data-testid="editor-reference-pane"
      data-reference-mode={mode}
      data-reference-state={reference ? status : 'choose-reference'}
    >
      <div
        ref={containerRef}
        className="absolute inset-0 touch-none"
        style={{ cursor: isZoomed ? 'grab' : displayUrl ? 'zoom-in' : 'default' }}
        onDoubleClick={displayUrl ? handleDoubleClick : undefined}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onKeyDown={handleWipeKeyDown}
        tabIndex={mode === 'wipe' ? 0 : undefined}
        aria-label={mode === 'wipe' ? 'Compare versions; arrow keys move the divider, A swaps sides' : undefined}
      >
        {mode === 'wipe' && baseUrl && (
          <div
            className="w-full h-full origin-top-left"
            style={{ transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})` }}
          >
            <img className="w-full h-full object-contain select-none" draggable={false} src={baseUrl} alt="Your edit" />
          </div>
        )}
        {mode === 'wipe' && revealUrl && (
          <div className="absolute inset-0 overflow-hidden" style={{ clipPath: `inset(0 ${100 - wipePosition}% 0 0)` }}>
            <div
              className="w-full h-full origin-top-left"
              style={{ transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})` }}
            >
              <img
                className="w-full h-full object-contain select-none"
                draggable={false}
                src={revealUrl}
                alt="AI edit"
              />
            </div>
          </div>
        )}
        {mode !== 'wipe' && displayUrl && (
          <div
            className="w-full h-full origin-top-left"
            style={{ transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})` }}
          >
            <img
              ref={imageRef}
              alt={reference?.label ?? ''}
              className={clsx('w-full h-full object-contain select-none', !previewUrl && 'blur-[1px]')}
              draggable={false}
              src={displayUrl}
            />
          </div>
        )}
        {mode === 'wipe' && (baseUrl || revealUrl) && (
          <div
            aria-label="Comparison divider"
            aria-orientation="vertical"
            aria-valuemax={100}
            aria-valuemin={0}
            aria-valuenow={Math.round(wipePosition)}
            className="absolute inset-y-0 z-30 w-8 -translate-x-1/2 cursor-ew-resize touch-none"
            data-wipe-handle="true"
            role="slider"
            style={{ left: `${wipePosition}%` }}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
              moveWipe(e.clientX);
            }}
            onPointerMove={(e) => {
              if (e.currentTarget.hasPointerCapture(e.pointerId)) moveWipe(e.clientX);
            }}
          >
            <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-white shadow" />
            <div className="absolute left-1/2 top-1/2 h-8 w-5 -translate-x-1/2 -translate-y-1/2 rounded bg-white/90 shadow" />
          </div>
        )}
      </div>

      {reference && status === 'loading' && (
        <div className="lights-out-chrome absolute bottom-3 right-3 pointer-events-none">
          <Loader2 size={20} className="animate-spin text-text-secondary" />
        </div>
      )}

      {reference && status === 'error' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-text-secondary pointer-events-none">
          <ImageOff size={32} />
          <span className="text-sm">{t('editor.referenceView.unavailable')}</span>
        </div>
      )}

      {mode !== 'wipe' && (isChooserOpen || !reference) && (
        <div className="lights-out-chrome absolute inset-0 flex items-center justify-center p-6 bg-bg-secondary/70 pointer-events-none">
          <div className="pointer-events-auto max-w-xs rounded-lg bg-surface/95 px-4 py-3 text-center shadow-lg">
            <p className="text-sm text-text-primary">{t('editor.referenceView.chooseHint')}</p>
            <button
              className="mt-3 rounded-md bg-bg-primary px-3 py-1 text-xs text-text-primary hover:bg-card-active"
              onClick={reference ? onCancelChooser : onExit}
              onKeyDown={blurOnKey}
              type="button"
            >
              {t('editor.referenceView.cancel')}
            </button>
          </div>
        </div>
      )}

      <div className="lights-out-chrome absolute inset-x-2 top-2 flex items-start justify-between gap-2 pointer-events-none">
        <div className="flex min-w-0 items-center gap-2 rounded-md bg-surface/90 px-2 py-1 shadow">
          <span className="shrink-0 rounded bg-text-secondary/25 px-1.5 text-[10px] font-semibold uppercase tracking-wide text-text-primary">
            {mode === 'wipe' ? (wipeSwapped ? 'AI edit' : 'Your edit') : t('editor.referenceView.reference')}
          </span>
          {reference && mode !== 'wipe' && (
            <span className="truncate text-xs text-text-primary" data-tooltip={reference.path}>
              {reference.label}
            </span>
          )}
          {reference && (
            <span className="shrink-0 text-[10px] text-text-secondary">{t('editor.referenceView.readOnly')}</span>
          )}
        </div>
        <div className="pointer-events-auto flex shrink-0 items-center gap-1 rounded-md bg-surface/90 p-0.5 shadow">
          {reference && (
            <>
              <button
                className="rounded p-1 text-text-primary hover:bg-card-active"
                data-tooltip={t('editor.referenceView.replace')}
                aria-label={t('editor.referenceView.replace')}
                onClick={onChoose}
                onKeyDown={blurOnKey}
                type="button"
              >
                <Replace size={14} />
              </button>
              <button
                className="rounded p-1 text-text-primary hover:bg-card-active"
                data-tooltip={t('editor.referenceView.clear')}
                aria-label={t('editor.referenceView.clear')}
                onClick={onClear}
                onKeyDown={blurOnKey}
                type="button"
              >
                <Trash2 size={14} />
              </button>
            </>
          )}
          <button
            className="rounded p-1 text-text-primary hover:bg-card-active"
            data-tooltip={t('editor.referenceView.close')}
            aria-label={t('editor.referenceView.close')}
            onClick={onExit}
            onKeyDown={blurOnKey}
            type="button"
          >
            <X size={14} />
          </button>
        </div>
      </div>
      {mode === 'wipe' && (
        <div className="lights-out-chrome absolute bottom-2 left-2 flex items-center gap-2 rounded-md bg-surface/90 px-2 py-1 text-xs text-text-primary shadow pointer-events-none">
          <span>{wipeSwapped ? 'Your edit' : 'AI edit'}</span>
          <span className="text-text-secondary">←/→ divider · A swap</span>
        </div>
      )}
    </div>
  );
}
