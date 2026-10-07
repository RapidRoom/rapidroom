import { useTranslation } from 'react-i18next';
import { useUIStore } from '../../store/useUIStore';

export interface XmpDenoiseRequest {
  path: string;
  amount: number | null;
}

interface Props {
  requests: XmpDenoiseRequest[];
  onReview: (paths: string[]) => void;
}

export default function LightroomDenoiseOffer({ requests, onReview }: Props) {
  const { t } = useTranslation();
  const busy = useUIStore((state) => state.denoiseModalState.isOpen || state.denoiseModalState.isProcessing);
  const paths = [...new Set(requests.map(({ path }) => path))];
  if (paths.length === 0) return null;
  const amount = requests.length === 1 ? requests[0].amount : null;
  return (
    <div className="space-y-2">
      <p>{t('contextMenus.xmpImportReport.denoisePending', { count: paths.length })}</p>
      {amount !== null && <p>{t('contextMenus.xmpImportReport.denoiseAmount', { amount })}</p>}
      <p>{t('contextMenus.xmpImportReport.denoiseCalibration')}</p>
      <button
        type="button"
        className="rounded bg-accent px-3 py-1.5 text-button-text hover:opacity-90 disabled:opacity-50"
        disabled={busy}
        onClick={() => onReview(paths)}
      >
        {t('contextMenus.xmpImportReport.reviewBestDenoise')}
      </button>
    </div>
  );
}
