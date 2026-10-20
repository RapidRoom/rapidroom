import { memo, useState, useEffect, useRef, useMemo } from 'react';
import { Eye, EyeOff, ArrowLeft, Maximize, Loader2, Undo, Redo, Columns2, Frame } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import { SelectedImage, GroupingMode } from '../../ui/AppProperties';
import ShootingInfo from '../../ui/ShootingInfo';
import Text from '../../ui/Text';
import AppliedPresetBadge from '../../ui/AppliedPresetBadge';
import { TextColors, TextVariants, TextWeights } from '../../../types/typography';
import { useEditorStore } from '../../../store/useEditorStore';
import { useLibraryStore } from '../../../store/useLibraryStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { findGroupVariants, getVariantLabel } from '../../../utils/imageGrouping';
import type { Adjustments } from '../../../utils/adjustments';
import { describeHistoryChange, historyMenuPlacement } from '../../../utils/editHistory';

interface EditorToolbarProps {
  canRedo: boolean;
  canUndo: boolean;
  isAndroid: boolean;
  isLoading: boolean;
  onBackToLibrary(): void;
  onImageSelect?(path: string, event?: React.MouseEvent<HTMLButtonElement>): void;
  onRedo(): void;
  onToggleFullScreen(): void;
  onToggleShowOriginal(): void;
  isReferenceViewActive?: boolean;
  onToggleReferenceView?(): void;
  onUndo(): void;
  selectedImage: SelectedImage;
  showOriginal: boolean;
  adjustmentsHistory: Adjustments[];
  adjustmentsHistoryIndex: number;
  goToAdjustmentsHistoryIndex(index: number): void;
}

