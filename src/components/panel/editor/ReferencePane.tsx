import { Replace, Trash2, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useEditorStore } from '../../../store/useEditorStore';

// Pinned sources share upstream's compare canvas; this only controls source selection.
export default function ReferencePane() {
  const { t } = useTranslation();
  const state = useEditorStore((s) => s.referenceView);
  const preview = useEditorStore((s) => s.originalPreview);
  const dispatch = useEditorStore((s) => s.dispatchReferenceView);
  return (
    <div
      className="lights-out-chrome flex items-center gap-2 bg-surface px-3 py-1 text-xs text-text-primary"
      data-testid="editor-reference-source"
    >
      <span>{state.isChooserOpen ? t('editor.referenceView.chooseHint') : state.reference?.label}</span>
      {preview?.sourcePath === state.reference?.path && preview?.error && (
        <span role="status">{t('editor.referenceView.unavailable')}</span>
      )}
      <span className="text-text-secondary">{t('editor.referenceView.readOnly')}</span>
      {state.reference && (
        <>
          <button aria-label={t('editor.referenceView.replace')} onClick={() => dispatch({ type: 'open-chooser' })}>
            <Replace size={14} />
          </button>
          <button aria-label={t('editor.referenceView.clear')} onClick={() => dispatch({ type: 'clear-reference' })}>
            <Trash2 size={14} />
          </button>
        </>
      )}
      <button aria-label={t('editor.referenceView.close')} onClick={() => dispatch({ type: 'exit' })}>
        <X size={14} />
      </button>
    </div>
  );
}
