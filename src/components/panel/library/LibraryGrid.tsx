import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { List, useListCallbackRef } from 'react-window';
import { ChevronUp, ChevronDown } from 'lucide-react';
import debounce from 'lodash.debounce';
import { useTranslation } from 'react-i18next';
import { Row as MemoizedRow } from './LibraryItems';
import type { ColumnWidths } from '../MainLibrary';
import { useShallow } from 'zustand/react/shallow';
import { useLibraryStore } from '../../../store/useLibraryStore';
import {
  LibraryViewMode,
  SortDirection,
  LibraryDisplayMode,
  ThumbnailAspectRatio,
  ThumbnailSize,
  ExifOverlay,
  ImageFile,
  SortCriteria,
} from '../../ui/AppProperties';
import type { GroupBadgeInfo, GroupId } from '../../../utils/imageGrouping';
import Text from '../../ui/Text';
import { TextColors, TextVariants, TextWeights, TEXT_COLOR_KEYS } from '../../../types/typography';
import { useProcessStore } from '../../../store/useProcessStore';
import { useSettingsStore } from '../../../store/useSettingsStore';

const Row = (props: Parameters<typeof MemoizedRow>[0]): React.ReactElement => <MemoizedRow {...props} />;

type ThumbnailSizeOption = { id: ThumbnailSize; label: string; size: number };

interface ListHeaderProps {
  containerRef: React.RefObject<HTMLDivElement | null>;
  onSortChange: (key: string) => void;
  setWidths: React.Dispatch<React.SetStateAction<ColumnWidths>>;
  sortCriteria: SortCriteria;
  widths: ColumnWidths;
}

interface ColumnProps {
  nextKey?: keyof ColumnWidths;
  sortKey?: string;
  title: string;
  widthKey: keyof ColumnWidths;
}

interface ImageRow {
  images: ImageFile[];
  justifiedWidths?: number[];
  rowHeight?: number;
  startIndex?: number;
  type: 'images';
}

interface FolderHeaderRow {
  count: number;
  isExpanded: boolean;
  path: string;
  type: 'header';
}

interface FooterRow {
  type: 'footer';
}

type GridRow = ImageRow | FolderHeaderRow | FooterRow;

interface FolderImageGroup {
  images: ImageFile[];
  path: string;
}

interface LibraryGridProps {
  activePath: string | null;
  currentFolderPath: string | null;
  groupBadgeInfo: Map<GroupId, GroupBadgeInfo> | null;
  imageList: ImageFile[];
  imageRatings: Record<string, number>;
  libraryDisplayMode: LibraryDisplayMode;
  libraryViewMode: LibraryViewMode;
  multiSelectedPaths: string[];
  onClearSelection: () => void;
  onContextMenu: (event: React.MouseEvent, path: string, forceSingleSelection?: boolean) => void;
  onEmptyAreaContextMenu: (event: React.MouseEvent) => void;
  onImageClick: (path: string, event: React.MouseEvent) => void;
  onImageDoubleClick: (path: string) => void;
  onRequestThumbnails?: (paths: string[]) => void;
  onThumbnailSizeChange: (size: ThumbnailSize) => void;
  thumbnailAspectRatio: ThumbnailAspectRatio;
  thumbnailSize: ThumbnailSize;
  thumbnailSizeOptions: ThumbnailSizeOption[];
}

