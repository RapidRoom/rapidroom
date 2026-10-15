import { useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'react-toastify';
import { useTranslation } from 'react-i18next';
import { Invokes } from '../components/ui/AppProperties';

interface WriteReport {
  action: 'write' | 'undo';
  written: number;
  failures: { path: string; reason: string }[];
  undoToken: string | null;
}

export function useJpegMetadataReports(refresh: () => unknown | Promise<unknown>) {
  const { t } = useTranslation();
  const undoToken = useRef<string | null>(null);
  const undoMetadata = useCallback(
    async (token?: string) => {
      const target = token ?? undoToken.current;
      if (!target) return;
      try {
        await invoke(Invokes.UndoJpegMetadataWrite, { token: target });
        await refresh();
      } catch (error) {
        toast.error(t('jpegMetadata.failed', { error: String(error) }));
      }
    },
    [refresh, t],
  );

  useEffect(() => {
    let closed = false;
    const pending = listen<WriteReport>('jpeg-metadata-write-report', ({ payload }) => {
      if (closed) return;
      if (payload.undoToken || payload.action === 'undo') undoToken.current = payload.undoToken;
      const token = payload.undoToken;
      const content = (
        <div className="space-y-2">
          <p>
            {t(payload.action === 'undo' ? 'jpegMetadata.restored' : 'jpegMetadata.written', {
              count: payload.written,
            })}
          </p>
          {payload.failures.length > 0 && (
            <ul className="max-h-48 overflow-auto break-words text-xs">
              {payload.failures.map((failure) => (
                <li key={failure.path}>
                  {failure.path}: {failure.reason}
                </li>
              ))}
            </ul>
          )}
          {token && (
            <button className="underline" onClick={() => void undoMetadata(token)}>
              {t('jpegMetadata.undo')}
            </button>
          )}
        </div>
      );
      const options = {
        type: payload.failures.length ? ('warning' as const) : ('info' as const),
        autoClose: payload.failures.length ? (false as const) : 8000,
        closeOnClick: false,
      };
      if (toast.isActive('jpeg-metadata-report')) toast.update('jpeg-metadata-report', { ...options, render: content });
      else toast(content, { ...options, toastId: 'jpeg-metadata-report' });
    });
    return () => {
      closed = true;
      void pending.then((unlisten) => unlisten());
    };
  }, [t, undoMetadata]);

  const writeMetadata = useCallback(
    async (paths: string[]) => {
      try {
        await invoke(Invokes.WriteJpegMetadataForPaths, { paths });
      } catch (error) {
        toast.error(t('jpegMetadata.failed', { error: String(error) }));
      }
    },
    [t],
  );
  return { writeMetadata, undoMetadata, undoToken };
}
