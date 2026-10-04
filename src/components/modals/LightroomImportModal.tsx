import { useCallback, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { CheckCircle2, CircleAlert, FolderSearch } from 'lucide-react';
import Switch from '../ui/Switch';
import LightroomDevelopImport from './LightroomDevelopImport';
import Text from '../ui/Text';
import { TextVariants } from '../../types/typography';
import { AlbumItem, Invokes } from '../ui/AppProperties';
import { useLibraryStore } from '../../store/useLibraryStore';
import { useSettingsStore } from '../../store/useSettingsStore';

export interface LightroomRootFolder {
  catalogPath: string;
  localPath: string;
  resolution: 'original' | 'mapped' | 'besideCatalog';
  found: boolean;
  imageCount: number;
  missingImageCount: number;
}

export interface LightroomImportPreview {
  catalogName: string;
  collectionCount: number;
  groupCount: number;
  matchedImageCount: number;
  missingImageCount: number;
  missingImages: string[];
  smartCollections: string[];
  skippedOtherCount: number;
  rootFolders: LightroomRootFolder[];
  replacesPreviousImport: boolean;
}

interface LightroomImportModalProps {
  catalogPath: string | null;
  onClose(): void;
  refreshAllFolderTrees(): Promise<void>;
  refreshImageList(): Promise<void>;
}

export default function LightroomImportModal({
  catalogPath,
  onClose,
  refreshAllFolderTrees,
  refreshImageList,
}: LightroomImportModalProps) {
  const { t } = useTranslation();
  const [isMounted, setIsMounted] = useState(false);
  const [show, setShow] = useState(false);
  const [mappings, setMappings] = useState<Record<string, string> | null>(null);
  const [preview, setPreview] = useState<LightroomImportPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [addRootFolders, setAddRootFolders] = useState(false);
  const [tab, setTab] = useState<'collections' | 'develop'>('collections');
  const rootPaths = useLibraryStore((state) => state.rootPaths);
  const isCardMode = useLibraryStore((state) => state.cardBrowseRoot !== null);

  useEffect(() => {
    if (catalogPath) {
      setIsMounted(true);
      setTab('collections');
      setPreview(null);
      setError(null);
      setAddRootFolders(false);
      setMappings({ ...(useSettingsStore.getState().appSettings?.lightroomPathMappings ?? {}) });
      const timer = setTimeout(() => setShow(true), 10);
      return () => clearTimeout(timer);
    }
    setShow(false);
    setMappings(null);
    const timer = setTimeout(() => setIsMounted(false), 300);
    return () => clearTimeout(timer);
  }, [catalogPath]);

  useEffect(() => {
    if (!catalogPath || !mappings) return;
    let cancelled = false;
    setIsLoading(true);
    invoke<LightroomImportPreview>(Invokes.PreviewLightroomCollections, { path: catalogPath, mappings })
      .then((result) => {
        if (cancelled) return;
        setPreview(result);
        setError(null);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [catalogPath, mappings]);

  const handleClose = useCallback(() => {
    if (!isImporting) onClose();
  }, [isImporting, onClose]);

  const handleRelink = async (root: LightroomRootFolder) => {
    const folder = await openDialog({
      directory: true,
      multiple: false,
      title: t('modals.lightroomImport.relinkTitle', { path: root.catalogPath }),
    });
    if (typeof folder !== 'string') return;
    setMappings((current) => ({ ...current, [root.catalogPath]: folder }));
  };

  const rootsToAdd =
    preview?.rootFolders
      .filter((root) => root.found && !rootPaths.includes(root.localPath))
      .map((root) => root.localPath) ?? [];

  const persistImportPaths = async () => {
    if (!mappings) return;
    const newRoots = addRootFolders && !isCardMode ? rootsToAdd : [];
    const { rootPaths: currentRoots, expandedFolders, setLibrary } = useLibraryStore.getState();
    const updatedRoots = [...currentRoots, ...newRoots.filter((root) => !currentRoots.includes(root))];
    const { appSettings, handleSettingsChange } = useSettingsStore.getState();
    if (appSettings) {
      await handleSettingsChange({
        ...appSettings,
        lightroomPathMappings: { ...appSettings.lightroomPathMappings, ...mappings },
        ...(newRoots.length > 0 ? { rootFolders: updatedRoots } : {}),
      });
    }
    if (newRoots.length > 0) {
      setLibrary({ rootPaths: updatedRoots, expandedFolders: new Set([...expandedFolders, ...newRoots]) });
      await refreshAllFolderTrees();
    }
  };

  const handleImport = async () => {
    if (!catalogPath || !mappings || !preview) return;
    setIsImporting(true);
    try {
      const result = await invoke<LightroomImportPreview>(Invokes.ImportLightroomCollections, {
        path: catalogPath,
        mappings,
      });
      const albumTree = await invoke<AlbumItem[]>(Invokes.GetAlbums);
      useLibraryStore.getState().setLibrary({ albumTree });

      await persistImportPaths();
      toast.success(t('contextMenus.toasts.importedLightroomCollections', { name: result.catalogName }));
      onClose();
    } catch (err) {
      toast.error(t('contextMenus.toasts.failedImportLightroomCollections', { err }));
    } finally {
      setIsImporting(false);
    }
  };

  const handleCopyMissing = () => {
    if (!preview) return;
    navigator.clipboard
      .writeText(preview.missingImages.join('\n'))
      .then(() => toast.success(t('modals.lightroomImport.copied')))
      .catch((err) => toast.error(String(err)));
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      handleClose();
    }
  };

  if (!isMounted) {
    return null;
  }

  const missingRootCount = preview?.rootFolders.filter((root) => !root.found).length ?? 0;
  const hasContent = !!preview && preview.groupCount + preview.collectionCount > 0;
  const resolutionLabel = (root: LightroomRootFolder) =>
    root.resolution === 'mapped'
      ? t('modals.lightroomImport.relinked')
      : root.resolution === 'besideCatalog'
        ? t('modals.lightroomImport.besideCatalog')
        : null;

  const stats = preview
    ? [
        { label: t('modals.lightroomImport.collectionSets'), value: preview.groupCount },
        { label: t('modals.lightroomImport.collections'), value: preview.collectionCount },
        { label: t('modals.lightroomImport.imagesFound'), value: preview.matchedImageCount },
        { label: t('modals.lightroomImport.imagesMissing'), value: preview.missingImageCount },
      ]
    : [];

  const modalContent = (
    <div
      aria-labelledby="lightroom-import-title"
      aria-modal="true"
      className={`fixed inset-0 flex items-center justify-center z-50 bg-black/30 backdrop-blur-xs transition-opacity duration-300 ease-in-out ${
        show ? 'opacity-100' : 'opacity-0'
      }`}
      onClick={handleClose}
      role="dialog"
    >
      <div
        className={`bg-surface rounded-lg shadow-xl p-6 w-full max-w-2xl max-h-[85vh] flex flex-col transform transition-all duration-300 ease-out ${
          show ? 'scale-100 opacity-100 translate-y-0' : 'scale-95 opacity-0 -translate-y-4'
        }`}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <Text variant={TextVariants.title} id="lightroom-import-title" className="mb-1">
          {t('modals.lightroomImport.title')}
        </Text>
        <Text variant={TextVariants.small} className="mb-4 break-all">
          {catalogPath}
        </Text>

        <div className="flex gap-2 mb-4" role="tablist" aria-label={t('modals.lightroomImport.title')}>
          <button
            role="tab"
            aria-selected={tab === 'collections'}
            disabled={isImporting}
            className="px-3 py-2 rounded-md aria-selected:bg-bg-primary disabled:opacity-50"
            onClick={() => setTab('collections')}
          >
            {t('modals.lightroomImport.collections')}
          </button>
          <button
            role="tab"
            aria-selected={tab === 'develop'}
            disabled={isImporting}
            className="px-3 py-2 rounded-md aria-selected:bg-bg-primary disabled:opacity-50"
            onClick={() => setTab('develop')}
          >
            {t('modals.lightroomDevelop.title')}
          </button>
        </div>
        <div className="flex-1 overflow-y-auto space-y-6 text-sm pr-1">
          {error && <Text color="error">{error}</Text>}
          {!preview && !error && <Text color="secondary">{t('modals.lightroomImport.reading')}</Text>}

          {preview && (
            <>
              {tab === 'collections' && (
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                  {stats.map((stat) => (
                    <div key={stat.label} className="bg-bg-primary rounded-md p-3">
                      <Text variant={TextVariants.heading}>{stat.value.toLocaleString()}</Text>
                      <Text variant={TextVariants.small}>{stat.label}</Text>
                    </div>
                  ))}
                </div>
              )}
              {tab === 'collections' && preview.replacesPreviousImport && (
                <Text color="secondary">{t('modals.lightroomImport.replacesPrevious')}</Text>
              )}

              {preview.rootFolders.length > 0 && (
                <div>
                  <Text variant={TextVariants.heading} className="block mb-1">
                    {t('modals.lightroomImport.rootFolders')}
                  </Text>
                  <Text variant={TextVariants.small} className="mb-2">
                    {t('modals.lightroomImport.rootFoldersHint')}
                  </Text>
                  {missingRootCount > 0 && (
                    <Text color="error" className="mb-2">
                      {t('modals.lightroomImport.missingRoots', { total: missingRootCount })}
                    </Text>
                  )}
                  <ul className="space-y-2">
                    {preview.rootFolders.map((root) => (
                      <li key={root.catalogPath} className="bg-bg-primary rounded-md p-3 flex items-start gap-3">
                        {root.found ? (
                          <CheckCircle2 size={18} className="text-green-400 shrink-0 mt-0.5" />
                        ) : (
                          <CircleAlert size={18} className="text-red-400 shrink-0 mt-0.5" />
                        )}
                        <div className="flex-1 min-w-0">
                          <Text className="break-all font-mono text-xs">{root.catalogPath}</Text>
                          <Text variant={TextVariants.small} className="break-all font-mono">
                            → {root.localPath}
                          </Text>
                          <Text variant={TextVariants.small}>
                            {[
                              root.found ? t('modals.lightroomImport.found') : t('modals.lightroomImport.notFound'),
                              resolutionLabel(root),
                              t('modals.lightroomImport.rootImages', {
                                images: root.imageCount,
                                missing: root.missingImageCount,
                              }),
                            ]
                              .filter(Boolean)
                              .join(' · ')}
                          </Text>
                        </div>
                        <button
                          className="px-3 py-1 rounded-md bg-surface text-text-secondary hover:bg-card-active transition-colors flex items-center gap-1 shrink-0 disabled:opacity-50"
                          disabled={isLoading || isImporting}
                          onClick={() => handleRelink(root)}
                        >
                          <FolderSearch size={14} />
                          {t('modals.lightroomImport.relink')}
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {tab === 'collections' && preview.smartCollections.length > 0 && (
                <details>
                  <summary className="cursor-pointer text-text-secondary">
                    {t('modals.lightroomImport.smartCollections', { total: preview.smartCollections.length })}
                  </summary>
                  <ul className="mt-2 ml-4 list-disc text-text-secondary">
                    {preview.smartCollections.map((name) => (
                      <li key={name}>{name}</li>
                    ))}
                  </ul>
                </details>
              )}

              {tab === 'collections' && preview.skippedOtherCount > 0 && (
                <Text color="secondary">
                  {t('modals.lightroomImport.skippedOther', { total: preview.skippedOtherCount })}
                </Text>
              )}

              {tab === 'collections' && preview.missingImageCount > 0 && (
                <details>
                  <summary className="cursor-pointer text-text-secondary">
                    {t('modals.lightroomImport.missingFiles', { total: preview.missingImageCount })}
                  </summary>
                  <Text variant={TextVariants.small} className="mt-2">
                    {t('modals.lightroomImport.missingFilesHint')}
                  </Text>
                  <ul className="mt-2 max-h-48 overflow-y-auto bg-bg-primary rounded-md p-2 font-mono text-xs text-text-secondary">
                    {preview.missingImages.map((path) => (
                      <li key={path} className="break-all">
                        {path}
                      </li>
                    ))}
                  </ul>
                  {preview.missingImageCount > preview.missingImages.length && (
                    <Text variant={TextVariants.small} className="mt-1">
                      {t('modals.lightroomImport.missingFilesMore', {
                        total: preview.missingImageCount - preview.missingImages.length,
                      })}
                    </Text>
                  )}
                  <button
                    className="mt-2 px-3 py-1 rounded-md bg-bg-primary text-text-secondary hover:bg-card-active transition-colors"
                    onClick={handleCopyMissing}
                  >
                    {t('modals.lightroomImport.copyList')}
                  </button>
                </details>
              )}

              {tab === 'collections' && !hasContent && (
                <Text color="secondary">{t('modals.lightroomImport.nothingToImport')}</Text>
              )}

              {rootsToAdd.length > 0 && !isCardMode && (
                <Switch
                  disabled={isImporting}
                  checked={addRootFolders}
                  label={t('modals.lightroomImport.addRootFolders')}
                  onChange={setAddRootFolders}
                />
              )}
            </>
          )}
          {tab === 'develop' && catalogPath && mappings && (
            <LightroomDevelopImport
              catalogPath={catalogPath}
              mappings={mappings}
              isCardMode={isCardMode}
              onBusyChange={setIsImporting}
              onClose={onClose}
              onImported={async () => {
                await persistImportPaths();
                await refreshImageList();
              }}
            />
          )}
        </div>

        <div className="flex justify-end gap-3 mt-6">
          <button
            className="px-4 py-2 rounded-md text-text-secondary hover:bg-bg-primary transition-colors disabled:opacity-50"
            disabled={isImporting}
            onClick={handleClose}
          >
            {t('modals.confirm.cancel')}
          </button>
          {tab === 'collections' && (
            <button
              className="px-4 py-2 rounded-md bg-accent shadow-shiny text-button-text font-semibold hover:bg-accent-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={!hasContent || isLoading || isImporting || !!error}
              onClick={handleImport}
            >
              {isImporting ? t('modals.lightroomImport.importing') : t('modals.lightroomImport.import')}
            </button>
          )}
        </div>
      </div>
    </div>
  );

  return createPortal(modalContent, document.body);
}
