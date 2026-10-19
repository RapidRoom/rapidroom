import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '../../store/useSettingsStore';
import {
  captureFolder,
  displayCaptureClock,
  folderCaptureShift,
  formatClockOffset,
  type CaptureExif,
} from '../../utils/captureTime';
import Text from './Text';
import { TextColors, TextVariants } from '../../types/typography';

interface Props {
  path: string;
  exif: CaptureExif;
  editable?: boolean;
}

export default function CaptureTime({ path, exif, editable = false }: Props) {
  const { t } = useTranslation();
  const shifts = useSettingsStore((s) => s.appSettings?.folderCaptureTimeShiftMinutes);
  const shift = folderCaptureShift(path, shifts);
  const clock = displayCaptureClock(exif, shift);
  const [open, setOpen] = useState(false);
  const [hours, setHours] = useState('0');
  const [editFolder, setEditFolder] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const folder = captureFolder(path);
  const minutes = Number(hours) * 60;
  const valid =
    hours.trim() !== '' &&
    Number.isFinite(minutes) &&
    Math.abs(minutes) <= 2880 &&
    Math.abs(minutes - Math.round(minutes)) < 1e-7;

  useEffect(() => {
    setOpen(false);
  }, [folder]);

  const save = async () => {
    if (!valid || saving || editFolder !== folder) return;
    const { appSettings, handleSettingsChange } = useSettingsStore.getState();
    if (!appSettings) return;
    setSaving(true);
    try {
      const values = { ...appSettings.folderCaptureTimeShiftMinutes };
      if (Math.round(minutes)) values[folder] = Math.round(minutes);
      else delete values[folder];
      await handleSettingsChange({ ...appSettings, folderCaptureTimeShiftMinutes: values });
      setOpen(false);
    } finally {
      setSaving(false);
    }
  };

  if (!clock)
    return (
      <Text variant={TextVariants.small} color={TextColors.secondary}>
        {exif?.DateTimeOriginal || '-'}
      </Text>
    );
  return (
    <div className="flex flex-col gap-1">
      <Text variant={TextVariants.small} color={TextColors.secondary} data-testid="capture-time">
        {clock.date} {clock.time}
        {!shift && clock.zone ? ` ${clock.zone}` : ''}
        {!!shift && ` · ${t('captureTime.shifted', { shift: formatClockOffset(shift) })}`}
      </Text>
      {!!shift && clock.zone && (
        <Text variant={TextVariants.small} color={TextColors.secondary}>
          {t('captureTime.cameraZone', { zone: clock.zone })}
        </Text>
      )}
      {editable && (
        <button
          className="text-xs text-accent hover:underline text-left"
          onClick={() => {
            setHours(String(shift / 60));
            setEditFolder(folder);
            setOpen(true);
          }}
        >
          {t('captureTime.folderShift')}
        </button>
      )}
      {open &&
        createPortal(
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="capture-time-title"
            className="fixed inset-0 z-100 flex items-center justify-center bg-black/40"
            onKeyDown={(event) => {
              event.stopPropagation();
              if (event.key === 'Escape') setOpen(false);
            }}
          >
            <form
              className="bg-surface border border-border rounded-xl p-6 w-full max-w-md space-y-4 shadow-xl"
              onSubmit={(event) => {
                event.preventDefault();
                void save();
              }}
            >
              <h2 id="capture-time-title" className="text-lg font-semibold">
                {t('captureTime.folderShift')}
              </h2>
              <p className="text-sm break-all">{folder}</p>
              <p className="text-sm text-text-secondary">{t('captureTime.description')}</p>
              <label className="flex flex-col gap-2 text-sm">
                {t('captureTime.hours')}
                <input
                  autoFocus
                  aria-label={t('captureTime.hours')}
                  type="number"
                  min="-48"
                  max="48"
                  step="any"
                  value={hours}
                  onChange={(event) => setHours(event.target.value)}
                  className="bg-bg-primary border border-border rounded px-3 py-2"
                />
              </label>
              <p className="text-sm text-text-secondary">{t('captureTime.example')}</p>
              <div className="flex justify-end gap-3">
                <button type="button" className="px-3 py-2 rounded hover:bg-bg-primary" onClick={() => setOpen(false)}>
                  {t('captureTime.cancel')}
                </button>
                <button
                  type="submit"
                  disabled={!valid || saving}
                  className="px-3 py-2 rounded bg-accent text-button-text disabled:opacity-40"
                >
                  {t('captureTime.save')}
                </button>
              </div>
            </form>
          </div>,
          document.body,
        )}
    </div>
  );
}
