// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import EditorToolbar from './EditorToolbar';
import { useEditorStore } from '../../../store/useEditorStore';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let cleanup: (() => void) | undefined;
afterEach(() => {
  cleanup?.();
  cleanup = undefined;
  useEditorStore.getState().resetHistory(INITIAL_ADJUSTMENTS);
});

describe('recorded history dropdown', () => {
  it('shows the full recorded action, step number and current step, then jumps to a chosen step', async () => {
    const snapshot = { ...INITIAL_ADJUSTMENTS, exposure: 0.35 };
    useEditorStore.getState().pushHistory(snapshot, 'Style: Teal & Orange 60');
    const last = { ...snapshot, contrast: -12 };
    useEditorStore.getState().pushHistory(last, 'Face and neck warmth: Exposure +0.35, Temperature +12');
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const jump = vi.fn();
    const originalScroll = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollIntoView');
    Object.defineProperty(Element.prototype, 'scrollIntoView', { configurable: true, value: vi.fn() });
    cleanup = () => {
      act(() => root.unmount());
      host.remove();
      if (originalScroll) Object.defineProperty(Element.prototype, 'scrollIntoView', originalScroll);
      else Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
    };
    const noop = () => undefined;
    await act(async () =>
      root.render(
        createElement(EditorToolbar, {
          canRedo: false,
          canUndo: true,
          isAndroid: false,
          isLoading: false,
          onBackToLibrary: noop,
          onRedo: noop,
          onToggleFullScreen: noop,
          onToggleShowOriginal: noop,
          onUndo: noop,
          selectedImage: {
            path: '/photos/public.raw',
            width: 6000,
            height: 4000,
            isRaw: true,
            isReady: true,
            thumbnailUrl: '',
            exif: {},
          },
          showOriginal: false,
          adjustmentsHistory: [INITIAL_ADJUSTMENTS, snapshot, last],
          adjustmentsHistoryIndex: 2,
          goToAdjustmentsHistoryIndex: jump,
        }),
      ),
    );
    const undo = host.querySelector('[data-bench-id="undo"]')!;
    await act(async () => undo.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })));
    const menu = host.querySelector('[role="menu"]') as HTMLElement;
    expect(menu).not.toBeNull();
    expect(parseFloat(menu.style.width)).toBeGreaterThanOrEqual(350);
    const items = [...menu.querySelectorAll('[role="menuitem"]')];
    expect(items).toHaveLength(3);
    expect(items[2].getAttribute('title')).toBe('Face and neck warmth: Exposure +0.35, Temperature +12');
    expect(items[2].getAttribute('aria-current')).toBe('step');
    expect(items[2].textContent).toContain('2');
    expect(items[1].textContent).toContain('Style: Teal & Orange 60');
    await act(async () => (items[1] as HTMLButtonElement).click());
    expect(jump).toHaveBeenCalledWith(1);
  });
});
