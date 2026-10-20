// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CullingPreview } from './CullingView';
import { useProcessStore } from '../../../store/useProcessStore';
import type { ImageFile } from '../../ui/AppProperties';

const animation = vi.hoisted(() => ({ complete: undefined as (() => void) | undefined }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../../hooks/useLibraryActions', () => ({
  useLibraryActions: () => ({
    handleRate: vi.fn(),
    handleToggleFlag: vi.fn(),
    handleSetColorLabel: vi.fn(),
    handleTagsChanged: vi.fn(),
  }),
}));
vi.mock('framer-motion', async () => {
  const { createElement, forwardRef } = await import('react');
  const elements = new Map<string, unknown>();
  return {
    AnimatePresence: ({ children }: { children: React.ReactNode }) => children,
    motion: new Proxy(
      {},
      {
        get: (_, tag: string) => {
          if (!elements.has(tag))
            elements.set(
              tag,
              forwardRef<HTMLElement, Record<string, unknown>>((props, ref) => {
                const {
                  initial,
                  animate: _animate,
                  exit: _exit,
                  transition,
                  onAnimationComplete,
                  layout: _layout,
                  layoutId: _layoutId,
                  ...rest
                } = props;
                if (tag === 'img') animation.complete = onAnimationComplete as (() => void) | undefined;
                return createElement(tag, {
                  ...rest,
                  ref,
                  ...(tag === 'img'
                    ? {
                        'data-initial': JSON.stringify(initial),
                        'data-duration': (transition as { duration?: number })?.duration,
                      }
                    : {}),
                });
              }),
            );
          return elements.get(tag);
        },
      },
    ),
  };
});
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initial = useProcessStore.getState();
const image: ImageFile = {
  path: '/photo.ARW',
  is_raw: true,
  modified: 0,
  is_edited: false,
  rating: 0,
  flag: null,
  tags: [],
  exif: null,
  is_virtual_copy: false,
  is_cloud_placeholder: false,
  group_id: null,
};
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1000);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(800);
  vi.stubGlobal('URL', Object.assign(class {}, { revokeObjectURL: vi.fn() }));
  useProcessStore.setState({ ...initial, mediumThumbnails: { [image.path]: 'camera.jpg' }, previews: {} }, true);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  useProcessStore.setState(initial, true);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
async function mount() {
  await act(async () =>
    root.render(
      <CullingPreview
        image={image}
        rating={0}
        isActive
        isSelected
        syncViewport={{ isActive: false, zoom: 1, pan: { x: 0, y: 0 }, isDragging: false }}
        setSyncViewport={vi.fn()}
        onContextMenu={vi.fn()}
        onImageDoubleClick={vi.fn()}
        hoveredPath={null}
        setHoveredCullingPath={vi.fn()}
        showRateBar={false}
        setShowRateBar={vi.fn()}
        showInfoBar={false}
        setShowInfoBar={vi.fn()}
      />,
    ),
  );
}
const thumb = () => host.querySelector<HTMLImageElement>('img[alt="library.culling.altThumbnailLoading"]');
const raw = () => host.querySelector<HTMLImageElement>('img[alt="library.culling.altCullingPreviewHighRes"]');

describe('actual CullingPreview paint handoff', () => {
  it('opens a cached RAW directly without showing or fading the camera JPEG', async () => {
    useProcessStore.getState().setPreview(image.path, 'blob:ready', 'camera.jpg', { width: 7008, height: 4672 });
    await mount();
    expect(raw()?.getAttribute('src')).toBe('blob:ready');
    expect(raw()?.getAttribute('data-initial')).toBe('false');
    expect(thumb()).toBeNull();
  });

  it('keeps an instant cold JPEG, aligns its known frame, fades150ms and removes the JPEG after RAW paint', async () => {
    await mount();
    expect(thumb()).not.toBeNull();
    expect(raw()).toBeNull();
    await act(async () =>
      useProcessStore.getState().setPreview(image.path, 'blob:new', 'camera.jpg', { width: 7008, height: 4672 }),
    );
    expect(raw()?.getAttribute('data-initial')).toBe('{"opacity":0}');
    expect(raw()?.getAttribute('data-duration')).toBe('0.15');
    expect(thumb()?.style.objectFit).toBe('cover');
    expect(parseFloat(thumb()!.style.height)).toBeCloseTo(83.33333, 4);
    await act(async () => animation.complete?.());
    expect(thumb()).toBeNull();
    expect(raw()).not.toBeNull();
    // A later thumbnail/metadata revision must restore a real cold transition.
    await act(async () => useProcessStore.setState({ mediumThumbnails: { [image.path]: 'revised.jpg' } }));
    expect(thumb()).not.toBeNull();
    expect(raw()).toBeNull();
    await act(async () =>
      useProcessStore.getState().setPreview(image.path, 'blob:revised', 'revised.jpg', { width: 4672, height: 7008 }),
    );
    expect(raw()?.getAttribute('data-initial')).toBe('{"opacity":0}');
    expect(parseFloat(thumb()!.style.width)).toBeCloseTo(53.33333, 4);
  });

  it('keeps a cached RAW visible when the same thumbnail hash upgrades from small to medium', async () => {
    const hash = 'a'.repeat(64);
    const prefix = 'asset://cache/' + hash;
    useProcessStore.setState({ mediumThumbnails: {}, thumbnails: { [image.path]: prefix + '_small.jpg' } });
    useProcessStore.getState().setPreview(image.path, 'blob:ready', prefix, { width: 7008, height: 4672 });
    await mount();
    expect(raw()?.getAttribute('src')).toBe('blob:ready');
    expect(thumb()).toBeNull();
    await act(async () => useProcessStore.setState({ mediumThumbnails: { [image.path]: prefix + '_medium.jpg' } }));
    expect(raw()?.getAttribute('src')).toBe('blob:ready');
    expect(thumb()).toBeNull();
  });

  it('bounds recent previews at10 and never evicts any of the six displayed images for prefetch', () => {
    const protectedPaths = Array.from({ length: 6 }, (_, i) => 'visible-' + i);
    vi.spyOn(Date, 'now').mockReturnValue(1); // Tied timestamps must not evict the displayed image.
    for (const path of protectedPaths) useProcessStore.getState().setPreview(path, 'blob:' + path, 'key');
    for (let i = 0; i < 20; i++)
      useProcessStore
        .getState()
        .setPreview('prefetch-' + i, 'blob:prefetch-' + i, 'key', { width: 7008, height: 4672 }, protectedPaths);
    const previews = useProcessStore.getState().previews;
    expect(Object.keys(previews)).toHaveLength(10);
    for (const path of protectedPaths) expect(previews[path].url).toBe('blob:' + path);
    expect(URL.revokeObjectURL).not.toHaveBeenCalledWith('blob:visible-0');
  });
});
