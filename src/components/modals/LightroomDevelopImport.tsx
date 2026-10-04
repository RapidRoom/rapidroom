import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { Invokes } from '../ui/AppProperties';
import Switch from '../ui/Switch';
import Text from '../ui/Text';

export interface DevelopPhoto {
  id: number;
  path: string;
  copyName: string;
  virtualCopy: boolean;
  found: boolean;
  existingSidecar: boolean;
  existingEdits: boolean;
  rating: number | null;
  adjustments: Record<string, unknown>;
  unsupported: string[];
  error: string | null;
}
export interface DevelopPreview {
  catalogName: string;
  fingerprint: string;
  photos: DevelopPhoto[];
}
interface ImportResult {
  imported: number;
  preserved: number;
  virtualCopies: number;
  errors: string[];
}
interface Props {
  catalogPath: string;
  mappings: Record<string, string>;
  isCardMode: boolean;
  onBusyChange(busy: boolean): void;
  onImported(): Promise<void>;
  onClose(): void;
}
const PAGE_SIZE = 50;
const canImport = (photo: DevelopPhoto) => photo.found && !photo.error;
const readable = (name: string) => name.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/_/g, ' ');

// Keep the preview readable without exposing a JSON document as an import flow.
function adjustmentDescription(value: unknown): string {
  if (typeof value === 'number') return Number(value.toFixed(3)).toLocaleString();
  if (typeof value === 'string' || typeof value === 'boolean') return String(value);
  if (Array.isArray(value)) return value.map(adjustmentDescription).join('; ');
  if (value && typeof value === 'object') {
    return Object.entries(value)
      .map(([key, entry]) => `${readable(key)}: ${adjustmentDescription(entry)}`)
      .join(', ');
  }
  return '—';
}

