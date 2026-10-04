import { useState, useEffect, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { ArrowRight, Undo2 } from 'lucide-react';
import { RENAME_VARIABLES } from '../ui/ExportImportProperties';
import { Invokes } from '../ui/AppProperties';
import Text from '../ui/Text';
import { TextColors, TextVariants } from '../../types/typography';
import {
  DEFAULT_RENAME_OPTIONS,
  RenameOptions,
  RenamePreview,
  UndoInfo,
  fileName,
  finalizeRenameTemplate,
  usesGroupTokens,
} from '../../utils/batchRename';

interface RenameFileModalProps {
  filesToRename: Array<string>;
  isOpen: boolean;
  onClose(): void;
  onSave(template: string, options: RenameOptions): void;
  onUndo(): void;
}

const PREVIEW_DELAY_MS = 150;

export default function RenameFileModal({ filesToRename, isOpen, onClose, onSave, onUndo }: RenameFileModalProps) {
  const { t } = useTranslation();
  const [nameTemplate, setNameTemplate] = useState('');
  const [options, setOptions] = useState<RenameOptions>(DEFAULT_RENAME_OPTIONS);
  const [preview, setPreview] = useState<RenamePreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [isPreviewing, setIsPreviewing] = useState(false);
  const [undoInfo, setUndoInfo] = useState<UndoInfo | null>(null);
  const [isMounted, setIsMounted] = useState(false);
  const [show, setShow] = useState(false);
  const nameInputRef = useRef<HTMLInputElement>(null);
  const requestId = useRef(0);

  const fileCount = filesToRename.length;
  const isSingleFile = fileCount === 1;
  const finalTemplate = finalizeRenameTemplate(nameTemplate, isSingleFile);
  const showGroupOptions = usesGroupTokens(finalTemplate);

  useEffect(() => {
    if (isOpen) {
      if (isSingleFile && filesToRename[0]) {
        const name = fileName(filesToRename[0].split('?vc=')[0]);
        const nameWithoutExt = name.substring(0, name.lastIndexOf('.'));
        if (nameWithoutExt) {
          setNameTemplate(nameWithoutExt);
        }
      } else {
        setNameTemplate('{original_filename}');
      }
      setOptions(DEFAULT_RENAME_OPTIONS);
      invoke<UndoInfo | null>(Invokes.GetLastRename)
        .then((info) => setUndoInfo(info ?? null))
        .catch(() => setUndoInfo(null));
      setIsMounted(true);
      const timer = setTimeout(() => setShow(true), 10);
      return () => clearTimeout(timer);
    } else {
      setShow(false);
      const timer = setTimeout(() => {
        setIsMounted(false);
        setNameTemplate('');
        setPreview(null);
        setPreviewError(null);
      }, 300);
      return () => clearTimeout(timer);
    }
  }, [isOpen, filesToRename, isSingleFile]);

  useEffect(() => {
    if (!isOpen || !finalTemplate || fileCount === 0) {
      setPreview(null);
      setPreviewError(null);
      return;
    }
    const id = ++requestId.current;
    setIsPreviewing(true);
    const timer = setTimeout(() => {
      invoke<RenamePreview>(Invokes.PreviewRenameFiles, {
        paths: filesToRename,
        nameTemplate: finalTemplate,
        options,
      })
        .then((result) => {
          if (id !== requestId.current) return;
          setPreview(result);
          setPreviewError(null);
        })
        .catch((err) => {
          if (id !== requestId.current) return;
          setPreview(null);
          setPreviewError(String(err));
        })
        .finally(() => {
          if (id === requestId.current) setIsPreviewing(false);
        });
    }, PREVIEW_DELAY_MS);
    return () => clearTimeout(timer);
  }, [isOpen, finalTemplate, filesToRename, fileCount, options]);

  const changeCount = preview?.entries.filter((entry) => entry.from !== entry.to).length ?? 0;
  const canSave =
    !!finalTemplate &&
    !isPreviewing &&
    !previewError &&
    !!preview &&
    preview.errors.length === 0 &&
    preview.conflictCount === 0 &&
    changeCount > 0;

  const handleSave = useCallback(() => {
    if (!canSave) return;
    onSave(finalTemplate, options);
    onClose();
  }, [canSave, finalTemplate, options, onSave, onClose]);

  const handleUndo = useCallback(() => {
    onUndo();
    onClose();
  }, [onUndo, onClose]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        handleSave();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    },
    [handleSave, onClose],
  );

  const handleVariableClick = (variable: string) => {
    if (!nameInputRef.current) {
      return;
    }
    const input = nameInputRef.current;
    const start = input?.selectionStart || 0;
    const end = input?.selectionEnd || 0;
    const currentValue = input.value;
    const newValue = currentValue.substring(0, start) + variable + currentValue.substring(end);
    setNameTemplate(newValue);
    setTimeout(() => {
      input.focus();
      const newCursorPos = start + variable.length;
      input.setSelectionRange(newCursorPos, newCursorPos);
    }, 0);
  };

  if (!isMounted) {
    return null;
  }

  return (
    <div
      aria-modal="true"
      className={`fixed inset-0 flex items-center justify-center z-50 bg-black/30 backdrop-blur-xs transition-opacity duration-300 ease-in-out ${
        show ? 'opacity-100' : 'opacity-0'
      }`}
      onClick={onClose}
      role="dialog"
    >
      <div
        className={`bg-surface rounded-lg shadow-xl p-6 w-full max-w-2xl transform transition-all duration-300 ease-out ${
          show ? 'scale-100 opacity-100 translate-y-0' : 'scale-95 opacity-0 -translate-y-4'
        }`}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <Text variant={TextVariants.title} className="mb-4">
          {isSingleFile
            ? t('modals.renameFile.titleSingle')
            : t('modals.renameFile.titleMultiple', { count: fileCount })}
        </Text>

        <div className="space-y-6 text-sm">
          <div>
            <Text variant={TextVariants.heading} className="block mb-2">
              {isSingleFile ? t('modals.renameFile.newName') : t('modals.renameFile.fileNamingTemplate')}
            </Text>
            <input
              autoFocus
              className="w-full bg-bg-primary border border-surface rounded-md p-2 text-sm text-text-primary focus:ring-accent focus:border-accent"
              data-testid="rename-template"
              onChange={(e) => setNameTemplate(e.target.value)}
              onKeyDown={handleKeyDown}
              ref={nameInputRef}
              type="text"
              value={nameTemplate}
            />
            {!isSingleFile && (
              <div className="flex flex-wrap gap-2 mt-2">
                {RENAME_VARIABLES.map((variable: string) => (
                  <button
                    className="px-2 py-1 bg-surface text-text-secondary text-xs rounded-md hover:bg-card-active transition-colors"
                    key={variable}
                    onClick={() => handleVariableClick(variable)}
                  >
                    {variable}
                  </button>
                ))}
              </div>
            )}
          </div>

          {showGroupOptions && (
            <div className="space-y-2" data-testid="rename-group-options">
              <Text variant={TextVariants.heading}>{t('modals.renameFile.groups')}</Text>
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  checked={options.groupMode === 'auto'}
                  name="rename-group-mode"
                  onChange={() => setOptions({ ...options, groupMode: 'auto' })}
                  type="radio"
                />
                <Text as="span">{t('modals.renameFile.groupAuto')}</Text>
                <input
                  aria-label={t('modals.renameFile.groupSeconds')}
                  className="w-16 bg-bg-primary border border-surface rounded-md px-2 py-1 text-sm text-text-primary"
                  disabled={options.groupMode !== 'auto'}
                  min={0}
                  onChange={(e) => setOptions({ ...options, groupSeconds: Math.max(0, Number(e.target.value) || 0) })}
                  step={0.5}
                  type="number"
                  value={options.groupSeconds}
                />
                <Text as="span">{t('modals.renameFile.seconds')}</Text>
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  checked={options.groupMode === 'selection'}
                  name="rename-group-mode"
                  onChange={() => setOptions({ ...options, groupMode: 'selection' })}
                  type="radio"
                />
                <Text as="span">{t('modals.renameFile.groupSelection')}</Text>
              </label>
            </div>
          )}

          <div>
            <div className="flex items-baseline justify-between mb-2">
              <Text variant={TextVariants.heading}>{t('modals.renameFile.preview')}</Text>
              {preview && (
                <Text variant={TextVariants.small} color={TextColors.secondary}>
                  {t('modals.renameFile.summary', { photos: preview.photoCount, files: preview.entries.length })}
                </Text>
              )}
            </div>
            {(previewError || (preview && preview.errors.length > 0)) && (
              <ul className="mb-2 space-y-1" data-testid="rename-errors">
                {(previewError ? [previewError] : (preview?.errors ?? [])).map((error) => (
                  <li key={error}>
                    <Text variant={TextVariants.small} color={TextColors.error}>
                      {error}
                    </Text>
                  </li>
                ))}
              </ul>
            )}
            {preview && preview.conflictCount > 0 && (
              <Text variant={TextVariants.small} color={TextColors.error} className="mb-2">
                {t('modals.renameFile.conflicts', { count: preview.conflictCount })}
              </Text>
            )}
            <ul
              className="max-h-64 overflow-y-auto rounded-md bg-bg-primary p-2 space-y-0.5 font-mono text-xs"
              data-testid="rename-preview"
            >
              {preview?.entries.map((entry) => (
                <li
                  className={`flex items-center gap-2 ${entry.kind === 'sidecar' ? 'pl-4 opacity-70' : ''} ${
                    entry.conflict ? 'text-red-400' : 'text-text-primary'
                  }`}
                  data-conflict={entry.conflict ?? undefined}
                  data-kind={entry.kind}
                  key={entry.from}
                >
                  <span className="truncate">{fileName(entry.from)}</span>
                  <ArrowRight className="shrink-0" size={12} />
                  <span className="truncate">{fileName(entry.to)}</span>
                  {entry.conflict && (
                    <span className="ml-auto shrink-0">
                      {entry.conflict === 'existing'
                        ? t('modals.renameFile.conflictExisting')
                        : t('modals.renameFile.conflictInBatch')}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        </div>

        <div className="flex items-center justify-between gap-3 mt-8">
          <div>
            {undoInfo && (
              <button
                className="flex items-center gap-2 px-3 py-2 rounded-md text-text-secondary hover:bg-card-active transition-colors"
                onClick={handleUndo}
              >
                <Undo2 size={14} />
                {t('modals.renameFile.undoLast', { count: undoInfo.photoCount })}
              </button>
            )}
          </div>
          <div className="flex gap-3">
            <button
              className="px-4 py-2 rounded-md text-text-secondary hover:bg-surface transition-colors"
              onClick={onClose}
            >
              {t('modals.renameFile.cancel')}
            </button>
            <button
              className="px-4 py-2 rounded-md bg-accent shadow-shiny text-button-text font-semibold hover:bg-accent-hover disabled:bg-gray-500 disabled:text-white disabled:cursor-not-allowed transition-colors"
              disabled={!canSave}
              onClick={handleSave}
            >
              {t('modals.renameFile.save')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
