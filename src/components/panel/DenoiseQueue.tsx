import { useSettingsStore } from '../../store/useSettingsStore';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { Check, ChevronDown, Grip, Loader2, X } from 'lucide-react';
import { platform } from '@tauri-apps/plugin-os';
import { toast } from 'react-toastify';
import { useLibraryStore } from '../../store/useLibraryStore';
import { DenoiseQueueSummary, refreshDenoiseMarks, useDenoiseQueueStore } from '../../store/useDenoiseQueueStore';

export function DenoiseBadge({ path, inline = false }: { path: string; inline?: boolean }) {
  const { t } = useTranslation();
  const status = useLibraryStore((s) => s.denoiseStatuses[path]);
  if (!status?.marked && !status?.denoised) return null;
  const Icon = status.denoised ? Check : Grip;
  return (
    <span
      className={
        inline
          ? 'inline-flex shrink-0 rounded bg-black/50 text-white p-1'
          : 'absolute top-1 left-1 z-10 rounded bg-black/70 text-white p-1'
      }
      title={t(status.denoised ? 'denoiseQueue.done' : 'denoiseQueue.marked')}
    >
      <Icon size={12} />
    </span>
  );
}

export default function DenoiseQueue() {
  const { t } = useTranslation();
  const queue = useDenoiseQueueStore();
  const folder = useLibraryStore((s) => s.currentFolderPath);
  const imageList = useLibraryStore((s) => s.imageList);
  const aiFree = useSettingsStore((s) => s.appSettings?.aiProvider === 'ai-free');
  const [method, setMethod] = useState(aiFree ? 'bm3d' : 'best');
  useEffect(() => {
    if (aiFree && ['best', 'fast', 'nind'].includes(method)) setMethod('bm3d');
  }, [aiFree, method]);
  const [sharpen, setSharpen] = useState(false);
  const [strength, setStrength] = useState(50);
  const [recursive, setRecursive] = useState(false);
  const lastStatusPaths = useRef<string | null>(null);
  useEffect(() => {
    // Backend refreshes may replace the list without changing its paths. One status map
    // drives badges and filters; debounce metadata bursts, without mutating imageList.
    const paths = imageList.map((image) => image.path);
    const key = JSON.stringify(paths);
    if (lastStatusPaths.current !== key) {
      lastStatusPaths.current = key;
      void refreshDenoiseMarks(paths).catch(console.error);
      return;
    }
    const timer = window.setTimeout(() => {
      void refreshDenoiseMarks(paths).catch(console.error);
    }, 100);
    return () => window.clearTimeout(timer);
  }, [imageList]);
  useEffect(() => {
    const subscriptions = [
      listen<{ jobId: number; current: number; total: number; path: string; summary: DenoiseQueueSummary }>(
        'denoise-queue-progress',
        ({ payload }) => {
          if (payload.jobId !== useDenoiseQueueStore.getState().jobId) return;
          useDenoiseQueueStore.setState({ progress: payload, summary: payload.summary });
          void refreshDenoiseMarks([payload.path]).catch(console.error);
        },
      ),
      listen<{ jobId: number; message: string }>('denoise-progress', ({ payload }) => {
        if (payload.jobId === useDenoiseQueueStore.getState().jobId)
          useDenoiseQueueStore.setState({ message: payload.message });
      }),
    ];
    return () => {
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
  }, []);
  const start = async () => {
    if (useDenoiseQueueStore.getState().starting || useDenoiseQueueStore.getState().jobId !== null || !folder) return;
    useDenoiseQueueStore.setState({ starting: true, summary: null, progress: null, message: '' });
    let jobId: number | null = null;
    try {
      const paths = await invoke<string[]>('denoise_queue_paths', { folder, recursive });
      jobId = await invoke<number>('create_denoise_job');
      useDenoiseQueueStore.setState({ jobId, starting: false, setupOpen: false });
      const summary = await invoke<DenoiseQueueSummary>('run_denoise_queue', {
        jobId,
        paths,
        options: { method, strength, sharpen: (method === 'best' || method === 'fast') && sharpen },
      });
      useDenoiseQueueStore.setState({ summary });
      await refreshDenoiseMarks(paths);
    } catch (error) {
      toast.error(String(error));
    } finally {
      if (jobId !== null) await invoke('cancel_denoise', { jobId }).catch(console.error);
      useDenoiseQueueStore.setState({ jobId: null, starting: false, message: '' });
    }
  };
  const busy = queue.starting || queue.jobId !== null;
  const controlClass = 'bg-bg-secondary rounded px-2 py-1 border border-border-color';
  return (
    <>
      {queue.setupOpen && (
        <div
          role="dialog"
          aria-label={t('denoiseQueue.title')}
          className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center"
        >
          <div className="bg-surface text-text-primary rounded-xl p-6 w-[480px] flex flex-col gap-4">
            <h2 className="text-lg font-semibold">{t('denoiseQueue.title')}</h2>
            <p className="text-sm break-all">{folder ?? t('denoiseQueue.chooseFolder')}</p>
            <label className="flex justify-between">
              {t('denoiseQueue.method')}
              <span className="relative">
                <select
                  aria-label={t('denoiseQueue.method')}
                  className={`${controlClass} appearance-none pr-7 text-text-primary`}
                  value={method}
                  onChange={(e) => setMethod(e.target.value)}
                >
                  {(['best', 'fast', 'nind', 'bm3d', 'raw9'] as const)
                    .filter((value) => value !== 'raw9' || platform() === 'macos')
                    .map((value) => (
                      <option
                        className="bg-bg-secondary text-text-primary"
                        key={value}
                        value={value}
                        disabled={aiFree && ['best', 'fast', 'nind'].includes(value)}
                      >
                        {t(`denoiseQueue.methods.${value}`)}
                      </option>
                    ))}
                </select>
                <ChevronDown
                  size={14}
                  className="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none"
                  aria-hidden="true"
                />
              </span>
            </label>
            {method === 'best' || method === 'fast' ? (
              <label>
                <input type="checkbox" checked={sharpen} onChange={(e) => setSharpen(e.target.checked)} />{' '}
                {t('denoiseQueue.sharpen')}
              </label>
            ) : (
              <label className="flex justify-between">
                {t('denoiseQueue.strength')}
                <input
                  aria-label={t('denoiseQueue.strength')}
                  className={controlClass}
                  type="number"
                  min="0"
                  max="100"
                  value={strength}
                  onChange={(e) => setStrength(Number(e.target.value))}
                />
              </label>
            )}
            <label>
              <input type="checkbox" checked={recursive} onChange={(e) => setRecursive(e.target.checked)} />{' '}
              {t('denoiseQueue.recursive')}
            </label>
            <p className="text-sm text-text-secondary">{t('denoiseQueue.hint')}</p>
            <div className="flex justify-end gap-3">
              <button onClick={() => useDenoiseQueueStore.setState({ setupOpen: false })}>
                {t('denoiseQueue.close')}
              </button>
              <button className={controlClass} disabled={busy || !folder} onClick={() => void start()}>
                {t('denoiseQueue.start')}
              </button>
            </div>
          </div>
        </div>
      )}
      {(busy || queue.summary) && (
        <aside
          aria-label={t('denoiseQueue.title')}
          className="fixed bottom-5 right-5 z-40 rounded-lg p-4 shadow-lg bg-surface text-text-primary max-w-md border border-border-color"
        >
          <div className="flex items-center gap-2">
            {busy ? <Loader2 className="animate-spin" size={16} /> : <Check size={16} />}
            <strong>{t('denoiseQueue.title')}</strong>
            {busy ? (
              <button
                className="ml-auto"
                disabled={queue.jobId === null}
                onClick={() => void invoke('cancel_denoise', { jobId: queue.jobId }).catch(console.error)}
              >
                {t('denoiseQueue.cancel')}
              </button>
            ) : (
              <button
                aria-label={t('denoiseQueue.close')}
                onClick={() => useDenoiseQueueStore.setState({ summary: null })}
              >
                <X size={16} />
              </button>
            )}
          </div>
          {queue.progress && (
            <p className="text-sm">
              {queue.progress.current} / {queue.progress.total} · {queue.progress.path.split(/[\\/]/).pop()}
            </p>
          )}
          {queue.message && <p className="text-sm">{queue.message}</p>}
          {queue.summary && (
            <>
              <p className="text-sm">
                {t('denoiseQueue.summary', queue.summary as unknown as Record<string, unknown>)}
              </p>
              {queue.summary.cancelled && <p>{t('denoiseQueue.cancelled')}</p>}
              {queue.summary.errors.length > 0 && (
                <details>
                  <summary>{t('denoiseQueue.errors')}</summary>
                  <ul className="text-sm max-h-40 overflow-y-auto break-all">
                    {queue.summary.errors.map((error, i) => (
                      <li key={i}>{error}</li>
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
        </aside>
      )}
    </>
  );
}