export default function LightroomDevelopImport({
  catalogPath,
  mappings,
  isCardMode,
  onBusyChange,
  onImported,
  onClose,
}: Props) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<DevelopPreview | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [page, setPage] = useState(0);
  const [refresh, setRefresh] = useState(0);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [replaceEdits, setReplaceEdits] = useState(false);
  const [importRatings, setImportRatings] = useState(true);
  const [replaceRatings, setReplaceRatings] = useState(false);

  useEffect(() => {
    setResult(null);
    setApplyError(null);
  }, [catalogPath, mappings]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setPreview(null);
    setSelected(new Set());
    setPage(0);
    invoke<DevelopPreview>(Invokes.PreviewLightroomDevelop, { path: catalogPath, mappings })
      .then((next) => {
        if (cancelled) return;
        setPreview(next);
        setSelected(
          new Set(next.photos.filter((photo) => canImport(photo) && !photo.existingSidecar).map((photo) => photo.id)),
        );
        setError(null);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [catalogPath, mappings, refresh]);

  const handleImport = async () => {
    if (!preview || busy || loading || isCardMode || selected.size === 0) return;
    setBusy(true);
    onBusyChange(true);
    setApplyError(null);
    try {
      const next = await invoke<ImportResult>(Invokes.ImportLightroomDevelop, {
        path: catalogPath,
        mappings,
        fingerprint: preview.fingerprint,
        selections: [...selected],
        replaceEdits,
        importRatings,
        replaceRatings: importRatings && replaceRatings,
      });
      setResult(next);
      await onImported();
      if (next.errors.length === 0) {
        toast.success(t('modals.lightroomDevelop.result', { ...next }));
        onClose();
      }
    } catch (err) {
      setApplyError(String(err));
    } finally {
      setRefresh((value) => value + 1);
      setBusy(false);
      onBusyChange(false);
    }
  };
  const disabled = busy || loading || isCardMode;
  const pageCount = Math.max(1, Math.ceil((preview?.photos.length ?? 0) / PAGE_SIZE));
  const unsupportedLabel = (key: string) => t(`modals.lightroomDevelop.fields.${key}`, { defaultValue: readable(key) });

  return (
    <section className="space-y-3" aria-label={t('modals.lightroomDevelop.title')}>
      <Text>{t('modals.lightroomDevelop.hint')}</Text>
      <Text color="secondary">{t('modals.lightroomDevelop.preserved')}</Text>
      {isCardMode && <Text color="error">{t('modals.lightroomDevelop.cardMode')}</Text>}
      {error && <Text color="error">{error}</Text>}
      {applyError && (
        <Text color="error" role="alert">
          {applyError}
        </Text>
      )}
      {result && (
        <div role="status">
          <Text>{t('modals.lightroomDevelop.result', { ...result })}</Text>
          {result.errors.length > 0 && (
            <ul className="text-red-400 break-all">
              {result.errors.map((item, index) => (
                <li key={index}>{item}</li>
              ))}
            </ul>
          )}
        </div>
      )}
      <button
        className="px-3 py-1 rounded-md bg-bg-primary disabled:opacity-50"
        disabled={busy || loading}
        onClick={() => setRefresh((value) => value + 1)}
      >
        {t('modals.lightroomDevelop.refresh')}
      </button>
      {loading && <Text>{t('modals.lightroomDevelop.reading')}</Text>}
      {preview && (
        <>
          <div className="space-y-2">
            <Switch
              disabled={disabled}
              checked={replaceEdits}
              onChange={setReplaceEdits}
              label={t('modals.lightroomDevelop.replaceEdits')}
            />
            <Switch
              disabled={disabled}
              checked={importRatings}
              onChange={setImportRatings}
              label={t('modals.lightroomDevelop.importRatings')}
            />
            <Switch
              disabled={disabled || !importRatings}
              checked={replaceRatings}
              onChange={setReplaceRatings}
              label={t('modals.lightroomDevelop.replaceRatings')}
            />
          </div>
          <div className="flex flex-wrap gap-3 items-center">
            <button
              className="px-3 py-1 rounded-md bg-bg-primary disabled:opacity-50"
              disabled={disabled}
              onClick={() => setSelected(new Set(preview.photos.filter(canImport).map((photo) => photo.id)))}
            >
              {t('modals.lightroomDevelop.selectAvailable')}
            </button>
            <button
              className="px-3 py-1 rounded-md bg-bg-primary disabled:opacity-50"
              disabled={disabled}
              onClick={() => setSelected(new Set())}
            >
              {t('modals.lightroomDevelop.clearSelection')}
            </button>
            <Text>
              {t('modals.lightroomDevelop.selected', { selected: selected.size, total: preview.photos.length })}
            </Text>
          </div>
          {preview.photos.length === 0 && <Text>{t('modals.lightroomDevelop.empty')}</Text>}
          <ul className="space-y-2">
            {preview.photos.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE).map((photo) => (
              <li key={photo.id} className="bg-bg-primary rounded-md p-3 space-y-1">
                <label className="flex items-start gap-2 break-all">
                  <input
                    type="checkbox"
                    disabled={disabled || !canImport(photo)}
                    checked={selected.has(photo.id)}
                    onChange={(event) =>
                      setSelected((current) => {
                        const next = new Set(current);
                        if (event.target.checked) next.add(photo.id);
                        else next.delete(photo.id);
                        return next;
                      })
                    }
                  />
                  <span>
                    {photo.path.split('?vc=')[0]}
                    {photo.copyName && ` · ${photo.copyName}`}
                  </span>
                </label>
                <Text color="secondary">
                  {photo.existingSidecar
                    ? t('modals.lightroomDevelop.existing')
                    : t('modals.lightroomDevelop.newPhoto')}
                  {photo.virtualCopy && ` · ${t('modals.lightroomDevelop.virtualCopy')}`}
                  {' · '}
                  {photo.rating === null
                    ? t('modals.lightroomDevelop.noRating')
                    : t('modals.lightroomDevelop.rating', { rating: photo.rating })}
                </Text>
                {(!photo.found || photo.error) && (
                  <Text color="error">{photo.error ?? t('modals.lightroomDevelop.missing')}</Text>
                )}
                <details>
                  <summary className="cursor-pointer">{t('modals.lightroomDevelop.details')}</summary>
                  {Object.keys(photo.adjustments).length === 0 ? (
                    <Text>{t('modals.lightroomDevelop.noAdjustments')}</Text>
                  ) : (
                    <dl className="mt-2 space-y-1">
                      {Object.entries(photo.adjustments).map(([key, value]) => (
                        <div key={key} className="flex flex-wrap gap-2">
                          <dt className="capitalize font-medium">{readable(key)}</dt>
                          <dd>{adjustmentDescription(value)}</dd>
                        </div>
                      ))}
                    </dl>
                  )}
                </details>
                {photo.unsupported.length > 0 && (
                  <Text color="secondary">
                    {t('modals.lightroomDevelop.unsupported', {
                      items: photo.unsupported.map(unsupportedLabel).join(', '),
                    })}
                  </Text>
                )}
              </li>
            ))}
          </ul>
          {pageCount > 1 && (
            <div className="flex items-center gap-3">
              <button disabled={page === 0 || busy} onClick={() => setPage((current) => current - 1)}>
                {t('modals.lightroomDevelop.previous')}
              </button>
              <Text>{t('modals.lightroomDevelop.page', { page: page + 1, total: pageCount })}</Text>
              <button disabled={page + 1 >= pageCount || busy} onClick={() => setPage((current) => current + 1)}>
                {t('modals.lightroomDevelop.next')}
              </button>
            </div>
          )}
          <button
            className="px-4 py-2 rounded-md bg-accent text-button-text font-semibold disabled:opacity-50 disabled:cursor-not-allowed"
            disabled={disabled || selected.size === 0}
            onClick={handleImport}
          >
            {busy ? t('modals.lightroomImport.importing') : t('modals.lightroomDevelop.apply')}
          </button>
        </>
      )}
    </section>
  );
}
