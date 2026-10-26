import { AlertTriangle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface EmbeddedPreviewWarningProps {
  isReady: boolean;
  usedEmbeddedPreview?: boolean;
}

export default function EmbeddedPreviewWarning({ isReady, usedEmbeddedPreview }: EmbeddedPreviewWarningProps) {
  const { t } = useTranslation();

  if (!isReady || !usedEmbeddedPreview) return null;

  return (
    <div
      role="alert"
      className="absolute top-3 left-3 right-3 z-40 flex items-center gap-2 rounded-md bg-amber-950/90 px-3 py-2 text-sm text-amber-50 shadow-lg pointer-events-none"
    >
      <AlertTriangle size={18} className="shrink-0" aria-hidden="true" />
      <span>{t('editor.embeddedPreviewWarning')}</span>
    </div>
  );
}
