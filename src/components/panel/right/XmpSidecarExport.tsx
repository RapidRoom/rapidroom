import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import Button from '../../ui/Button';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import type { Adjustments } from '../../../utils/adjustments';

interface NamedLoss {
  field: string;
  name: string;
  reason: string;
  preservedInPayload: boolean;
}

interface ExportReport {
  filePath: string;
  losses: NamedLoss[];
  written: boolean;
}

interface Prepared {
  owner: string;
  reference: Adjustments;
  draft: Adjustments;
  report: ExportReport;
}

export default function XmpSidecarExport({ disabled }: { disabled: boolean }) {
  const { t } = useTranslation();
  const activeView = useUIStore((state) => state.activeView);
  const owner = useEditorStore((state) => state.selectedImage?.path);
  const adjustments = useEditorStore((state) => state.adjustments);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [written, setWritten] = useState(false);
  const epoch = useRef(0);

  useEffect(() => {
    epoch.current += 1;
    setPrepared(null);
    setError(null);
    setWritten(false);
    setBusy(false);
    return () => {
      epoch.current += 1;
    };
  }, [owner, adjustments, activeView]);

  function current(photo: string, reference: Adjustments, request: number) {
    const state = useEditorStore.getState();
    return (
      epoch.current === request &&
      useUIStore.getState().activeView === 'editor' &&
      state.selectedImage?.path === photo &&
      state.adjustments === reference
    );
  }

  async function preview(customDestination = false) {
    if (activeView !== 'editor' || !owner || busy || disabled) return;
    const photo = owner;
    const reference = useEditorStore.getState().adjustments;
    const draft: Adjustments = structuredClone(reference);
    const request = ++epoch.current;
    setBusy(true);
    setPrepared(null);
    setWritten(false);
    setError(null);
    try {
      const [source, copy] = photo.split('?vc=');
      const suffix = copy ? `-copy-${copy.replace(/[^a-zA-Z0-9-]/g, '_').slice(0, 100)}.xmp` : '.xmp';
      const defaultPath = source.replace(/\.[^./\\]+$/, suffix);
      const filePath = customDestination
        ? await save({
            title: t('export.xmp.chooseDestination'),
            defaultPath,
            filters: [{ name: 'XMP', extensions: ['xmp'] }],
          })
        : defaultPath;
      if (!filePath || !current(photo, reference, request)) return;
      const report = await invoke<ExportReport>('export_xmp_round_trip', {
        imagePath: photo,
        filePath,
        adjustments: draft,
        write: false,
      });
      if (current(photo, reference, request)) setPrepared({ owner: photo, reference, draft, report });
    } catch (failure) {
      if (current(photo, reference, request)) setError(String(failure));
    } finally {
      if (epoch.current === request) setBusy(false);
    }
  }

  async function write() {
    if (!prepared || busy || disabled) return;
    const { owner: photo, reference, draft, report } = prepared;
    if (!current(photo, reference, epoch.current)) {
      setPrepared(null);
      return;
    }
    const request = ++epoch.current;
    setBusy(true);
    setError(null);
    try {
      await invoke<ExportReport>('export_xmp_round_trip', {
        imagePath: photo,
        filePath: report.filePath,
        adjustments: draft,
        write: true,
      });
      if (current(photo, reference, request)) {
        setPrepared(null);
        setWritten(true);
      }
    } catch (failure) {
      if (current(photo, reference, request)) setError(String(failure));
    } finally {
      if (epoch.current === request) setBusy(false);
    }
  }

  if (activeView !== 'editor') return null;

  return (
    <section aria-label={t('export.xmp.title')} className="space-y-3 rounded-lg border border-border p-3">
      <h3 className="text-sm font-semibold text-text-primary">{t('export.xmp.title')}</h3>
      <p className="text-xs text-text-secondary">{t('export.xmp.description')}</p>
      <Button onClick={() => preview()} disabled={disabled || busy || !owner} className="w-full bg-surface">
        {busy ? t('export.xmp.working') : t('export.xmp.preview')}
      </Button>
      <Button onClick={() => preview(true)} disabled={disabled || busy || !owner} className="w-full bg-surface">
        {t('export.xmp.chooseDestination')}
      </Button>
      {prepared && (
        <div className="space-y-3" aria-live="polite">
          <p className="break-all text-xs text-text-secondary">{prepared.report.filePath}</p>
          <ul className="space-y-2 text-xs">
            {prepared.report.losses.map((loss) => (
              <li key={loss.field}>
                <span className="font-semibold text-text-primary">{loss.name}</span>
                {loss.preservedInPayload && <span className="ml-2 text-text-secondary">{t('export.xmp.stored')}</span>}
                <p className="text-text-secondary">{loss.reason}</p>
              </li>
            ))}
          </ul>
          <Button onClick={write} disabled={disabled || busy} className="w-full">
            {t('export.xmp.write')}
          </Button>
        </div>
      )}
      {error && (
        <p role="alert" className="break-words text-xs text-red-400">
          {error}
        </p>
      )}
      {written && (
        <p role="status" className="text-xs text-text-primary">
          {t('export.xmp.written')}
        </p>
      )}
    </section>
  );
}