function ListHeader({ widths, setWidths, containerRef, sortCriteria, onSortChange }: ListHeaderProps) {
  const { t } = useTranslation();
  const exifOverlay = useSettingsStore((s) => s.appSettings?.exifOverlay || ExifOverlay.Off);
  const showExifCols = exifOverlay !== ExifOverlay.Off;
  const totalRawWidth =
    widths.thumbnail +
    widths.name +
    widths.date +
    widths.rating +
    widths.color +
    (showExifCols ? widths.shutter + widths.aperture + widths.iso + widths.focal : 0);

  const handleResize = (e: React.MouseEvent, leftCol: keyof ColumnWidths, rightCol: keyof ColumnWidths) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startLeftWidth = widths[leftCol];
    const startRightWidth = widths[rightCol];
    const containerWidth = containerRef.current?.clientWidth || 1000;

    const onMouseMove = (moveEvent: MouseEvent) => {
      const deltaX = moveEvent.clientX - startX;
      const deltaPercent = (deltaX / containerWidth) * 100;

      let newLeft = startLeftWidth + deltaPercent;
      let newRight = startRightWidth - deltaPercent;

      if (newLeft < 1) {
        newRight -= 1 - newLeft;
        newLeft = 1;
      }
      if (newRight < 1) {
        newLeft -= 1 - newRight;
        newRight = 1;
      }

      setWidths((prev) => ({
        ...prev,
        [leftCol]: newLeft,
        [rightCol]: newRight,
      }));
    };

    const onMouseUp = () => {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
    };

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  };

  const Column = ({ title, widthKey, nextKey, sortKey }: ColumnProps) => {
    const isSorted = sortCriteria.key === sortKey;
    const isAsc = sortCriteria.order === SortDirection.Ascending;
    const actualWidth = `${(widths[widthKey] / totalRawWidth) * 100}%`;

    return (
      <div
        style={{ width: actualWidth }}
        className={`relative flex items-center px-3 h-full select-none ${
          sortKey ? 'cursor-pointer hover:bg-bg-primary/50 transition-colors' : ''
        }`}
        onClick={() => sortKey && onSortChange(sortKey)}
      >
        <Text
          variant={TextVariants.small}
          weight={TextWeights.semibold}
          color={isSorted ? TextColors.primary : TextColors.secondary}
          className="uppercase tracking-wider text-[11px]"
        >
          {title}
        </Text>
        {isSorted && (
          <span className={`ml-1 flex items-center ${TEXT_COLOR_KEYS[TextColors.primary]}`}>
            {isAsc ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
          </span>
        )}
        {nextKey && (
          <div
            className="absolute right-[-3px] top-1.5 bottom-1.5 w-[6px] cursor-col-resize z-10 group flex items-center justify-center"
            onMouseDown={(e) => handleResize(e, widthKey, nextKey)}
          >
            <div className="w-px h-full bg-border-color/40 group-hover:bg-accent transition-colors" />
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="flex items-center w-full h-9 bg-bg-secondary/80 backdrop-blur-sm border-b border-border-color/50 shrink-0">
      <Column title="" widthKey="thumbnail" nextKey="name" />
      <Column title={t('library.grid.columns.name')} widthKey="name" nextKey="date" sortKey="name" />
      <Column title={t('library.grid.columns.modified')} widthKey="date" nextKey="rating" sortKey="date" />
      <Column title={t('library.grid.columns.rating')} widthKey="rating" nextKey="color" sortKey="rating" />
      {showExifCols ? (
        <>
          <Column title={t('library.grid.columns.label')} widthKey="color" nextKey="shutter" />
          <Column
            title={t('library.grid.columns.shutter')}
            widthKey="shutter"
            nextKey="aperture"
            sortKey="shutter_speed"
          />
          <Column title={t('library.grid.columns.aperture')} widthKey="aperture" nextKey="iso" sortKey="aperture" />
          <Column title={t('library.grid.columns.iso')} widthKey="iso" nextKey="focal" sortKey="iso" />
          <Column title={t('library.grid.columns.focal')} widthKey="focal" sortKey="focal_length" />
        </>
      ) : (
        <Column title={t('library.grid.columns.label')} widthKey="color" />
      )}
    </div>
  );
}

const groupImagesByFolder = (images: ImageFile[], baseFolderPath: string | null): FolderImageGroup[] => {
  const groups: Record<string, ImageFile[]> = {};

  images.forEach((img) => {
    const physicalPath = img.path.split('?vc=')[0];
    const separator = physicalPath.includes('/') ? '/' : '\\';
    const lastSep = physicalPath.lastIndexOf(separator);
    const dir = lastSep > -1 ? physicalPath.substring(0, lastSep) : physicalPath;

    if (!groups[dir]) {
      groups[dir] = [];
    }
    groups[dir].push(img);
  });

  const sortedKeys = Object.keys(groups).sort((a, b) => {
    if (a === baseFolderPath) return -1;
    if (b === baseFolderPath) return 1;
    return a.localeCompare(b);
  });

  return sortedKeys.map((dir) => ({
    path: dir,
    images: groups[dir],
  }));
};

export function buildJustifiedRows(
  images: ImageFile[],
  ratioMap: Record<string, number>,
  containerWidth: number,
  targetHeight: number,
  gap: number,
): ImageRow[] {
  const rows: ImageRow[] = [];
  let currentRow: { image: ImageFile; ratio: number }[] = [];
  let currentWidth = 0;

  for (const img of images) {
    const ratio = ratioMap[img.path] || 1.5;
    currentRow.push({ image: img, ratio });
    currentWidth += ratio * targetHeight;

    const totalGaps = (currentRow.length - 1) * gap;

    if (currentWidth + totalGaps >= containerWidth) {
      const availableForImages = containerWidth - totalGaps;
      const totalRatio = currentRow.reduce((sum, item) => sum + item.ratio, 0);
      const computedHeight = Math.floor(availableForImages / totalRatio);

      let usedWidth = 0;
      const justifiedWidths = currentRow.map((item, index) => {
        if (index === currentRow.length - 1) {
          return availableForImages - usedWidth;
        }
        const w = Math.floor(item.ratio * computedHeight);
        usedWidth += w;
        return w;
      });

      rows.push({
        type: 'images',
        rowHeight: computedHeight,
        images: currentRow.map((item) => item.image),
        justifiedWidths,
      });

      currentRow = [];
      currentWidth = 0;
    }
  }

  if (currentRow.length > 0) {
    rows.push({
      type: 'images',
      rowHeight: targetHeight,
      images: currentRow.map((item) => item.image),
      justifiedWidths: currentRow.map((item) => Math.floor(item.ratio * targetHeight)),
    });
  }

  return rows;
}

export default function LibraryGrid(props: LibraryGridProps) {
  const {
    imageList,
    libraryViewMode,
    thumbnailSize,
    libraryDisplayMode,
    currentFolderPath,
    activePath,
    multiSelectedPaths,
    onContextMenu,
    onImageClick,
    onImageDoubleClick,
    thumbnailAspectRatio,
    imageRatings,
    onRequestThumbnails,
    thumbnailSizeOptions,
    onThumbnailSizeChange,
    groupBadgeInfo,
  } = props;
  const { listColumnWidths, setLibrary, sortCriteria, setSortCriteria } = useLibraryStore(
    useShallow((state) => ({
      listColumnWidths: state.listColumnWidths,
      setLibrary: state.setLibrary,
      sortCriteria: state.sortCriteria,
      setSortCriteria: state.setSortCriteria,
    })),
  );

  const [gridSize, setGridSize] = useState({ height: 0, width: 0 });
  const [listHandle, setListHandle] = useListCallbackRef();
  const [collapsedRecursiveFolders, setCollapsedRecursiveFolders] = useState<Set<string>>(new Set());
  const libraryContainerRef = useRef<HTMLDivElement>(null);
  const gridObserverRef = useRef<ResizeObserver | null>(null);
  const loadedThumbnailsRef = useRef(new Set<string>());
  const requestQueueRef = useRef<Set<string>>(new Set());
  const requestTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const exifOverlay = useSettingsStore((s) => s.appSettings?.exifOverlay || ExifOverlay.Off);
  const showExifCols = exifOverlay !== ExifOverlay.Off;

  const isClickSelectionRef = useRef(false);
  const clickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleImageClick = useCallback(
    (path: string, e: React.MouseEvent) => {
      isClickSelectionRef.current = true;
      if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
      clickTimerRef.current = setTimeout(() => {
        isClickSelectionRef.current = false;
      }, 300);
      onImageClick?.(path, e);
    },
    [onImageClick],
  );

  const handleImageDoubleClick = useCallback(
    (path: string) => {
      isClickSelectionRef.current = true;
      if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
      clickTimerRef.current = setTimeout(() => {
        isClickSelectionRef.current = false;
      }, 300);
      onImageDoubleClick?.(path);
    },
    [onImageDoubleClick],
  );

  useEffect(() => {
    return () => {
      if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
    };
  }, []);

  const ratioMapRef = useRef<Record<string, number>>({});
  const [ratioMapVersion, setRatioMapVersion] = useState(0);
  const ratioTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleAspectRatioLoaded = useCallback(
    (path: string, ratio: number) => {
      if (Math.abs((ratioMapRef.current[path] || 0) - ratio) > 0.01) {
        ratioMapRef.current[path] = ratio;

        if (!ratioTimeoutRef.current) {
          ratioTimeoutRef.current = setTimeout(() => {
            const resetAfterIndex =
              listHandle && 'resetAfterIndex' in listHandle ? listHandle.resetAfterIndex : undefined;
            if (typeof resetAfterIndex === 'function') {
              resetAfterIndex.call(listHandle, 0);
            }
            setRatioMapVersion((v) => v + 1);
            ratioTimeoutRef.current = null;
          }, 150);
        }
      }
    },
    [listHandle],
  );

  useEffect(() => {
    return () => {
      if (ratioTimeoutRef.current) clearTimeout(ratioTimeoutRef.current);
    };
  }, []);

  useEffect(() => {
    const el = libraryContainerRef.current;
    if (gridObserverRef.current) {
      gridObserverRef.current.disconnect();
      gridObserverRef.current = null;
    }
    if (el) {
      const ro = new ResizeObserver((entries) => {
        const entry = entries[0];
        if (entry) {
          const height = Math.round(entry.contentRect.height);
          const width = Math.round(entry.contentRect.width);

          setGridSize((prev) => (prev.height === height && prev.width === width ? prev : { height, width }));
        }
      });
      ro.observe(el);
      gridObserverRef.current = ro;
    }
    return () => gridObserverRef.current?.disconnect();
  }, [libraryContainerRef]);

  useEffect(() => {
    const handleWheel = (event: WheelEvent) => {
      const container = libraryContainerRef.current;
      if (!container || !(event.target instanceof Node) || !container.contains(event.target)) {
        return;
      }

      if (event.ctrlKey || event.metaKey) {
        event.preventDefault();
        const currentIndex = thumbnailSizeOptions.findIndex((option) => option.id === thumbnailSize);
        if (currentIndex === -1) {
          return;
        }

        const nextIndex =
          event.deltaY < 0
            ? Math.min(currentIndex + 1, thumbnailSizeOptions.length - 1)
            : Math.max(currentIndex - 1, 0);
        if (nextIndex !== currentIndex) {
          onThumbnailSizeChange(thumbnailSizeOptions[nextIndex].id);
        }
      }
    };

    window.addEventListener('wheel', handleWheel, { passive: false });
    return () => {
      window.removeEventListener('wheel', handleWheel);
    };
  }, [thumbnailSize, onThumbnailSizeChange, thumbnailSizeOptions]);

  const handleScroll = useMemo(
    () =>
      debounce((top: number) => {
        setLibrary({ libraryScrollTop: top });
      }, 200),
    [setLibrary],
  );

  useEffect(() => () => handleScroll.cancel(), [handleScroll]);

  const queueThumbnailRequest = useCallback(
    (path: string) => {
      if (!onRequestThumbnails) return;
      if (useProcessStore.getState().thumbnails[path]) return;
      requestQueueRef.current.add(path);
      if (!requestTimeoutRef.current) {
        requestTimeoutRef.current = setTimeout(() => {
          const paths = Array.from(requestQueueRef.current);
          if (paths.length > 0) {
            onRequestThumbnails(paths);
            requestQueueRef.current.clear();
          }
          requestTimeoutRef.current = null;
        }, 50);
      }
    },
    [onRequestThumbnails],
  );

  const handleToggleRecursiveFolder = useCallback((path: string) => {
    setCollapsedRecursiveFolders((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const handleImageLoad = useCallback((path: string) => {
    loadedThumbnailsRef.current.add(path);
  }, []);

  const gridData = useMemo(() => {
    if (gridSize.width === 0 || imageList.length === 0) return null;

    const isListView = libraryDisplayMode === LibraryDisplayMode.List;
    const OUTER_PADDING = isListView ? 0 : 12;
    const ITEM_GAP = isListView ? 0 : 12;
    const minThumbWidth = thumbnailSizeOptions.find((option) => option.id === thumbnailSize)?.size || 240;

    const availableWidth = gridSize.width - OUTER_PADDING * 2;
    const columnCount = isListView
      ? 1
      : Math.max(1, Math.floor((availableWidth + ITEM_GAP) / (minThumbWidth + ITEM_GAP)));
    const itemWidth = isListView ? availableWidth : (availableWidth - ITEM_GAP * (columnCount - 1)) / columnCount;

    const totalBase =
      listColumnWidths.thumbnail +
      listColumnWidths.name +
      listColumnWidths.date +
      listColumnWidths.rating +
      listColumnWidths.color +
      (showExifCols
        ? listColumnWidths.shutter + listColumnWidths.aperture + listColumnWidths.iso + listColumnWidths.focal
        : 0);

    const listRowHeight = Math.max(36, Math.min(300, (availableWidth * listColumnWidths.thumbnail) / totalBase));
    const rowHeight = isListView ? listRowHeight : itemWidth + ITEM_GAP;
    const headerHeight = 40;

    const rows: GridRow[] = [];
    const isJustified = thumbnailAspectRatio === ThumbnailAspectRatio.Justified && !isListView;

    if (libraryViewMode === LibraryViewMode.Recursive) {
      const groups = groupImagesByFolder(imageList, currentFolderPath);
      groups.forEach((group) => {
        if (group.images.length === 0) return;

        const isExpanded = !collapsedRecursiveFolders.has(group.path);
        rows.push({ type: 'header', path: group.path, count: group.images.length, isExpanded });

        if (isExpanded) {
          if (isJustified) {
            rows.push(
              ...buildJustifiedRows(group.images, ratioMapRef.current, availableWidth, minThumbWidth, ITEM_GAP),
            );
          } else {
            for (let i = 0; i < group.images.length; i += columnCount) {
              rows.push({
                type: 'images',
                images: group.images.slice(i, i + columnCount),
                startIndex: i,
              });
            }
          }
        }
      });
    } else {
      if (isJustified) {
        rows.push(...buildJustifiedRows(imageList, ratioMapRef.current, availableWidth, minThumbWidth, ITEM_GAP));
      } else {
        for (let i = 0; i < imageList.length; i += columnCount) {
          rows.push({
            type: 'images',
            images: imageList.slice(i, i + columnCount),
            startIndex: i,
          });
        }
      }
    }

    rows.push({ type: 'footer' });

    return {
      rows,
      itemWidth,
      rowHeight,
      listRowHeight,
      OUTER_PADDING,
      ITEM_GAP,
      columnCount,
      isListView,
      headerHeight,
    };
  }, [
    gridSize.width,
    imageList,
    libraryViewMode,
    libraryDisplayMode,
    collapsedRecursiveFolders,
    thumbnailSize,
    listColumnWidths.thumbnail,
    currentFolderPath,
    thumbnailSizeOptions,
    thumbnailAspectRatio,
    ratioMapVersion,
  ]);

  useEffect(() => {
    if (!listHandle?.element || !gridData) return;

    const savedTop = useLibraryStore.getState().libraryScrollTop;
    const element = listHandle.element as HTMLElement;

    if (savedTop > 0) {
      element.scrollTop = savedTop;
    }
  }, [listHandle, currentFolderPath]);

  const getItemSize = useCallback(
    (index: number) => {
      if (!gridData) return 0;
      const row = gridData.rows[index];
      if (!row) return 0;
      if (row.type === 'footer') return gridData.isListView ? 24 : gridData.OUTER_PADDING;
      if (row.type === 'header') return gridData.headerHeight;
      return gridData.isListView ? gridData.listRowHeight : (row.rowHeight || gridData.itemWidth) + gridData.ITEM_GAP;
    },
    [gridData],
  );

  const prevActivePath = useRef<string | null>(null);
  const prevDisplayMode = useRef<LibraryDisplayMode | null>(null);
  const prevListElement = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const isClick = isClickSelectionRef.current;
    isClickSelectionRef.current = false;

    if (!listHandle?.element || !gridData || multiSelectedPaths.length > 1) {
      prevActivePath.current = activePath;
      prevDisplayMode.current = libraryDisplayMode;
      if (listHandle?.element) prevListElement.current = listHandle.element as HTMLElement;
      return;
    }

    const element = listHandle.element as HTMLElement;
    const isPathSame = activePath === prevActivePath.current;
    const isModeSame = libraryDisplayMode === prevDisplayMode.current;
    const isElementSame = element === prevListElement.current;

    if (isPathSame && isModeSame && isElementSame) return;

    prevActivePath.current = activePath;
    prevDisplayMode.current = libraryDisplayMode;
    prevListElement.current = element;

    if (isClick && isModeSame && isElementSame) {
      return;
    }

    let targetTop = 0;
    let targetRowHeight = gridData.rowHeight;
    let found = false;

    for (let i = 0; i < gridData.rows.length; i++) {
      const row = gridData.rows[i];
      const currentRowHeight = getItemSize(i);

      if (row.type === 'images') {
        const hasImage = row.images.some((img) => img.path === activePath);
        if (hasImage) {
          targetRowHeight = currentRowHeight;
          found = true;
          break;
        }
      }

      targetTop += currentRowHeight;
    }

    if (found) {
      const clientHeight = element.clientHeight;
      const scrollTop = element.scrollTop;
      const itemBottom = targetTop + targetRowHeight;
      const SCROLL_OFFSET = 120;

      if (!isModeSame || !isElementSame) {
        element.scrollTo({
          top: Math.max(0, targetTop - clientHeight / 2 + targetRowHeight / 2),
          behavior: 'instant',
        });
      } else if (itemBottom > scrollTop + clientHeight) {
        element.scrollTo({
          top: Math.min(targetTop, itemBottom - clientHeight + SCROLL_OFFSET),
          behavior: 'smooth',
        });
      } else if (targetTop < scrollTop) {
        element.scrollTo({
          top: Math.max(0, targetTop - SCROLL_OFFSET),
          behavior: 'smooth',
        });
      }
    }
  }, [activePath, gridData, getItemSize, multiSelectedPaths.length, listHandle, libraryDisplayMode]);

  const memoizedRowProps = useMemo(() => {
    if (!gridData) return {};

    return {
      rows: gridData.rows,
      activePath,
      multiSelectedSet: new Set(multiSelectedPaths),
      onContextMenu,
      onImageClick: handleImageClick,
      onImageDoubleClick: handleImageDoubleClick,
      thumbnailAspectRatio,
      onImageLoad: handleImageLoad,
      imageRatings,
      baseFolderPath: currentFolderPath,
      itemWidth: gridData.itemWidth,
      itemHeight: gridData.isListView ? gridData.listRowHeight : gridData.itemWidth,
      outerPadding: gridData.OUTER_PADDING,
      gap: gridData.ITEM_GAP,
      isListView: gridData.isListView,
      columnWidths: listColumnWidths,
      queueThumbnailRequest,
      onToggleRecursiveFolder: handleToggleRecursiveFolder,
      groupBadgeInfo,
      onAspectRatioLoaded: handleAspectRatioLoaded,
    };
  }, [
    gridData,
    activePath,
    multiSelectedPaths,
    onContextMenu,
    handleImageClick,
    handleImageDoubleClick,
    thumbnailAspectRatio,
    handleImageLoad,
    imageRatings,
    currentFolderPath,
    listColumnWidths,
    queueThumbnailRequest,
    handleToggleRecursiveFolder,
    groupBadgeInfo,
    handleAspectRatioLoaded,
  ]);

  if (!gridData) {
    return (
      <div
        ref={libraryContainerRef}
        className="flex-1 w-full h-full min-h-0 overflow-hidden"
        onClick={props.onClearSelection}
        onContextMenu={props.onEmptyAreaContextMenu}
      />
    );
  }

  const handleHeaderSort = (key: string) => {
    props.onClearSelection();
    setSortCriteria((prev) => {
      if (prev.key === key) {
        if (prev.order === SortDirection.Ascending) {
          return { ...prev, order: SortDirection.Descending };
        } else {
          return { key: 'name', order: SortDirection.Ascending };
        }
      }
      return { key, order: SortDirection.Ascending };
    });
  };

  return (
    <div
      ref={libraryContainerRef}
      className="flex-1 w-full h-full min-h-0 overflow-hidden"
      onClick={props.onClearSelection}
      onContextMenu={props.onEmptyAreaContextMenu}
    >
      <div className="flex flex-col w-full h-full">
        {gridData.isListView && (
          <ListHeader
            widths={listColumnWidths}
            setWidths={(widths) =>
              setLibrary({ listColumnWidths: typeof widths === 'function' ? widths(listColumnWidths) : widths })
            }
            containerRef={libraryContainerRef}
            sortCriteria={sortCriteria}
            onSortChange={handleHeaderSort}
          />
        )}
        <div style={{ height: gridData.isListView ? gridSize.height - 36 : gridSize.height, width: gridSize.width }}>
          <List
            listRef={setListHandle}
            rowCount={gridData.rows.length}
            rowHeight={getItemSize}
            onScroll={(e: React.UIEvent<HTMLElement>) => handleScroll(e.currentTarget.scrollTop)}
            className="custom-scrollbar"
            rowComponent={Row}
            rowProps={memoizedRowProps}
          />
        </div>
      </div>
    </div>
  );
}
