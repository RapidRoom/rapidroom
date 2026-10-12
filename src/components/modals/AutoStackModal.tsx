import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Invokes } from '../ui/AppProperties';
import { useLibraryStore } from '../../store/useLibraryStore';
import { acceptStackUpdates, StackUpdate } from '../../utils/photoStacks';

export default function AutoStackModal() {
  const { t } = useTranslation();
  const open = useLibraryStore((s) => s.stackPreviewOpen);
  const [seconds, setSeconds] = useState('1');
  const [groups, setGroups] = useState<string[][] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const generation = useRef(0);
  useEffect(() => {
    generation.current++;
    setGroups(null);
    setError('');
    setBusy(false);
  }, [open]);
  if (!open) return null;
  const close = () => {
    if (!busy) useLibraryStore.getState().setLibrary({ stackPreviewOpen: false });
  };
  const preview = async () => {
    const run = ++generation.current;
    setBusy(true);
    setError('');
    setGroups(null);
    try {
      const paths = useLibraryStore
        .getState()
        .imageList.filter((i) => !i.is_virtual_copy && !i.is_cloud_placeholder)
        .map((i) => i.path);
      const found = await invoke<string[][]>(Invokes.PreviewPhotoStacks, { paths, seconds: Number(seconds) });
      if (generation.current === run) setGroups(found);
    } catch (err) {
      if (generation.current === run) setError(String(err));
    } finally {
      if (generation.current === run) setBusy(false);
    }
  };
  const apply = async () => {
    if (!groups?.length) return;
    setBusy(true);
    setError('');
    try {
      const updates = await invoke<StackUpdate[]>(Invokes.ApplyPhotoStacks, { groups });
      acceptStackUpdates(updates);
      useLibraryStore
        .getState()
        .setLibrary({ stackPreviewOpen: false, multiSelectedPaths: [], libraryActivePath: null });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div
      className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/70"
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === 'Escape') close();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="auto-stack-title"
        className="w-[520px] max-h-[85vh] overflow-auto rounded-lg bg-surface p-6 text-text-primary shadow-xl"
      >
        <h2 id="auto-stack-title" className="text-lg font-semibold">
          {t('stacks.auto')}
        </h2>
        <p className="my-3 text-sm">{t('stacks.explanation')}</p>
        <label className="flex gap-3 items-center">
          {t('stacks.gap')}
          <input
            autoFocus
            type="number"
            min="0"
            max="3600"
            step="0.1"
            value={seconds}
            disabled={busy}
            className="w-24 rounded bg-black/30 p-2"
            onChange={(e) => {
              setSeconds(e.target.value);
              setGroups(null);
            }}
          />
        </label>
        {groups && (
          <div className="my-4">
            <p>{t('stacks.previewCount', { count: groups.length })}</p>
            <ul className="mt-2 max-h-60 overflow-auto text-sm">
              {groups.map((g) => (
                <li key={g[0]} className="border-b border-white/10 py-2">
                  {g.map((p) => p.split(/[\\/]/).pop()).join(' · ')}
                </li>
              ))}
            </ul>
          </div>
        )}
        {error && (
          <p role="alert" className="my-3 text-red-400">
            {error}
          </p>
        )}
        <div className="mt-4 flex justify-end gap-3">
          <button disabled={busy} onClick={close}>
            {t('stacks.cancel')}
          </button>
          <button
            disabled={
              busy ||
              seconds.trim() === '' ||
              !Number.isFinite(Number(seconds)) ||
              Number(seconds) < 0 ||
              Number(seconds) > 3600
            }
            onClick={() => void preview()}
          >
            {t('stacks.preview')}
          </button>
          <button
            disabled={busy || !groups?.length || !!useLibraryStore.getState().cardBrowseRoot}
            onClick={() => void apply()}
          >
            {t('stacks.apply')}
          </button>
        </div>
      </div>
    </div>
  );
}
