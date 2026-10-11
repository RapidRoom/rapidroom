import { useTranslation } from 'react-i18next';
import { useEditorStore } from '../../../store/useEditorStore';
import { useLibraryStore } from '../../../store/useLibraryStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { useEditorActions, debouncedSetHistory } from '../../../hooks/useEditorActions';
import type { Adjustments } from '../../../utils/adjustments';
import Basic from '../../adjustments/Basic';
import { SliderDensityScope } from '../../ui/Slider';
import MetadataPanel from './MetadataPanel';

export default function LightroomLibrarySidebar() {
  const { t } = useTranslation();
  const selectedImage = useEditorStore((s) => s.selectedImage);
  const activePath = useLibraryStore((s) => s.libraryActivePath);
  const adjustments = useEditorStore((s) => s.adjustments);
  const historyIndex = useEditorStore((s) => s.historyIndex);
  const historyLength = useEditorStore((s) => s.history.length);
  const setEditor = useEditorStore((s) => s.setEditor);
  const appSettings = useSettingsStore((s) => s.appSettings);
  const { setAdjustments } = useEditorActions();
  const setScopedAdjustments = (value: Partial<Adjustments> | ((previous: Adjustments) => Adjustments)) => {
    if (
      useEditorStore.getState().selectedImage?.path !== selectedImage?.path ||
      useLibraryStore.getState().libraryActivePath !== selectedImage?.path
    )
      return;
    setAdjustments(value);
  };
  const ready = !!selectedImage?.isReady && selectedImage.path === activePath;
  return (
    <div className="h-full flex flex-col overflow-y-auto">
      <details open className="shrink-0 border-b border-surface">
        <summary className="px-3 py-2 font-semibold cursor-pointer">{t('lightroomWorkspace.quickDevelop')}</summary>
        <SliderDensityScope density="compact" className="px-3 pb-3">
          {ready ? (
            <>
              <div className="flex gap-3 mb-2">
                <button
                  disabled={historyIndex === 0}
                  onClick={() => {
                    debouncedSetHistory.flush();
                    useEditorStore.getState().undo();
                  }}
                >
                  {t('settings.keybinds.actions.undo')}
                </button>
                <button
                  disabled={historyIndex >= historyLength - 1}
                  onClick={() => {
                    debouncedSetHistory.flush();
                    useEditorStore.getState().redo();
                  }}
                >
                  {t('settings.keybinds.actions.redo')}
                </button>
              </div>
              <Basic
                adjustments={adjustments}
                appSettings={appSettings}
                setAdjustments={setScopedAdjustments}
                onDragStateChange={(dragging) => {
                  setEditor({ isSliderDragging: dragging });
                  if (!dragging) debouncedSetHistory.flush();
                }}
              />
            </>
          ) : (
            <p className="text-xs text-text-secondary">{t('lightroomWorkspace.selectPhoto')}</p>
          )}
        </SliderDensityScope>
      </details>
      <div className="min-h-96 grow">
        <MetadataPanel />
      </div>
    </div>
  );
}
