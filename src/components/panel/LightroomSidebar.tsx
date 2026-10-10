import { useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { useShallow } from 'zustand/react/shallow';
import { useEditorStore } from '../../store/useEditorStore';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useProcessStore } from '../../store/useProcessStore';
import { useEditorActions, debouncedSetHistory } from '../../hooks/useEditorActions';
import { type Adjustments, normalizeLoadedAdjustments } from '../../utils/adjustments';
import { describeHistoryChange } from '../../utils/editHistory';
import { isPathInCardRoot } from '../../utils/cardMode';
import PresetsPanel from './right/PresetsPanel';

type Snapshot = { id: string; name: string; adjustments: Adjustments };
interface Props {
  children: ReactNode;
  onNavigateToCommunity(): void;
  onZoom(zoom: number, fit?: boolean): void;
}

export default function LightroomSidebar({ children, onNavigateToCommunity, onZoom }: Props) {
  const { t } = useTranslation();
  const { selectedImage, history, historyIndex, historyDetails, finalPreviewUrl, ready } = useEditorStore(
    useShallow((s) => ({
      selectedImage: s.selectedImage,
      history: s.history,
      historyIndex: s.historyIndex,
      historyDetails: s.historyDetails,
      finalPreviewUrl: s.finalPreviewUrl,
      ready: !!s.selectedImage?.isReady && !s.isSliderDragging && !s.previewOverride && !s.showOriginal,
    })),
  );
  const path = selectedImage?.path;
  const thumbnail = useProcessStore((s) => (path ? s.thumbnails[path] : undefined));
  const cardRoot = useLibraryStore((s) => s.cardBrowseRoot);
  const readOnly = isPathInCardRoot(path, cardRoot);
  const { setAdjustments } = useEditorActions();
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [loadedPath, setLoadedPath] = useState<string | null>(null);
  const loaded = !!path && loadedPath === path;
  const [loadError, setLoadError] = useState(false);
  useEffect(() => {
    let active = true;
    setLoadedPath(null);
    setSnapshots([]);
    setName('');
    setLoadError(false);
    setBusy(false);
    if (path)
      invoke<Snapshot[]>('get_photo_snapshots', { path })
        .then((items) => {
          if (active) {
            setSnapshots(items);
            setLoadedPath(path);
          }
        })
        .catch((error) => {
          if (active) {
            setLoadError(true);
            toast.error(t('lightroomWorkspace.loadFailed', { error: String(error) }));
          }
        });
    return () => {
      active = false;
    };
  }, [path, t]);
  const persist = async (next: Snapshot[]) => {
    if (!path || !loaded || useEditorStore.getState().selectedImage?.path !== path || readOnly || busy || loadError)
      return;
    setBusy(true);
    try {
      await invoke('save_photo_snapshots', { path, snapshots: next });
      if (useEditorStore.getState().selectedImage?.path === path) {
        setSnapshots(next);
        setName('');
      }
    } catch (error) {
      toast.error(t('lightroomWorkspace.saveFailed', { error: String(error) }));
    } finally {
      if (useEditorStore.getState().selectedImage?.path === path) setBusy(false);
    }
  };
  const restore = (snapshot: Snapshot) => {
    if (!ready || useEditorStore.getState().selectedImage?.path !== path) return;
    debouncedSetHistory.flush();
    setAdjustments(() => normalizeLoadedAdjustments(snapshot.adjustments));
    debouncedSetHistory.flush();
  };
  return (
    <div className="h-full overflow-y-auto custom-scrollbar text-sm" data-workspace="lightroom-sidebar">
      <details open className="border-b border-surface">
        <summary className="px-3 py-2 font-semibold cursor-pointer">{t('lightroomWorkspace.navigator')}</summary>
        <div className="px-3 pb-3">
          {(finalPreviewUrl || thumbnail || selectedImage?.thumbnailUrl) && (
            <img
              src={finalPreviewUrl || thumbnail || selectedImage?.thumbnailUrl}
              alt="Navigator"
              className="w-full h-36 object-contain bg-black/30"
            />
          )}
          <div className="flex justify-between gap-2 mt-2">
            <button onClick={() => onZoom(0, true)} disabled={!ready}>
              {t('lightroomWorkspace.fit')}
            </button>
            <button onClick={() => onZoom(1)} disabled={!ready}>
              100%
            </button>
            <button onClick={() => onZoom(2)} disabled={!ready}>
              200%
            </button>
          </div>
        </div>
      </details>
      <details open className="border-b border-surface">
        <summary className="px-3 py-2 font-semibold cursor-pointer">{t('lightroomWorkspace.presets')}</summary>
        <div className="h-80">
          <PresetsPanel onNavigateToCommunity={onNavigateToCommunity} />
        </div>
      </details>
      <details className="border-b border-surface">
        <summary className="px-3 py-2 font-semibold cursor-pointer">{t('lightroomWorkspace.snapshots')}</summary>
        <div className="px-3 pb-3 space-y-2">
          {readOnly && <p className="text-xs text-text-secondary">{t('lightroomWorkspace.cardReadOnly')}</p>}
          <form
            className="flex gap-1"
            onSubmit={(event) => {
              event.preventDefault();
              if (ready && name.trim() && snapshots.length < 64)
                void persist([
                  ...snapshots,
                  { id: crypto.randomUUID(), name: name.trim(), adjustments: useEditorStore.getState().adjustments },
                ]);
            }}
          >
            <input
              aria-label={t('lightroomWorkspace.snapshotName')}
              maxLength={256}
              value={name}
              onChange={(event) => setName(event.target.value)}
              className="min-w-0 w-full rounded bg-surface px-2 py-1"
            />
            <button
              disabled={!ready || !loaded || readOnly || busy || loadError || !name.trim() || snapshots.length >= 64}
              className="disabled:opacity-40"
            >
              {t('lightroomWorkspace.save')}
            </button>
          </form>
          {snapshots.map((snapshot) => (
            <div key={snapshot.id} className="flex justify-between gap-2">
              <button disabled={!ready} onClick={() => restore(snapshot)} className="truncate text-left">
                {snapshot.name}
              </button>
              <button
                aria-label={t('lightroomWorkspace.deleteSnapshot', { name: snapshot.name })}
                disabled={!loaded || readOnly || busy || loadError}
                onClick={() => void persist(snapshots.filter((item) => item.id !== snapshot.id))}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      </details>
      <details open className="border-b border-surface">
        <summary className="px-3 py-2 font-semibold cursor-pointer">{t('lightroomWorkspace.history')}</summary>
        <div className="max-h-52 overflow-y-auto px-2 pb-2">
          {history
            .map((entry, index) => {
              const label =
                historyDetails[index]?.label ??
                (index === 0
                  ? t('lightroomWorkspace.opened')
                  : describeHistoryChange(history[index - 1], entry, 'user').label);
              return (
                <button
                  key={index}
                  disabled={!ready}
                  aria-current={index === historyIndex ? 'step' : undefined}
                  className={`block w-full truncate text-left px-2 py-1 ${index === historyIndex ? 'bg-surface text-accent' : ''}`}
                  onClick={() => {
                    debouncedSetHistory.flush();
                    const state = useEditorStore.getState();
                    const current = state.history.indexOf(entry);
                    if (current >= 0) state.goToHistoryIndex(current);
                  }}
                >
                  {label}
                </button>
              );
            })
            .reverse()}
        </div>
      </details>
      <details className="border-b border-surface">
        <summary className="px-3 py-2 font-semibold cursor-pointer">{t('lightroomWorkspace.collections')}</summary>
        <div className="h-64">{children}</div>
      </details>
    </div>
  );
}
