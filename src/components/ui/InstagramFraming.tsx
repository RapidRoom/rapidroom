import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { debouncedSave, waitForPendingEditorSaves } from '../../hooks/useEditorActions';
import { useEditorStore } from '../../store/useEditorStore';
import type { InstagramFrame, InstagramRatio } from './ExportImportProperties';

interface Preview {
  jpeg: string;
  width: number;
  height: number;
  framing: InstagramFrame;
}

interface Props {
  paths: string[];
  ratio: InstagramRatio;
  active: boolean;
  disabled: boolean;
  onFrame: (path: string, frame: InstagramFrame) => void;
  onBusy: (busy: boolean) => void;
}

export default function InstagramFraming({ paths, ratio, active, disabled, onFrame, onBusy }: Props) {
  const { t } = useTranslation();
  const [index, setIndex] = useState(0);
  const path = paths[Math.min(index, Math.max(0, paths.length - 1))];
  const [preview, setPreview] = useState<Preview | null>(null);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const [busy, setBusy] = useState(true);
  const imageRef = useRef<HTMLDivElement>(null);
  const frameRef = useRef<InstagramFrame | null>(null);
  const drag = useRef<{ x: number; y: number; frame: InstagramFrame } | null>(null);
  const callbacks = useRef({ onFrame, onBusy });
  callbacks.current = { onFrame, onBusy };
  const selectedAdjustments = useEditorStore((s) => (s.selectedImage?.path === path ? s.adjustments : null));

  useEffect(() => {
    let cancelled = false;
    if (!active || !path) return;
    setPreview(null);
    setError('');
    setBusy(true);
    callbacks.current.onBusy(true);
    const load = async () => {
      try {
        debouncedSave.flush();
        await waitForPendingEditorSaves();
        const result = await invoke<Preview>('get_instagram_framing_preview', { path, ratio });
        if (cancelled) return;
        frameRef.current = result.framing;
        setPreview(result);
        callbacks.current.onFrame(path, result.framing);
        setBusy(false);
        callbacks.current.onBusy(false);
      } catch (failure) {
        if (!cancelled) setError(String(failure));
      }
    };
    const timer = setTimeout(() => void load(), 300);
    return () => {
      clearTimeout(timer);
      cancelled = true;
    };
  }, [path, ratio, active, selectedAdjustments, retry]);

  const move = (x: number, y: number) => {
    const current = frameRef.current;
    if (!current) return;
    const frame = {
      ...current,
      frame: {
        ...current.frame,
        x: Math.max(0, Math.min(1 - current.frame.width, x)),
        y: Math.max(0, Math.min(1 - current.frame.height, y)),
      },
    };
    frameRef.current = frame;
    setPreview((p) => (p ? { ...p, framing: frame } : null));
    callbacks.current.onFrame(path, frame);
  };

  const persist = async () => {
    const framing = frameRef.current;
    if (!framing) return;
    setBusy(true);
    callbacks.current.onBusy(true);
    setError('');
    try {
      debouncedSave.flush();
      await waitForPendingEditorSaves();
      await invoke('save_instagram_framing', { path, framing });
      setBusy(false);
      callbacks.current.onBusy(false);
    } catch (failure) {
      setError(String(failure));
    }
  };

  return (
    <div className="space-y-3" data-instagram-framing>
      <p className="text-xs text-text-secondary">{t('export.instagram.description')}</p>
      {paths.length > 1 && (
        <div className="flex items-center justify-between gap-2">
          <button type="button" disabled={disabled || busy || index <= 0} onClick={() => setIndex(index - 1)}>
            {t('export.instagram.previous')}
          </button>
          <span className="text-xs">
            {Math.min(index + 1, paths.length)} / {paths.length}
          </span>
          <button
            type="button"
            disabled={disabled || busy || index >= paths.length - 1}
            onClick={() => setIndex(index + 1)}
          >
            {t('export.instagram.next')}
          </button>
        </div>
      )}
      <p className="text-xs break-all">{path?.split(/[\\/]/).pop()}</p>
      {preview && (
        <div
          ref={imageRef}
          className="relative w-full overflow-hidden rounded bg-black"
          style={{ aspectRatio: `${preview.width}/${preview.height}` }}
        >
          <img
            alt={t('export.instagram.preview')}
            src={`data:image/jpeg;base64,${preview.jpeg}`}
            className="block w-full h-full"
            draggable={false}
          />
          <button
            type="button"
            aria-label={t('export.instagram.drag')}
            data-instagram-frame
            disabled={disabled || busy}
            className="absolute border-2 border-white cursor-move touch-none focus:outline-2 focus:outline-accent"
            style={{
              left: `${preview.framing.frame.x * 100}%`,
              top: `${preview.framing.frame.y * 100}%`,
              width: `${preview.framing.frame.width * 100}%`,
              height: `${preview.framing.frame.height * 100}%`,
              boxShadow: '0 0 0 100vmax rgb(0 0 0 / 55%)',
            }}
            onPointerDown={(event) => {
              if (!frameRef.current) return;
              event.preventDefault();
              event.currentTarget.setPointerCapture(event.pointerId);
              drag.current = { x: event.clientX, y: event.clientY, frame: frameRef.current };
              callbacks.current.onBusy(true);
            }}
            onPointerMove={(event) => {
              const start = drag.current;
              const rect = imageRef.current?.getBoundingClientRect();
              if (!start || !rect) return;
              move(
                start.frame.frame.x + (event.clientX - start.x) / rect.width,
                start.frame.frame.y + (event.clientY - start.y) / rect.height,
              );
            }}
            onPointerUp={(event) => {
              if (!drag.current) return;
              drag.current = null;
              event.currentTarget.releasePointerCapture(event.pointerId);
              void persist();
            }}
            onPointerCancel={() => {
              drag.current = null;
              void persist();
            }}
            onKeyDown={(event) => {
              event.stopPropagation();
              if (busy || disabled || drag.current) return;
              const frame = frameRef.current?.frame;
              if (!frame) return;
              const step = event.shiftKey ? 0.05 : 0.01;
              const delta = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[
                event.key
              ];
              if (!delta) return;
              event.preventDefault();
              move(frame.x + delta[0], frame.y + delta[1]);
              void persist();
            }}
          />
        </div>
      )}
      {busy && !error && <p className="text-xs">{t('export.instagram.loading')}</p>}
      {error && (
        <div role="alert" className="text-xs text-red-500">
          <p>{error}</p>
          <button type="button" onClick={() => setRetry((n) => n + 1)}>
            {t('export.instagram.retry')}
          </button>
        </div>
      )}
    </div>
  );
}
