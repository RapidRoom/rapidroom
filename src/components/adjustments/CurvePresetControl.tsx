import { useMemo, useState } from 'react';
import { Edit, Save, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { usePresets, type UserPreset } from '../../hooks/usePresets';
import { debouncedSetHistory } from '../../hooks/useEditorActions';
import { useEditorStore } from '../../store/useEditorStore';
import type { Adjustments } from '../../utils/adjustments';
import { commitPresetApplication, preparePresetApplication } from '../../utils/presetApplication';
import type { Preset } from '../ui/AppProperties';
import Dropdown from '../ui/Dropdown';
import CreateFolderModal from '../modals/CreateFolderModal';
import RenameFolderModal from '../modals/RenameFolderModal';

interface CurvePresetControlProps {
  adjustments: Adjustments;
  setAdjustments(updater: (prev: Adjustments) => Adjustments): void;
}

export default function CurvePresetControl({ adjustments, setAdjustments }: CurvePresetControlProps) {
  const { t } = useTranslation();
  const isReady = useEditorStore((state) => state.selectedImage?.isReady ?? false);
  const { addCurvePreset, deleteItem, isLoading, presets, renameItem } = usePresets(adjustments);
  const [selectedPresetId, setSelectedPresetId] = useState<string | null>(null);
  const [isSaveOpen, setIsSaveOpen] = useState(false);
  const [isRenameOpen, setIsRenameOpen] = useState(false);

  const curvePresets = useMemo(
    () =>
      presets.flatMap((item: UserPreset) => {
        if (item.preset?.curvePreset) return [item.preset];
        return item.folder?.children.filter((preset) => preset.curvePreset) ?? [];
      }),
    [presets],
  );
  const selectedPreset = curvePresets.find((preset) => preset.id === selectedPresetId) ?? null;

  const applyPreset = (preset: Preset) => {
    debouncedSetHistory.flush();
    try {
      const plan = preparePresetApplication(preset, 100);
      commitPresetApplication(plan, (next) => setAdjustments(() => next));
      setSelectedPresetId(preset.id);
      debouncedSetHistory.flush();
    } catch (error) {
      toast.info(String(error instanceof Error ? error.message : error));
    }
  };

  const handleSave = async (name: string) => {
    const preset = await addCurvePreset(name);
    if (preset) setSelectedPresetId(preset.id);
  };

  const handleDelete = () => {
    if (!selectedPreset) return;
    deleteItem(selectedPreset.id);
    setSelectedPresetId(null);
  };

  return (
    <div className="flex items-center gap-1 mb-2">
      <Dropdown
        className="min-w-0 grow"
        disabled={!isReady || isLoading}
        onChange={(id) => {
          const preset = curvePresets.find((item) => item.id === id);
          if (preset) applyPreset(preset);
        }}
        options={curvePresets.map((preset) => ({ label: preset.name, value: preset.id }))}
        placeholder={t('adjustments.curves.presets.select')}
        triggerClassName="bg-surface-secondary"
        value={selectedPresetId}
      />
      <button
        aria-label={t('adjustments.curves.presets.save')}
        className="p-2 rounded-full hover:bg-surface-secondary text-text-secondary hover:text-text-primary disabled:opacity-40"
        data-tooltip={t('adjustments.curves.presets.save')}
        disabled={!isReady || isLoading}
        onClick={() => setIsSaveOpen(true)}
        type="button"
      >
        <Save size={16} />
      </button>
      {selectedPreset && (
        <>
          <button
            aria-label={t('adjustments.curves.presets.rename')}
            className="p-2 rounded-full hover:bg-surface-secondary text-text-secondary hover:text-text-primary"
            data-tooltip={t('adjustments.curves.presets.rename')}
            onClick={() => setIsRenameOpen(true)}
            type="button"
          >
            <Edit size={16} />
          </button>
          <button
            aria-label={t('adjustments.curves.presets.delete')}
            className="p-2 rounded-full hover:bg-surface-secondary text-text-secondary hover:text-text-primary"
            data-tooltip={t('adjustments.curves.presets.delete')}
            onClick={handleDelete}
            type="button"
          >
            <Trash2 size={16} />
          </button>
        </>
      )}
      <CreateFolderModal
        buttonText={t('adjustments.curves.presets.save')}
        isOpen={isSaveOpen}
        onClose={() => setIsSaveOpen(false)}
        onSave={handleSave}
        placeholder={t('adjustments.curves.presets.namePlaceholder')}
        title={t('adjustments.curves.presets.saveTitle')}
      />
      <RenameFolderModal
        buttonText={t('adjustments.curves.presets.rename')}
        currentName={selectedPreset?.name ?? ''}
        isOpen={isRenameOpen}
        onClose={() => setIsRenameOpen(false)}
        onSave={(name) => {
          if (selectedPreset) renameItem(selectedPreset.id, name);
        }}
        placeholder={t('adjustments.curves.presets.namePlaceholder')}
        title={t('adjustments.curves.presets.renameTitle')}
      />
    </div>
  );
}