const EditorToolbar = memo(
  ({
    canRedo,
    canUndo,
    isAndroid,
    isLoading,
    onBackToLibrary,
    onImageSelect,
    onRedo,
    onToggleFullScreen,
    onToggleShowOriginal,
    isReferenceViewActive = false,
    onToggleReferenceView,
    onUndo,
    selectedImage,
    showOriginal,
    adjustmentsHistory,
    adjustmentsHistoryIndex,
    goToAdjustmentsHistoryIndex,
  }: EditorToolbarProps) => {
    const { t } = useTranslation();
    const isAnyLoading = isLoading;
    const [isLoaderVisible, setIsLoaderVisible] = useState(false);
    const [isLoaderMounted, setIsLoaderMounted] = useState(false);
    const [disableLoaderTransition, setDisableLoaderTransition] = useState(false);
    const hideTimeoutRef = useRef<number | null>(null);
    const prevIsLoadingRef = useRef(isLoading);
    const [isVcHovered, setIsVcHovered] = useState(false);
    const [isHistoryVisible, setIsHistoryVisible] = useState(false);
    const historyContainerRef = useRef<HTMLDivElement>(null);
    const historyButtonRef = useRef<HTMLDivElement>(null);

    const showResolution = !isAndroid && selectedImage.width > 0 && selectedImage.height > 0;

    const imageList = useLibraryStore((s) => s.imageList);
    const appSettings = useSettingsStore((s) => s.appSettings);
    const handleSettingsChange = useSettingsStore((s) => s.handleSettingsChange);
    const groupingMode: GroupingMode = useSettingsStore((s) => s.appSettings?.grouping) ?? 'off';

    const variantOptions = useMemo(() => {
      if (groupingMode === 'off' || !onImageSelect) return [];
      const isVC = selectedImage.path.includes('?vc=');
      if (isVC) return [];
      const variants = findGroupVariants(imageList, selectedImage.group_id);
      if (variants.length < 2) return [];
      return variants.map((v) => ({ path: v.path, label: getVariantLabel(v.path) }));
    }, [groupingMode, selectedImage.path, imageList, onImageSelect]);

    const { baseName, isVirtualCopy, vcId } = useMemo(() => {
      const parts = selectedImage.path.split('?vc=');
      return {
        baseName: parts[0].split(/[\\/]/).pop() || '',
        isVirtualCopy: parts.length > 1,
        vcId: parts.length > 1 ? parts[1] : null,
      };
    }, [selectedImage.path]);

    useEffect(() => {
      const wasLoadingResolution = prevIsLoadingRef.current && !isLoading;

      if (isAnyLoading) {
        if (hideTimeoutRef.current) clearTimeout(hideTimeoutRef.current);
        setDisableLoaderTransition(false);
        setIsLoaderVisible(true);
      } else if (isLoaderVisible) {
        if (wasLoadingResolution) {
          setDisableLoaderTransition(true);
          setIsLoaderVisible(false);
        } else {
          setDisableLoaderTransition(false);
          hideTimeoutRef.current = window.setTimeout(() => {
            setIsLoaderVisible(false);
          }, 300);
        }
      }

      prevIsLoadingRef.current = isLoading;

      return () => {
        if (hideTimeoutRef.current) clearTimeout(hideTimeoutRef.current);
      };
    }, [isAnyLoading, isLoading, isLoaderVisible]);

    useEffect(() => {
      if (isLoaderVisible) {
        setIsLoaderMounted(true);
      } else if (disableLoaderTransition) {
        setIsLoaderMounted(false);
      }
    }, [isLoaderVisible, disableLoaderTransition]);

    useEffect(() => {
      if (!isHistoryVisible) return;
      const handleClickOutside = (e: MouseEvent) => {
        if (
          historyContainerRef.current &&
          !historyContainerRef.current.contains(e.target as Node) &&
          historyButtonRef.current &&
          !historyButtonRef.current.contains(e.target as Node)
        ) {
          setIsHistoryVisible(false);
        }
      };
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }, [isHistoryVisible]);

    const explicitHistoryDetails = useEditorStore((state) => state.historyDetails);
    const historyNames = useMemo(
      () =>
        adjustmentsHistory.map(
          (snapshot, index) =>
            explicitHistoryDetails[index]?.label ??
            (index === 0
              ? 'Initial State'
              : describeHistoryChange(adjustmentsHistory[index - 1], snapshot, 'user').label),
        ),
      [adjustmentsHistory, explicitHistoryDetails],
    );
    const [historyPosition, setHistoryPosition] = useState<React.CSSProperties>({});
    useEffect(() => {
      if (!isHistoryVisible) return;
      const place = () => {
        const rect = historyButtonRef.current?.getBoundingClientRect();
        if (rect) setHistoryPosition(historyMenuPlacement(rect, window.innerWidth, window.innerHeight));
      };
      place();
      window.addEventListener('resize', place);
      window.addEventListener('scroll', place, true);
      return () => {
        window.removeEventListener('resize', place);
        window.removeEventListener('scroll', place, true);
      };
    }, [isHistoryVisible]);

    useEffect(() => {
      if (isHistoryVisible && historyContainerRef.current) {
        const timer = setTimeout(() => {
          const activeEl = historyContainerRef.current?.querySelector('[data-active="true"]');
          if (activeEl) {
            activeEl.scrollIntoView({ block: 'nearest', behavior: 'auto' });
          }
        }, 10);
        return () => clearTimeout(timer);
      }
    }, [isHistoryVisible, adjustmentsHistoryIndex]);

    const handleButtonKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
      if (e.key === 'Tab') return;
      e.currentTarget.blur();
    };

    return (
      <div className="@container/editor-toolbar group/shooting-info relative shrink-0 grid grid-cols-[auto_minmax(0,1fr)_auto] items-center px-3 min-h-10 py-1 gap-x-3 z-40">
        <div className="flex items-center gap-2 shrink-0 z-40 @max-[32rem]/editor-toolbar:gap-1">
          <button
            className="bg-surface text-text-primary p-1 rounded-full hover:bg-card-active transition-colors shrink-0"
            onClick={onBackToLibrary}
            onKeyDown={handleButtonKeyDown}
            data-tooltip={t('editor.toolbar.tooltips.backToLibrary')}
            data-bench-id="back-to-library"
          >
            <ArrowLeft size={20} />
          </button>

          <div className="hidden 2xl:flex items-center gap-2" aria-hidden="true">
            <div className="p-1 invisible pointer-events-none">
              <Undo size={20} />
            </div>
            <div className="p-1 invisible pointer-events-none">
              <Undo size={20} />
            </div>
            <div className="p-1 invisible pointer-events-none">
              <Undo size={20} />
            </div>
            <div className="p-1 invisible pointer-events-none">
              <Undo size={20} />
            </div>
          </div>
        </div>

        <div className="@container/editor-info flex-1 flex justify-center min-w-0 relative">
          <div className="bg-surface flex items-center overflow-hidden min-w-0 max-w-full rounded-2xl px-3 py-1 @max-[14rem]/editor-info:px-1">
            <div className="flex items-center justify-center min-w-0 max-w-full h-5" data-editor-info-line>
              <Text
                as="span"
                variant={TextVariants.small}
                color={TextColors.primary}
                weight={TextWeights.medium}
                className="truncate min-w-0 shrink"
                title={showResolution ? `${baseName} - ${selectedImage.width} × ${selectedImage.height}` : baseName}
              >
                {baseName}
              </Text>
              <AppliedPresetBadge compact />

              {isVirtualCopy && (
                <Text
                  as="div"
                  variant={TextVariants.small}
                  color={TextColors.accent}
                  weight={TextWeights.bold}
                  className="ml-2 shrink-0 bg-accent/20 px-2 py-0.5 rounded-full flex items-center overflow-hidden cursor-default @max-[14rem]/editor-info:ml-1 @max-[14rem]/editor-info:px-1"
                  onMouseEnter={() => setIsVcHovered(true)}
                  onMouseLeave={() => setIsVcHovered(false)}
                >
                  <span>{t('editor.toolbar.vc')}</span>
                  <div
                    className={clsx(
                      'transition-all duration-300 ease-out overflow-hidden whitespace-nowrap @max-[14rem]/editor-info:hidden',
                      isVcHovered ? 'max-w-20 opacity-100' : 'max-w-0 opacity-0',
                    )}
                  >
                    <span>-{vcId}</span>
                  </div>
                </Text>
              )}

              {variantOptions.length > 0 && (
                <div className="flex items-center ml-2 shrink-0 border border-text-secondary/20 rounded-full overflow-hidden">
                  {variantOptions.map((v) => {
                    const isActive = v.path === selectedImage.path;
                    return (
                      <button
                        key={v.path}
                        disabled={isActive}
                        className={clsx(
                          'px-2.5 py-1 text-[11px] font-medium transition-colors',
                          isActive ? 'bg-surface text-text-primary' : 'text-text-secondary hover:bg-surface/50',
                        )}
                        data-tooltip={t('editor.toolbar.switchToVariant', { label: v.label })}
                        onClick={(e) => onImageSelect?.(v.path, e)}
                        onKeyDown={handleButtonKeyDown}
                      >
                        {v.label}
                      </button>
                    );
                  })}
                </div>
              )}

              <ShootingInfo exif={selectedImage.exif} inline />

              <div
                className={clsx(
                  'overflow-hidden shrink-0',
                  isLoaderVisible ? 'max-w-4 opacity-100 ml-2' : 'max-w-0 opacity-0 ml-0',
                  disableLoaderTransition ? 'transition-none' : 'transition-all duration-300',
                )}
                onTransitionEnd={(e) => {
                  if (e.propertyName === 'opacity' && !isLoaderVisible) {
                    setIsLoaderMounted(false);
                  }
                }}
              >
                {isLoaderMounted && <Loader2 size={12} className="text-text-secondary animate-spin" />}
              </div>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0 z-40 @max-[32rem]/editor-toolbar:gap-1">
          <div className="relative flex items-center gap-2 @max-[32rem]/editor-toolbar:gap-1" ref={historyButtonRef}>
            <button
              className="bg-surface text-text-primary p-1 rounded-full hover:bg-card-active transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={!canUndo}
              onClick={onUndo}
              onKeyDown={handleButtonKeyDown}
              onContextMenu={(e) => {
                e.preventDefault();
                setIsHistoryVisible((prev) => !prev);
              }}
              data-tooltip={t('editor.toolbar.tooltips.undo')}
              data-bench-id="undo"
            >
              <Undo size={20} />
            </button>
            <button
              className="bg-surface text-text-primary p-1 rounded-full hover:bg-card-active transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={!canRedo}
              onClick={onRedo}
              onKeyDown={handleButtonKeyDown}
              onContextMenu={(e) => {
                e.preventDefault();
                setIsHistoryVisible((prev) => !prev);
              }}
              data-tooltip={t('editor.toolbar.tooltips.redo')}
            >
              <Redo size={20} />
            </button>

            <AnimatePresence>
              {isHistoryVisible && adjustmentsHistory && adjustmentsHistory.length > 1 && (
                <motion.div
                  ref={historyContainerRef}
                  initial={{ opacity: 0, y: -10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  transition={{ duration: 0.15, ease: 'easeOut' }}
                  style={historyPosition}
                  role="menu"
                  aria-label="Edit history"
                  className="fixed w-96 max-w-[90vw] max-h-80 bg-surface/90 backdrop-blur-md border border-text-secondary/10 shadow-xl rounded-lg overflow-y-auto custom-scrollbar z-50 flex flex-col py-1.5 px-0.5"
                >
                  {historyNames.map((name, i) => {
                    const isCurrent = i === adjustmentsHistoryIndex;
                    const isFuture = i > adjustmentsHistoryIndex;

                    const textColor = isCurrent
                      ? TextColors.button
                      : isFuture
                        ? TextColors.secondary
                        : TextColors.primary;
                    const textWeight = isCurrent ? TextWeights.medium : TextWeights.normal;

                    return (
                      <button
                        key={i}
                        data-active={isCurrent}
                        role="menuitem"
                        aria-current={isCurrent ? 'step' : undefined}
                        title={name}
                        onClick={() => {
                          goToAdjustmentsHistoryIndex(i);
                          setIsHistoryVisible(false);
                        }}
                        onKeyDown={handleButtonKeyDown}
                        className={clsx(
                          'text-left px-3 py-2 transition-colors mx-1 my-0.5 rounded-md',
                          isCurrent
                            ? 'bg-accent'
                            : isFuture
                              ? 'opacity-50 hover:bg-bg-primary hover:opacity-100'
                              : 'hover:bg-bg-primary',
                        )}
                      >
                        <div className="flex justify-between items-start gap-3">
                          <Text
                            as="span"
                            color={textColor}
                            weight={textWeight}
                            className="whitespace-normal break-words min-w-0 line-clamp-2"
                          >
                            {name}
                          </Text>
                          <Text
                            as="span"
                            variant={TextVariants.small}
                            color={textColor}
                            weight={textWeight}
                            className="opacity-50 shrink-0"
                          >
                            {i}
                          </Text>
                        </div>
                      </button>
                    );
                  })}
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          <button
            className={clsx(
              'p-1 rounded-full transition-colors',
              showOriginal
                ? 'bg-accent text-button-text hover:bg-accent/90 hover:text-button-text'
                : 'bg-surface hover:bg-card-active text-text-primary',
            )}
            onClick={onToggleShowOriginal}
            onKeyDown={handleButtonKeyDown}
            data-tooltip={
              showOriginal ? t('editor.toolbar.tooltips.showEdited') : t('editor.toolbar.tooltips.showOriginal')
            }
          >
            {showOriginal ? <EyeOff size={20} /> : <Eye size={20} />}
          </button>
          <button
            type="button"
            aria-label={t('export.instagram.guide')}
            aria-pressed={appSettings?.showInstagramGuide ?? false}
            data-instagram-guide-toggle
            data-tooltip={t('export.instagram.guide')}
            className={clsx(
              'p-1 rounded-full transition-colors',
              appSettings?.showInstagramGuide
                ? 'bg-accent text-button-text'
                : 'bg-surface hover:bg-card-active text-text-primary',
            )}
            onClick={() =>
              appSettings &&
              void handleSettingsChange({ ...appSettings, showInstagramGuide: !appSettings.showInstagramGuide })
            }
          >
            <Frame size={20} />
          </button>
          {onToggleReferenceView && (
            <button
              className={clsx(
                'p-1 rounded-full transition-colors',
                isReferenceViewActive
                  ? 'bg-accent text-button-text hover:bg-accent/90 hover:text-button-text'
                  : 'bg-surface hover:bg-card-active text-text-primary',
              )}
              onClick={onToggleReferenceView}
              onKeyDown={handleButtonKeyDown}
              aria-pressed={isReferenceViewActive}
              data-tooltip={t('editor.toolbar.tooltips.referenceView')}
              data-testid="editor-reference-view-toggle"
            >
              <Columns2 size={20} />
            </button>
          )}
          <button
            className="bg-surface text-text-primary p-1 rounded-full hover:bg-card-active transition-colors disabled:opacity-50 disabled:cursor-not-allowed relative"
            onClick={onToggleFullScreen}
            onKeyDown={handleButtonKeyDown}
            data-tooltip={t('editor.toolbar.tooltips.fullscreen')}
          >
            <div className="relative w-5 h-5 flex items-center justify-center">
              <Maximize size={20} />
            </div>
          </button>
        </div>
      </div>
    );
  },
);

export default EditorToolbar;
