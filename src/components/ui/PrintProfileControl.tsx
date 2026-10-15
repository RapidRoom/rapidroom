import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { DEFAULT_PRINT_PROFILE, PrintProfileOptions } from '../../utils/printProfile';

interface Props {
  value: PrintProfileOptions | null;
  onChange(value: PrintProfileOptions | null): void;
  proof?: boolean;
  disabled?: boolean;
  renderError?: string | null;
}

export default function PrintProfileControl({ value, onChange, proof = false, disabled = false, renderError }: Props) {
  const { t } = useTranslation();
  const [chosen, setChosen] = useState<PrintProfileOptions>(value ?? DEFAULT_PRINT_PROFILE);
  const [error, setError] = useState<string | null>(null);
  const [draftPath, setDraftPath] = useState(value?.profilePath ?? '');
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  const options = value ?? chosen;

  async function validate(next: PrintProfileOptions, enable: boolean) {
    const id = ++request.current;
    setBusy(true);
    setError(null);
    try {
      await invoke('validate_print_profile', { options: next, proof });
      if (id !== request.current) return;
      setChosen(next);
      setDraftPath(next.profilePath);
      onChange(enable ? next : null);
    } catch (err) {
      if (id === request.current) setError(String(err));
    } finally {
      if (id === request.current) setBusy(false);
    }
  }

  async function choose() {
    try {
      const path = await open({ multiple: false, filters: [{ name: 'ICC profile', extensions: ['icc', 'icm'] }] });
      if (typeof path === 'string') await validate({ ...options, profilePath: path }, value !== null);
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <fieldset disabled={disabled || busy} className="space-y-2 text-xs" onKeyDown={(event) => event.stopPropagation()}>
      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={value !== null}
          disabled={!options.profilePath}
          onChange={(event) => {
            if (event.target.checked) void validate(options, true);
            else {
              request.current += 1;
              onChange(null);
              setError(null);
            }
          }}
        />
        {proof ? t('printProof.enabled') : t('printProof.exportEnabled')}
      </label>
      <button
        type="button"
        className="w-full px-2 py-1.5 rounded bg-surface hover:bg-card-active text-left truncate"
        onClick={() => void choose()}
        title={options.profilePath}
      >
        {options.profilePath.split(/[\\/]/).pop() || t('printProof.chooseProfile')}
      </button>
      <div className="flex gap-1">
        <input
          aria-label={t('printProof.profilePath')}
          className="min-w-0 flex-1 bg-surface rounded p-1"
          value={draftPath}
          onChange={(event) => setDraftPath(event.target.value)}
        />
        <button
          type="button"
          className="rounded bg-surface px-2 py-1"
          disabled={!draftPath.trim()}
          onClick={() => void validate({ ...options, profilePath: draftPath }, value !== null)}
        >
          {t('printProof.useProfile')}
        </button>
      </div>
      <label className="flex justify-between items-center gap-2">
        {t('printProof.intent')}
        <select
          className="bg-surface rounded p-1"
          value={options.intent}
          onChange={(event) =>
            void validate({ ...options, intent: event.target.value as PrintProfileOptions['intent'] }, value !== null)
          }
          disabled={!options.profilePath}
        >
          <option value="relative">{t('printProof.relative')}</option>
          <option value="perceptual">{t('printProof.perceptual')}</option>
        </select>
      </label>
      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={options.blackPointCompensation}
          disabled={!options.profilePath}
          onChange={(event) =>
            void validate({ ...options, blackPointCompensation: event.target.checked }, value !== null)
          }
        />
        {t('printProof.bpc')}
      </label>
      <p className="text-text-secondary">{proof ? t('printProof.displayAssumption') : t('printProof.exportFormats')}</p>
      {(error || renderError) && (
        <p role="alert" className="text-red-400 break-words">
          {error || renderError}
        </p>
      )}
    </fieldset>
  );
}
