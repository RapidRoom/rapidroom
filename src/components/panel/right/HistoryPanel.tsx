import { useContext, useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useEditorStore } from '../../../store/useEditorStore';
import { debouncedSetHistory } from '../../../hooks/useEditorActions';
import { DockHeaderContext } from '../DockHeaderContext';
import RenameFolderModal from '../../modals/RenameFolderModal';

export default function HistoryPanel() {
  const { t } = useTranslation();
  const editor = useEditorStore();
  const header = useContext(DockHeaderContext);
  const rows = useRef<HTMLDivElement>(null);
  const [naming, setNaming] = useState<{ id?: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const selected = editor.selectedImage?.isReady && editor.historyPath === editor.selectedImage.path;
  const editable = !!selected && editor.historyWritable;
  const later = editor.history.length - editor.historyIndex - 1;
  const time = (timestamp?: number | null) => (timestamp ? new Date(timestamp).toLocaleString() : '—');
  const actor = (value?: string) =>
    value === 'assistant'
      ? 'AI'
      : value === 'imported'
        ? 'LR'
        : value === 'system'
          ? t('history.system')
          : t('history.user');

  function navigate(event: KeyboardEvent<HTMLDivElement>) {
    if (!(event.target instanceof HTMLButtonElement) || !event.target.hasAttribute('data-history-row')) return;
    if (event.key === 'Delete' || event.key === 'Enter') {
      event.stopPropagation();
      if (event.key === 'Delete') event.preventDefault();
      return;
    }
    const buttons = [...(rows.current?.querySelectorAll<HTMLButtonElement>('[data-history-row]') ?? [])];
    const index = buttons.indexOf(event.target);
    const destination =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? buttons.length - 1
          : event.key === 'ArrowDown'
            ? Math.min(buttons.length - 1, index + 1)
            : event.key === 'ArrowUp'
              ? Math.max(0, index - 1)
              : -1;
    if (destination >= 0) {
      event.preventDefault();
      event.stopPropagation();
      buttons[destination]?.focus();
    }
  }

  return (
    <section className="flex flex-col h-full min-h-0 text-text-primary" aria-label={t('history.title')}>
      <div className="flex items-center gap-2 p-3 border-b border-surface">
        <h2 className="font-semibold flex-1">{t('history.title')}</h2>
        <button
          className="rounded px-2 py-1 hover:bg-surface disabled:opacity-40"
          disabled={!editable || editor.historySnapshots.length >= 20}
          onClick={() => {
            debouncedSetHistory.flush();
            setNaming({ name: '' });
          }}
        >
          {t('history.addSnapshot')}
        </button>
        {header && (
          <button onClick={header.onToggle} aria-label={t('history.close')}>
            ×
          </button>
        )}
      </div>
      {editor.historyWarning && (
        <p role="alert" className="m-3 text-sm text-amber-400">
          {editor.historyWarning}
        </p>
      )}
      {!selected ? (
        <p className="p-3 text-text-secondary">{t('history.selectPhoto')}</p>
      ) : (
        <div ref={rows} className="overflow-auto custom-scrollbar flex-1" onKeyDown={navigate}>
          <h3 className="px-3 pt-3 text-xs text-text-secondary uppercase">{t('history.snapshots')}</h3>
          {[...editor.historySnapshots].reverse().map((snapshot) => (
            <div key={snapshot.id} className="flex items-center gap-1 px-2">
              <button
                data-history-row
                className="flex-1 text-left rounded p-2 hover:bg-surface min-w-0"
                title={time(snapshot.createdAt)}
                disabled={!editable}
                onClick={() => {
                  debouncedSetHistory.flush();
                  editor.restoreHistorySnapshot(snapshot.id);
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Delete' && editable) {
                    event.preventDefault();
                    event.stopPropagation();
                    setDeleting(snapshot.id);
                  }
                }}
              >
                <span aria-hidden>{'◈ '}</span>
                {snapshot.name}
              </button>
              <details className="relative">
                <summary aria-label={t('history.snapshotActions')} className="cursor-pointer p-2">
                  {'⋯'}
                </summary>
                <div className="absolute right-0 z-10 bg-surface shadow-lg rounded p-1">
                  <button
                    className="block p-2"
                    disabled={!editable}
                    onClick={() => setNaming({ id: snapshot.id, name: snapshot.name })}
                  >
                    {t('history.rename')}
                  </button>
                  <button className="block p-2" disabled={!editable} onClick={() => setDeleting(snapshot.id)}>
                    {t('history.delete')}
                  </button>
                </div>
              </details>
            </div>
          ))}
          <h3 className="px-3 py-2 text-xs text-text-secondary uppercase">{t('history.steps')}</h3>
          {[...editor.history]
            .map((_, index) => index)
            .reverse()
            .map((index) => {
              const details = editor.historyDetails[index];
              const current = index === editor.historyIndex;
              const imported = details?.actor === 'imported';
              return (
                <button
                  key={details?.id ?? index}
                  data-history-row
                  aria-current={current ? 'step' : undefined}
                  className={`w-full grid grid-cols-[1rem_minmax(0,1fr)_3.5rem_3.5rem_1.5rem] items-start gap-2 text-left px-3 py-2 hover:bg-surface ${current ? 'bg-surface' : ''}`}
                  title={`${actor(details?.actor)} · ${time(details?.timestamp)} · ${details?.label ?? t('history.initial')}`}
                  onClick={() => {
                    if (!imported) {
                      debouncedSetHistory.flush();
                      const latest = useEditorStore.getState();
                      const target = details?.id
                        ? latest.historyDetails.findIndex((step) => step?.id === details.id)
                        : index;
                      latest.goToHistoryIndex(target);
                    }
                  }}
                >
                  <span aria-hidden>{current ? '●' : imported ? '◇' : '○'}</span>
                  <span className="min-w-0 break-words text-sm">
                    {details?.display?.name ?? details?.label ?? t('history.initial')}
                    <span className="block text-xs text-text-secondary">{time(details?.timestamp)}</span>
                  </span>
                  <span className="text-right tabular-nums text-sm text-text-secondary">{details?.display?.delta}</span>
                  <span className="text-right tabular-nums text-sm">{details?.display?.value}</span>
                  <span className="text-xs text-text-secondary">
                    {details?.actor === 'assistant' ? 'AI' : imported ? 'LR' : ''}
                  </span>
                </button>
              );
            })}
          {later > 0 && <p className="p-3 text-xs text-text-secondary">{t('history.branch', { count: later })}</p>}
          {editor.historyCompacted > 0 && (
            <p className="p-3 text-xs text-text-secondary">
              {t('history.compacted', { count: editor.historyCompacted })}
            </p>
          )}
        </div>
      )}
      <RenameFolderModal
        isOpen={naming !== null}
        currentName={naming?.name ?? ''}
        onClose={() => setNaming(null)}
        title={t(naming?.id ? 'history.rename' : 'history.addSnapshot')}
        placeholder={t('history.snapshotName')}
        buttonText={t('history.save')}
        onSave={(name) =>
          naming?.id ? editor.renameHistorySnapshot(naming.id, name) : editor.createHistorySnapshot(name)
        }
      />
      {deleting && (
        <div
          role="alertdialog"
          aria-modal="true"
          aria-label={t('history.delete')}
          className="fixed inset-0 z-50 bg-black/30 flex items-center justify-center"
          onKeyDown={(event) => {
            event.stopPropagation();
            if (event.key === 'Escape') setDeleting(null);
          }}
        >
          <div className="bg-surface p-6 rounded shadow-xl">
            <p>{t('history.confirmDelete')}</p>
            <div className="flex gap-3 mt-4">
              <button autoFocus onClick={() => setDeleting(null)}>
                {t('history.cancel')}
              </button>
              <button
                onClick={() => {
                  editor.deleteHistorySnapshot(deleting);
                  setDeleting(null);
                }}
              >
                {t('history.delete')}
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
