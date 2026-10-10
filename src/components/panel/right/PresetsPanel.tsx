import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
} from '@dnd-kit/core';
import { useTranslation } from 'react-i18next';
import {
  PresetImportFailure,
  PresetImportWarning,
  PresetListType,
  usePresets,
  UserPreset,
} from '../../../hooks/usePresets';
import { useContextMenu } from '../../../context/ContextMenuContext';
import {
  CopyPlus,
  Edit,
  FileDown,
  FileUp,
  Folder as FolderIcon,
  FolderOpen,
  FolderPlus,
  Loader2,
  Plus,
  SortAsc,
  Trash2,
  Users,
  Layers,
  Crop,
  Save,
  Wrench,
  Star,
  Settings2,
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import ConfigurePresetModal from '../../modals/ConfigurePresetModal';
import CreateFolderModal from '../../modals/CreateFolderModal';
import RenameFolderModal from '../../modals/RenameFolderModal';
import Button from '../../ui/Button';
import Text from '../../ui/Text';
import Slider from '../../ui/Slider';
import { TextColors, TextVariants, TextWeights } from '../../../types/typography';
import { Adjustments, ADJUSTMENT_GROUPS } from '../../../utils/adjustments';
import { OPTION_SEPARATOR, Panel, Preset, SelectedImage } from '../../ui/AppProperties';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import { debouncedSetHistory, useEditorActions } from '../../../hooks/useEditorActions';
import { mixAdjustments } from '../../../utils/presetAmount';
import { presetUnavailableReason, filterPresetLibrary } from '../../../utils/presetBrowser';
import { clearPresetHoverPreview, showPresetHoverPreview } from '../../../utils/presetHoverPreview';

interface DroppableFolderItemProps {
  children: any;
  folder: any;
  isExpanded: boolean;
  isVirtual?: boolean;
  onContextMenu(event: any, folder: any): void;
  onToggle(id: string): void;
}

interface DraggablePresetItemProps {
  onApply(preset: any): void;
  onContextMenu(event: any, preset: any): void;
  onToggleFavorite(): void;
  preset: any;
  dndId?: string;
  disabled?: boolean;
  isActive?: boolean;
  intensity?: number;
  onIntensityChange?: (val: number) => void;
  onDragStateChange?: (isDragging: boolean) => void;
  onHoverChange?: (preset: Preset | null) => void;
}

interface FolderProps {
  folder: any;
}

interface FolderState {
  isOpen: boolean;
  folder: any;
}

interface ModalState {
  isOpen: boolean;
  preset: Preset | null;
}

interface PresetItemDisplayProps {
  preset: Preset;
  onToggleFavorite?: () => void;
  isActive?: boolean;
  intensity?: number;
  onIntensityChange?: (val: number) => void;
  onDragStateChange?: (isDragging: boolean) => void;
}

interface PresetsPanelProps {
  onNavigateToCommunity(): void;
}

const FAVORITES_FOLDER_ID = 'favorites';

const itemVariants = {
  hidden: { opacity: 0, x: -15 },
  visible: (i: number) => ({
    opacity: 1,
    x: 0,
    transition: {
      duration: 0.25,
      delay: i * 0.05,
    },
  }),
  exit: { opacity: 0, x: -15, transition: { duration: 0.2 } },
};

// Rows are dnd-kit drag handles without a keyboard sensor, so Enter and Space activate them instead.
const handleActivationKey = (event: React.KeyboardEvent, activate: () => void) => {
  if (event.key !== 'Enter' && event.key !== ' ') return;
  if ((event.target as HTMLElement).closest('button, input, textarea, select')) return;
  event.preventDefault();
  activate();
};

function PresetItemDisplay({
  preset,
  onToggleFavorite,
  isActive,
  intensity,
  onIntensityChange,
  onDragStateChange,
}: PresetItemDisplayProps) {
  const { t } = useTranslation();
  const geometryKeys = ADJUSTMENT_GROUPS.geometry.flatMap((g) => g.keys);

  const supportsMasks = preset.includeMasks ?? (preset.adjustments?.masks && preset.adjustments.masks.length > 0);
  const supportsGeometry =
    preset.includeCropTransform ?? geometryKeys.some((key) => preset.adjustments?.[key] !== undefined);
  const isTool = preset.presetType === 'tool';
  const tooltipContent = useMemo(() => {
    const features = [];
    if (supportsMasks) features.push(t('editor.presets.supports.masks'));
    if (supportsGeometry) features.push(t('editor.presets.supports.cropTransform'));

    if (features.length === 0) return undefined;
    return t('editor.presets.supports.label', { features: features.join(' + ') });
  }, [supportsMasks, supportsGeometry, t]);

  return (
    <div className="group flex flex-col px-2.5 py-1.5 rounded-md bg-surface hover:bg-surface-hover transition-colors cursor-grabbing">
      <div className="flex items-center gap-2">
        <Text color={TextColors.primary} className="grow min-w-0 truncate">
          {preset.name}
        </Text>
        {(isTool || supportsMasks || supportsGeometry) && (
          <div className="flex items-center gap-1.5 shrink-0 text-text-secondary" data-tooltip={tooltipContent}>
            {isTool && <Wrench size={12} aria-label={t('editor.presets.types.tool')} />}
            {supportsMasks && <Layers size={12} />}
            {supportsGeometry && <Crop size={12} />}
          </div>
        )}
        {(onToggleFavorite || preset.favorite) && (
          <button
            type="button"
            aria-label={
              preset.favorite ? t('editor.presets.tooltips.unfavorite') : t('editor.presets.tooltips.favorite')
            }
            className={`shrink-0 self-stretch -my-1.5 -mr-2.5 px-2.5 flex items-center cursor-pointer text-text-secondary hover:text-text-primary transition-opacity ${
              preset.favorite ? '' : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'
            }`}
            onClick={(e) => {
              e.stopPropagation();
              onToggleFavorite?.();
            }}
            onPointerDown={(e) => e.stopPropagation()}
          >
            <Star size={12} className={preset.favorite ? 'fill-current text-primary' : ''} />
          </button>
        )}
      </div>

      <AnimatePresence initial={false}>
        {isActive && onIntensityChange && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.25, ease: 'easeInOut' }}
            className="w-full cursor-auto overflow-hidden"
            onClick={(e: any) => e.stopPropagation()}
            onPointerDown={(e: any) => e.stopPropagation()}
          >
            <div className="mt-2 pb-1">
              <Slider
                min={0}
                max={200}
                defaultValue={100}
                value={intensity ?? 100}
                onChange={(e: any) => onIntensityChange(Number(e.target.value))}
                onDragStateChange={onDragStateChange}
                label={t('editor.presets.amount')}
                step={1}
              />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function FolderItemDisplay({ folder }: FolderProps) {
  return (
    <div className="flex items-center gap-2 p-2 rounded-lg bg-surface cursor-grabbing w-full">
      <div className="p-1">
        <FolderIcon size={18} />
      </div>
      <Text color={TextColors.primary} weight={TextWeights.medium} className="grow truncate select-none">
        {folder.name}
      </Text>
      <Text as="span" weight={TextWeights.medium} className="ml-auto pr-1">
        {folder.children?.length || 0}
      </Text>
    </div>
  );
}

function DraggablePresetItem({
  preset,
  onApply,
  onContextMenu,
  onToggleFavorite,
  dndId,
  disabled,
  isActive,
  intensity,
  onIntensityChange,
  onDragStateChange,
  onHoverChange,
}: DraggablePresetItemProps) {
  const {
    attributes,
    listeners,
    setNodeRef: setDraggableNodeRef,
    isDragging,
  } = useDraggable({
    id: dndId ?? preset.id,
    data: { type: PresetListType.Preset, preset },
    disabled,
  });

  const { setNodeRef: setDroppableNodeRef, isOver } = useDroppable({
    data: { type: PresetListType.Preset, preset },
    id: dndId ?? preset.id,
    disabled,
  });

  const setCombinedRef = useCallback(
    (node: any) => {
      setDraggableNodeRef(node);
      setDroppableNodeRef(node);
    },
    [setDraggableNodeRef, setDroppableNodeRef],
  );

  const cameraModel = useEditorStore((s) => s.selectedImage?.exif?.Model);
  const unavailable = presetUnavailableReason(preset, cameraModel);
  const style = {
    borderRadius: '6px',
    opacity: isDragging || unavailable ? 0.4 : 1,
    outline: isOver ? '2px solid var(--color-primary)' : '2px solid transparent',
    outlineOffset: '-2px',
    touchAction: 'none',
  };

  return (
    <div
      title={unavailable || undefined}
      aria-disabled={!!unavailable}
      onClick={() => !unavailable && onApply(preset)}
      onKeyDown={(e) => handleActivationKey(e, () => !unavailable && onApply(preset))}
      onContextMenu={(e: any) => onContextMenu(e, { preset })}
      onMouseEnter={() => onHoverChange?.(preset)}
      onMouseLeave={() => onHoverChange?.(null)}
      ref={setCombinedRef}
      style={style}
    >
      <motion.div
        {...listeners}
        {...attributes}
        className="cursor-grab"
        whileTap={{ scale: isActive ? 1 : 0.98 }}
        transition={{ type: 'spring', stiffness: 400, damping: 17 }}
      >
        <PresetItemDisplay
          preset={preset}
          onToggleFavorite={onToggleFavorite}
          isActive={isActive}
          intensity={intensity}
          onIntensityChange={onIntensityChange}
          onDragStateChange={onDragStateChange}
        />
      </motion.div>
    </div>
  );
}

function DroppableFolderItem({
  folder,
  onContextMenu,
  children,
  onToggle,
  isExpanded,
  isVirtual,
}: DroppableFolderItemProps) {
  const {
    attributes,
    listeners,
    setNodeRef: setDraggableNodeRef,
    isDragging,
  } = useDraggable({
    data: { type: PresetListType.Folder, folder },
    id: folder.id,
    disabled: isVirtual,
  });

  const { setNodeRef: setDroppableNodeRef, isOver } = useDroppable({
    data: { type: PresetListType.Folder, folder },
    id: folder.id,
    disabled: isVirtual,
  });

  const style = {
    opacity: isDragging ? 0.4 : 1,
    touchAction: 'none',
  };

  const hasChildren = folder.children && folder.children.length > 0;
  const Icon = isVirtual ? Star : isExpanded ? FolderOpen : FolderIcon;

  return (
    <div
      className={`rounded-lg transition-colors ${isOver ? 'bg-surface-hover' : ''}`}
      ref={setDroppableNodeRef}
      style={style}
    >
      <div
        className={`flex items-center gap-2 p-2 rounded-lg bg-surface ${isVirtual ? 'cursor-pointer' : 'cursor-grab'}`}
        onClick={() => onToggle(folder.id)}
        onKeyDown={(e) => handleActivationKey(e, () => onToggle(folder.id))}
        onContextMenu={isVirtual ? undefined : (e: any) => onContextMenu(e, { folder })}
        ref={setDraggableNodeRef}
        {...listeners}
        {...attributes}
      >
        <Icon className={`m-1 shrink-0 ${isExpanded ? 'text-primary' : 'text-text-secondary'}`} size={18} />
        <Text color={TextColors.primary} weight={TextWeights.medium} className="grow truncate select-none">
          {folder.name}
        </Text>
        <Text as="span" variant={TextVariants.small} color={TextColors.secondary} className="ml-auto pr-1">
          {folder.children?.length || 0}
        </Text>
      </div>
      <AnimatePresence>
        {isExpanded && hasChildren && (
          <motion.div
            animate={{ height: 'auto', opacity: 1 }}
            className="ml-4 pl-2 border-l-[1.5px] border-border-color/50 space-y-1 overflow-hidden pt-1"
            exit={{ height: 0, opacity: 0 }}
            initial={{ height: 0, opacity: 0 }}
          >
            {children}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function RootDroppableArea({
  children,
  onContextMenu,
}: {
  children: React.ReactNode;
  onContextMenu: (e: any) => void;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: 'root' });

  return (
    <div
      className={`grow overflow-y-auto p-3 pr-0.5 [scrollbar-gutter:stable] space-y-1 rounded-lg transition-colors ${isOver ? 'bg-surface-hover' : ''}`}
      onContextMenu={onContextMenu}
      ref={setNodeRef}
    >
      {children}
    </div>
  );
}

export default function PresetsPanel({ onNavigateToCommunity }: PresetsPanelProps) {
  const { t } = useTranslation();
  const selectedImage = useEditorStore((s) => s.selectedImage);
  const adjustments = useEditorStore((s) => s.adjustments);
  const setEditor = useEditorStore((s) => s.setEditor);
  const { setAdjustments } = useEditorActions();

  const {
    addFolder,
    addPreset,
    configurePreset,
    deleteItem,
    duplicatePreset,
    exportPresetsToFile,
    importPresetsFromFiles,
    isLoading,
    movePreset,
    overwritePreset,
    presets,
    renameItem,
    reorderItems,
    sortAllPresetsAlphabetically,
    toggleFavorite,
  } = usePresets(adjustments);
  const { showContextMenu } = useContextMenu();
  const [configureModalState, setConfigureModalState] = useState<ModalState>({ isOpen: false, preset: null });
  const [isAddFolderModalOpen, setIsAddFolderModalOpen] = useState(false);
  const [renameFolderState, setRenameFolderState] = useState<FolderState>({ isOpen: false, folder: null });
  const [expandedFolders, setExpandedFolders] = useState(new Set<string>([FAVORITES_FOLDER_ID]));
  const [activeItem, setActiveItem] = useState<any>(null);
  const [deletingItemId, setDeletingItemId] = useState<string | null>(null);

  const [query, setQuery] = useState('');
  const [compatibleOnly, setCompatibleOnly] = useState(false);
  const exportNewXmp = useRef(false);
  const presetBase = useRef<Adjustments | null>(null);
  const lastPresetEdit = useRef<Adjustments | null>(null);

  const [activePresetId, setActivePresetId] = useState<string | null>(null);
  const [presetIntensity, setPresetIntensity] = useState<number>(100);
  const [isActivePresetExpanded, setIsActivePresetExpanded] = useState(true);

  const activeView = useUIStore((s) => s.activeView);

  // Hovering a preset renders it on the editor image via previewOverride, leaving adjustments untouched.
  const setHoverPreview = useCallback(
    (preset: Preset | null) => {
      if (
        preset &&
        !presetUnavailableReason(preset, selectedImage?.exif?.Model) &&
        preset.id !== activePresetId &&
        activeView === 'editor' &&
        selectedImage?.isReady
      ) {
        showPresetHoverPreview(mixAdjustments(preset.adjustments, 100, useEditorStore.getState().adjustments));
      } else {
        clearPresetHoverPreview();
      }
    },
    [activePresetId, activeView, selectedImage?.isReady, selectedImage?.exif?.Model],
  );

  useEffect(() => clearPresetHoverPreview, []);

  // Rows can unmount under the pointer without a mouseleave, so any of these changes ends the preview.
  useEffect(() => {
    clearPresetHoverPreview();
  }, [
    adjustments,
    activeView,
    presets,
    query,
    compatibleOnly,
    selectedImage?.path,
    selectedImage?.isReady,
    selectedImage?.exif?.Model,
  ]);

  useEffect(() => {
    if (configureModalState.isOpen || isAddFolderModalOpen || renameFolderState.isOpen) clearPresetHoverPreview();
  }, [configureModalState.isOpen, isAddFolderModalOpen, renameFolderState.isOpen]);

  useEffect(() => {
    window.addEventListener('blur', clearPresetHoverPreview);
    return () => window.removeEventListener('blur', clearPresetHoverPreview);
  }, []);

  const handleDragStateChange = useCallback(
    (isDragging: boolean) => {
      setEditor({ isSliderDragging: isDragging });
      if (!isDragging) debouncedSetHistory.flush();
    },
    [setEditor],
  );

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 10,
      },
    }),
  );

  const allItemsMap = useMemo(() => {
    const map = new Map();
    presets.forEach((item: any) => {
      if (item.preset) {
        map.set(item.preset.id, { type: PresetListType.Preset, data: item.preset });
      } else if (item.folder) {
        map.set(item.folder.id, { type: PresetListType.Folder, data: item.folder });
        item.folder.children.forEach((p: any) => map.set(p.id, { type: PresetListType.Preset, data: p }));
      }
    });
    return map;
  }, [presets]);

  const itemParentMap = useMemo(() => {
    const map = new Map();
    presets.forEach((item: UserPreset) => {
      if (item.preset) {
        map.set(item.preset.id, null);
      } else if (item.folder) {
        map.set(item.folder.id, null);
        item.folder.children.forEach((p: UserPreset) => {
          if (!item?.folder) {
            return;
          }
          map.set(p.id, item.folder.id);
        });
      }
    });
    return map;
  }, [presets]);

  const toggleFolder = (folderId: string) => {
    setExpandedFolders((prev: Set<string>) => {
      const newSet = new Set(prev);
      if (newSet.has(folderId)) {
        newSet.delete(folderId);
      } else {
        newSet.add(folderId);
      }
      return newSet;
    });
  };

  useEffect(() => {
    setActivePresetId(null);
    presetBase.current = null;
    lastPresetEdit.current = null;
  }, [selectedImage?.path, selectedImage?.isReady]);

  useEffect(() => {
    if (activePresetId && lastPresetEdit.current !== adjustments) {
      setActivePresetId(null);
      presetBase.current = null;
    }
  }, [adjustments, activePresetId]);

  const handleApplyPreset = (preset: Preset) => {
    if (!selectedImage?.isReady || presetUnavailableReason(preset, selectedImage.exif?.Model)) return;
    setHoverPreview(null);
    if (activePresetId === preset.id) {
      setIsActivePresetExpanded((expanded) => !expanded);
      return;
    }

    setActivePresetId(preset.id);
    setIsActivePresetExpanded(true);
    setPresetIntensity(100);

    debouncedSetHistory.flush();
    presetBase.current = useEditorStore.getState().adjustments;
    const next = { ...presetBase.current, ...mixAdjustments(preset.adjustments, 100, presetBase.current) };
    lastPresetEdit.current = next;
    setAdjustments(() => next);
    debouncedSetHistory.flush();
  };

  const handleIntensityChange = useCallback(
    (preset: Preset, intensity: number) => {
      setPresetIntensity(intensity);
      const base = presetBase.current;
      if (!base || useEditorStore.getState().adjustments !== lastPresetEdit.current) return;
      const mixed = mixAdjustments(preset.adjustments, intensity, base);
      const next = { ...base, ...mixed };
      lastPresetEdit.current = next;
      setAdjustments(() => next);
    },
    [setAdjustments],
  );

  const handleSaveConfiguredPreset = async (
    name: string,
    includeMasks: boolean,
    includeCropTransform: boolean,
    presetType: 'tool' | 'style',
  ) => {
    if (configureModalState.preset) {
      configurePreset(configureModalState.preset.id, name, includeMasks, includeCropTransform, presetType);
    } else {
      const preset = addPreset(name, null, includeMasks, includeCropTransform, presetType);
      if (exportNewXmp.current) {
        exportNewXmp.current = false;
        await handleExportXmp(preset);
      }
    }
    setConfigureModalState({ isOpen: false, preset: null });
  };

  const handleAddFolder = (name: string) => {
    addFolder(name);
    setIsAddFolderModalOpen(false);
  };

  const handleRenameFolderSave = (newName: string) => {
    if (renameFolderState.folder) {
      renameItem(renameFolderState.folder.id, newName);
    }
    setRenameFolderState({ isOpen: false, folder: null });
  };

  const handleDeleteItem = (id: string | null, isFolder = false) => {
    setDeletingItemId(id);
    if (!id) {
      return;
    }

    setTimeout(() => {
      deleteItem(id);
      if (isFolder) {
        setExpandedFolders((prev: Set<string>) => {
          const newSet = new Set(prev);
          newSet.delete(id);
          return newSet;
        });
      }
    }, 300);
  };

  const handleDragStart = (event: any) => {
    setHoverPreview(null);
    setActiveItem(allItemsMap.get(event.active.id) ?? null);
  };

  const handleDragEnd = (event: any) => {
    const { active, over } = event;
    setActiveItem(null);

    const activeId = active.id;
    const activeParentId = itemParentMap.get(activeId);
    const activeType = active.data.current?.type;

    if (!over) {
      if (activeParentId !== null) {
        movePreset(activeId, null, null);
      }
      return;
    }

    if (active.id === over.id) {
      return;
    }

    const overId = over.id;
    const overParentId = itemParentMap.get(overId);
    const overType = over.data.current?.type;

    const targetFolderId = overType === PresetListType.Folder ? overId : overParentId;

    if (activeType === PresetListType.Folder) {
      if (targetFolderId && targetFolderId !== activeId) {
        reorderItems(activeId, targetFolderId);
      }
      return;
    }

    if (activeType === PresetListType.Preset && targetFolderId) {
      if (activeParentId !== targetFolderId) {
        movePreset(activeId, targetFolderId);
        setExpandedFolders((prev: Set<string>) => new Set(prev).add(targetFolderId));
      } else {
        reorderItems(activeId, overId);
      }
      return;
    }

    if (activeParentId !== null && !targetFolderId) {
      movePreset(activeId, null, overId);
      return;
    }

    if (activeParentId === null && !targetFolderId) {
      reorderItems(activeId, overId);
      return;
    }
  };

  const handleImportPresets = async () => {
    try {
      const selectedPaths = await openDialog({
        filters: [
          { name: t('editor.presets.dialog.allPresetFiles'), extensions: ['rrpreset', 'xmp', 'lrtemplate'] },
          { name: t('editor.presets.dialog.rapidRawPreset'), extensions: ['rrpreset'] },
          { name: t('editor.presets.dialog.legacyPreset'), extensions: ['xmp', 'lrtemplate'] },
        ],
        multiple: true,
        title: t('editor.presets.dialog.importPresetsTitle'),
      });

      if (!selectedPaths) {
        return;
      }

      const paths = Array.isArray(selectedPaths) ? selectedPaths : [selectedPaths];
      if (paths.length === 0) {
        return;
      }

      const { failures, warnings } = await importPresetsFromFiles(paths);

      failures.forEach((failure: PresetImportFailure) =>
        console.error(`Failed to import ${failure.fileName}: ${failure.error}`),
      );
      warnings.forEach((warning: PresetImportWarning) => console.warn(`${warning.fileName}: ${warning.message}`));
    } catch (error) {
      console.error('Failed to import presets:', error);
    }
  };

  const handleExport = async (item: UserPreset) => {
    const isFolder = !!item.folder;
    const name = isFolder ? item.folder?.name : item.preset?.name;
    const itemsToExport = [item];

    try {
      const filePath = await saveDialog({
        defaultPath: `${name}.rrpreset`.replace(/[<>:"/\\|?*]/g, '_'),
        filters: [{ name: t('editor.presets.dialog.presetFile'), extensions: ['rrpreset'] }],
        title: t('editor.presets.dialog.exportTitle', {
          type: isFolder ? t('editor.presets.types.folder') : t('editor.presets.types.preset'),
        }),
      });

      if (filePath) {
        await exportPresetsToFile(itemsToExport, filePath);
      }
    } catch (error) {
      console.error(`Failed to export ${isFolder ? PresetListType.Folder : PresetListType.Preset}:`, error);
    }
  };

  const handleExportXmp = async (preset: Preset) => {
    try {
      const filePath = await saveDialog({
        defaultPath: `${preset.name.replace(/[<>:"/\\|?*]/g, '_')}.xmp`,
        filters: [{ name: t('editor.presets.lightroomXmp'), extensions: ['xmp'] }],
      });
      if (!filePath) return;
      const unsupported = await invoke<string[]>('export_lightroom_preset', { preset, filePath });
      if (unsupported.length) toast.info(t('editor.presets.xmpExportLimited', { keys: unsupported.join(', ') }));
      else toast.success(t('editor.presets.xmpSaved'));
    } catch (error) {
      toast.error(String(error));
    }
  };

  const presetCreationRequested = useUIStore((s) => s.presetCreationRequested);
  useEffect(() => {
    if (!presetCreationRequested || activeView !== 'editor' || !selectedImage?.isReady) return;
    clearPresetHoverPreview();
    exportNewXmp.current = true;
    setConfigureModalState({ isOpen: true, preset: null });
    useUIStore.getState().setUI({ presetCreationRequested: false });
  }, [presetCreationRequested, activeView, selectedImage?.isReady]);

  const handleExportAllPresets = async () => {
    if (presets.length === 0) {
      return;
    }
    try {
      const filePath = await saveDialog({
        defaultPath: 'all_presets.rrpreset',
        filters: [{ name: t('editor.presets.dialog.presetFile'), extensions: ['rrpreset'] }],
        title: t('editor.presets.dialog.exportAllTitle'),
      });

      if (filePath) {
        await exportPresetsToFile(presets, filePath);
      }
    } catch (error) {
      console.error('Failed to export all presets:', error);
    }
  };

  const handleContextMenu = (event: any, item: UserPreset) => {
    event.preventDefault();
    clearPresetHoverPreview();
    event.stopPropagation();

    const isFolder = !!item.folder;
    const data = isFolder ? item.folder : item.preset;

    let options = [];
    if (isFolder) {
      options = [
        {
          icon: Edit,
          label: t('editor.presets.menu.renameFolder'),
          onClick: () => setRenameFolderState({ isOpen: true, folder: data }),
        },
        {
          icon: FileDown,
          label: t('editor.presets.menu.exportFolder'),
          onClick: () => handleExport(item),
        },
        { type: OPTION_SEPARATOR },
        {
          icon: Trash2,
          isDestructive: true,
          label: t('editor.presets.menu.deleteFolder'),
          onClick: () => handleDeleteItem(data?.id ?? null, true),
        },
      ];
    } else {
      options = [
        {
          icon: Save,
          label: t('editor.presets.menu.overwrite'),
          onClick: () => overwritePreset(data?.id ?? null),
        },
        {
          icon: Settings2,
          label: t('editor.presets.menu.configurePreset'),
          onClick: () => setConfigureModalState({ isOpen: true, preset: data as Preset }),
        },
        { type: OPTION_SEPARATOR },
        {
          icon: CopyPlus,
          label: t('editor.presets.menu.duplicatePreset'),
          onClick: () => duplicatePreset(data?.id ?? null),
        },
        {
          icon: FileDown,
          label: t('editor.presets.menu.exportPreset'),
          onClick: () => handleExport(item),
        },
        { label: t('editor.presets.lightroomXmp'), onClick: () => item.preset && handleExportXmp(item.preset) },
        { type: OPTION_SEPARATOR },
        {
          icon: Trash2,
          isDestructive: true,
          label: t('editor.presets.menu.deletePreset'),
          onClick: () => handleDeleteItem(data?.id ?? null, false),
        },
      ];
    }

    showContextMenu(event.clientX, event.clientY, options);
  };

  const handleBackgroundContextMenu = (event: any) => {
    if (!event.currentTarget.contains(event.target)) {
      return;
    }
    event.preventDefault();
    clearPresetHoverPreview();
    const options = [
      {
        icon: Plus,
        label: t('editor.presets.menu.newPreset'),
        onClick: () => setConfigureModalState({ isOpen: true, preset: null }),
      },
      {
        icon: FolderPlus,
        label: t('editor.presets.menu.newFolder'),
        onClick: () => setIsAddFolderModalOpen(true),
      },
      { type: OPTION_SEPARATOR },
      {
        disabled: presets.length === 0,
        icon: SortAsc,
        label: t('editor.presets.menu.sortAll'),
        onClick: sortAllPresetsAlphabetically,
      },
    ];
    showContextMenu(event.clientX, event.clientY, options);
  };

  const visiblePresets = useMemo(
    () => filterPresetLibrary(presets, query, compatibleOnly, selectedImage?.exif?.Model),
    [presets, query, compatibleOnly, selectedImage?.exif?.Model],
  );
  const folders = useMemo(() => visiblePresets.filter((item: UserPreset) => item.folder), [visiblePresets]);
  const rootPresets = useMemo(() => visiblePresets.filter((item: UserPreset) => item.preset), [visiblePresets]);
  const favoritesFolder = useMemo(
    () => ({
      id: FAVORITES_FOLDER_ID,
      name: t('editor.presets.favorites'),
      children: visiblePresets
        .flatMap((item: UserPreset) => (item.folder ? item.folder.children : item.preset ? [item.preset] : []))
        .filter((p: Preset) => p.favorite && p.id !== deletingItemId),
    }),
    [visiblePresets, deletingItemId, t],
  );

  return (
    <DndContext id="presets-panel-dnd" sensors={sensors} onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
      <div className="flex flex-col h-full">
        <div className="p-3 flex justify-between items-center shrink-0 border-b border-surface">
          <Text variant={TextVariants.title}>{t('editor.presets.title')}</Text>
          <div className="flex items-center gap-1">
            <button
              className="p-2 rounded-full hover:bg-surface transition-colors"
              onClick={onNavigateToCommunity}
              data-tooltip={t('editor.presets.tooltips.explore')}
            >
              <Users size={18} />
            </button>
            <button
              className="p-2 rounded-full hover:bg-surface transition-colors"
              disabled={isLoading}
              onClick={handleImportPresets}
              data-tooltip={t('editor.presets.tooltips.import')}
            >
              <FileUp size={18} />
            </button>
            <button
              className="p-2 rounded-full hover:bg-surface transition-colors"
              disabled={presets.length === 0 || isLoading}
              onClick={handleExportAllPresets}
              data-tooltip={t('editor.presets.tooltips.export')}
            >
              <FileDown size={18} />
            </button>
            <button
              className="px-2 py-1 rounded hover:bg-surface text-xs"
              disabled={!selectedImage?.isReady || isLoading}
              data-tooltip={t('editor.presets.newXmp')}
              onClick={() => {
                exportNewXmp.current = true;
                setConfigureModalState({ isOpen: true, preset: null });
              }}
            >
              XMP
            </button>
            <button
              className="p-2 rounded-full hover:bg-surface transition-colors"
              disabled={isLoading}
              onClick={() => {
                exportNewXmp.current = false;
                setConfigureModalState({ isOpen: true, preset: null });
              }}
              data-tooltip={t('editor.presets.tooltips.saveNew')}
            >
              <Plus size={18} />
            </button>
          </div>
        </div>

        <div className="px-3 py-2 space-y-2">
          <input
            aria-label={t('editor.presets.search')}
            placeholder={t('editor.presets.search')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="w-full rounded bg-surface px-2 py-1 text-sm"
          />
          <label className="flex items-center gap-2 text-xs">
            <input type="checkbox" checked={compatibleOnly} onChange={(e) => setCompatibleOnly(e.target.checked)} />
            {t('editor.presets.compatibleOnly')}
          </label>
        </div>
        <RootDroppableArea onContextMenu={handleBackgroundContextMenu}>
          {isLoading && presets.length === 0 ? (
            <Text
              as="div"
              variant={TextVariants.heading}
              color={TextColors.secondary}
              weight={TextWeights.normal}
              className="text-center mt-4"
            >
              <Loader2 size={14} className="animate-spin inline-block mr-2" /> {t('editor.presets.status.loading')}
            </Text>
          ) : !isLoading && presets.length === 0 ? (
            <div className="text-center text-text-secondary flex flex-col items-center gap-4 pt-4">
              <Text className="max-w-xs">{t('editor.presets.status.empty')}</Text>
              <Button variant="secondary" onClick={onNavigateToCommunity}>
                <Users size={16} className="mr-2" />
                {t('editor.presets.status.getCommunity')}
              </Button>
            </div>
          ) : (
            <>
              {favoritesFolder.children.length > 0 && (
                <DroppableFolderItem
                  folder={favoritesFolder}
                  isExpanded={expandedFolders.has(FAVORITES_FOLDER_ID)}
                  isVirtual
                  onContextMenu={handleContextMenu}
                  onToggle={toggleFolder}
                >
                  {favoritesFolder.children.map((preset: Preset) => (
                    <DraggablePresetItem
                      key={preset.id}
                      dndId={`${FAVORITES_FOLDER_ID}:${preset.id}`}
                      disabled
                      onApply={handleApplyPreset}
                      onContextMenu={(e: any) => handleContextMenu(e, { preset })}
                      onToggleFavorite={() => toggleFavorite(preset.id)}
                      preset={preset}
                      isActive={preset.id === activePresetId && isActivePresetExpanded}
                      intensity={preset.id === activePresetId ? presetIntensity : 100}
                      onIntensityChange={(val) => handleIntensityChange(preset, val)}
                      onDragStateChange={handleDragStateChange}
                      onHoverChange={setHoverPreview}
                    />
                  ))}
                </DroppableFolderItem>
              )}
              <AnimatePresence>
                {folders
                  .filter((item: UserPreset) => item.folder?.id !== deletingItemId)
                  .map((item: UserPreset, index: number) => (
                    <motion.div
                      animate="visible"
                      custom={index}
                      exit="exit"
                      initial="hidden"
                      key={item.folder?.id}
                      layout="position"
                      variants={itemVariants}
                    >
                      <DroppableFolderItem
                        folder={item.folder}
                        isExpanded={!!query.trim() || (item.folder?.id ? expandedFolders.has(item.folder?.id) : false)}
                        onContextMenu={(e: any) => handleContextMenu(e, item)}
                        onToggle={toggleFolder}
                      >
                        <AnimatePresence>
                          {item.folder?.children
                            .filter((preset: Preset) => preset.id !== deletingItemId)
                            .map((preset: Preset) => (
                              <motion.div
                                exit={{ opacity: 0, x: -15, transition: { duration: 0.2 } }}
                                key={preset.id}
                                layout="position"
                              >
                                <DraggablePresetItem
                                  onApply={handleApplyPreset}
                                  onContextMenu={(e: any) => handleContextMenu(e, { preset })}
                                  onToggleFavorite={() => toggleFavorite(preset.id)}
                                  preset={preset}
                                  isActive={preset.id === activePresetId && isActivePresetExpanded}
                                  intensity={preset.id === activePresetId ? presetIntensity : 100}
                                  onIntensityChange={(val) => handleIntensityChange(preset, val)}
                                  onDragStateChange={handleDragStateChange}
                                  onHoverChange={setHoverPreview}
                                />
                              </motion.div>
                            ))}
                        </AnimatePresence>
                      </DroppableFolderItem>
                    </motion.div>
                  ))}
              </AnimatePresence>
              <AnimatePresence>
                {rootPresets
                  .filter((item: UserPreset) => item.preset?.id !== deletingItemId)
                  .map((item: UserPreset, index: number) => (
                    <motion.div
                      animate="visible"
                      custom={folders.length + index}
                      exit="exit"
                      initial="hidden"
                      key={item.preset?.id}
                      layout="position"
                      variants={itemVariants}
                    >
                      <DraggablePresetItem
                        onApply={handleApplyPreset}
                        onContextMenu={(e: any) => handleContextMenu(e, item)}
                        onToggleFavorite={() => item.preset && toggleFavorite(item.preset.id)}
                        preset={item.preset}
                        isActive={item.preset?.id === activePresetId && isActivePresetExpanded}
                        intensity={item.preset?.id === activePresetId ? presetIntensity : 100}
                        onIntensityChange={(val) => handleIntensityChange(item.preset as Preset, val)}
                        onHoverChange={setHoverPreview}
                      />
                    </motion.div>
                  ))}
              </AnimatePresence>
            </>
          )}
        </RootDroppableArea>

        <ConfigurePresetModal
          isOpen={configureModalState.isOpen}
          initialPreset={configureModalState.preset}
          onClose={() => setConfigureModalState({ isOpen: false, preset: null })}
          onSave={handleSaveConfiguredPreset}
        />
        <CreateFolderModal
          isOpen={isAddFolderModalOpen}
          onClose={() => setIsAddFolderModalOpen(false)}
          onSave={handleAddFolder}
        />
        <RenameFolderModal
          currentName={renameFolderState.folder?.name}
          isOpen={renameFolderState.isOpen}
          onClose={() => setRenameFolderState({ isOpen: false, folder: null })}
          onSave={handleRenameFolderSave}
        />
      </div>
      <DragOverlay>
        {activeItem ? (
          activeItem.type === 'preset' ? (
            <PresetItemDisplay preset={activeItem.data} />
          ) : (
            <FolderItemDisplay folder={activeItem.data} />
          )
        ) : null}
      </DragOverlay>
    </DndContext>
  );
}
