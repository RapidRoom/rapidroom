// @vitest-environment jsdom
import { act, createElement, type ComponentProps, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import EditorToolbar from './EditorToolbar';
import { useEditorStore } from '../../../store/useEditorStore';
import { globalImageCache } from '../../../utils/ImageLRUCache';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
vi.mock('../../../hooks/useOsPlatform', () => ({ useOsPlatform: () => 'linux' }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('../../ui/PrintProfileControl', () => ({
  default: ({ onChange }: { onChange(value: null): void }) =>
    createElement('button', { 'data-testid': 'disable-proof', onClick: () => onChange(null) }, 'disable'),
}));
vi.mock('framer-motion', async () => {
  const { createElement, forwardRef } = await import('react');
  return {
    AnimatePresence: ({ children }: { children: ReactNode }) => children,
    motion: {
      div: forwardRef<HTMLDivElement, { children?: ReactNode }>(({ children }, ref) =>
        createElement('div', { ref }, children),
      ),
    },
  };
});
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it.each([false, true])(
  'releases unprotected proof URLs while preserving cache ownership (protected=%s)',
  async (protectedFrame) => {
    vi.useFakeTimers();
    const revoke = vi.fn();
    vi.stubGlobal('URL', Object.assign(class {}, { revokeObjectURL: revoke }));
    const initial = useEditorStore.getState();
    const image = {
      path: '/photo.arw',
      width: 640,
      height: 480,
      exif: null,
      group_id: null,
      isRaw: true,
      isReady: true,
      thumbnailUrl: '',
    };
    if (protectedFrame)
      globalImageCache.set(image.path, {
        adjustments: INITIAL_ADJUSTMENTS,
        histogram: null,
        waveform: null,
        finalPreviewUrl: 'blob:frame',
        uncroppedPreviewUrl: 'blob:crop',
        selectedImage: image,
        originalSize: { width: 640, height: 480 },
        previewSize: { width: 640, height: 480 },
      });
    useEditorStore.setState({
      softProof: { profilePath: '/paper.icc', intent: 'relative', blackPointCompensation: true },
      finalPreviewUrl: 'blob:frame',
      uncroppedAdjustedPreviewUrl: 'blob:crop',
      interactivePatch: { url: 'blob:patch', normX: 0, normY: 0, normW: 1, normH: 1 },
    });
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const props: ComponentProps<typeof EditorToolbar> = {
      canRedo: false,
      canUndo: false,
      isAndroid: false,
      isLoading: false,
      onBackToLibrary: vi.fn(),
      onRedo: vi.fn(),
      onToggleFullScreen: vi.fn(),
      onToggleShowOriginal: vi.fn(),
      onUndo: vi.fn(),
      selectedImage: image,
      showOriginal: false,
      showDateView: false,
      onToggleDateView: vi.fn(),
      adjustmentsHistory: [],
      adjustmentsHistoryIndex: 0,
      goToAdjustmentsHistoryIndex: vi.fn(),
    };
    try {
      await act(async () => root.render(createElement(EditorToolbar, props)));
      await act(async () => host.querySelector<HTMLButtonElement>('[data-testid=disable-proof]')!.click());
      expect(useEditorStore.getState().finalPreviewUrl).toBeNull();
      expect(useEditorStore.getState().uncroppedAdjustedPreviewUrl).toBeNull();
      expect(useEditorStore.getState().interactivePatch).toBeNull();
      await act(async () => vi.advanceTimersByTime(1000));
      const released = revoke.mock.calls.map((call) => call[0]);
      expect(released).toContain('blob:patch');
      if (protectedFrame) {
        expect(released).not.toContain('blob:frame');
        expect(released).not.toContain('blob:crop');
      } else {
        expect(released).toContain('blob:frame');
        expect(released).toContain('blob:crop');
      }
    } finally {
      await act(async () => root.unmount());
      host.remove();
      globalImageCache.clear();
      useEditorStore.setState(initial, true);
      vi.unstubAllGlobals();
      vi.useRealTimers();
    }
  },
);
