import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent, type ReactNode } from 'react';
import { useDroppable } from '@dnd-kit/core';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { useUIStore } from '../../store/useUIStore';
import { Panel } from '../ui/AppProperties';
import PanelSwitcher from './PanelSwitcher';
import AgentLauncher from './AgentLauncher';
import { clampDockHeight } from '../../utils/dockHeight';
import { DockHeaderContext } from './DockHeaderContext';

export default function BottomDock({ renderPanel }: { renderPanel: (panel: Panel) => ReactNode }) {
  const { t } = useTranslation();
  const height = useUIStore((state) => state.bottomDockHeight);
  const expanded = useUIStore((state) => state.bottomDockExpanded);
  const activeView = useUIStore((state) => state.activeView);
  const dock = useRef<HTMLElement | null>(null);
  const [availableHeight, setAvailableHeight] = useState(window.innerHeight);
  const active = useUIStore((state) => state.activePanels.bottom);
  const panels = useUIStore((state) => state.panelLayout.bottom);
  const lastActive = useRef<Panel | null>(null);
  const [viewportHeight, setViewportHeight] = useState(window.innerHeight);
  const [headerSlot, setHeaderSlot] = useState<HTMLDivElement | null>(null);
  useEffect(() => {
    const resize = () => setViewportHeight(window.innerHeight);
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);
  useEffect(() => {
    if (active) lastActive.current = active;
  }, [active]);
  useLayoutEffect(() => {
    const viewport = activeView === 'editor' ? document.querySelector('[data-editor-viewport]') : null;
    const measure = () => {
      const bounds = viewport?.getBoundingClientRect();
      const available =
        bounds && bounds.width > 0 && dock.current
          ? bounds.height + dock.current.getBoundingClientRect().height
          : window.innerHeight;
      setAvailableHeight((old) => (Math.abs(old - available) > 1 ? available : old));
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (viewport) observer.observe(viewport);
    if (dock.current) observer.observe(dock.current);
    window.addEventListener('resize', measure);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [activeView, active]);
  const renderedHeight = clampDockHeight(expanded ? Number.MAX_SAFE_INTEGER : height, viewportHeight, availableHeight);
  const toggleHeight = () => useUIStore.getState().toggleDockHeight();
  const toggle = () => {
    const panel = panels.find((panel) => panel === lastActive.current) ?? panels[0] ?? null;
    useUIStore.getState().setUI((state) => ({
      activePanels: { ...state.activePanels, bottom: state.activePanels.bottom ? null : panel },
      bottomDockExpanded: state.activePanels.bottom ? false : state.bottomDockExpanded,
    }));
  };
  const toggleLabel = t(active ? 'terminal.collapseDock' : 'terminal.expandDock');
  const { setNodeRef, isOver } = useDroppable({
    id: 'layout-region-bottom',
    data: { type: 'layout-region', region: 'bottom' },
  });
  const resize = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    const start = event.clientY;
    const startHeight = renderedHeight;
    const element = event.currentTarget;
    const move = (next: globalThis.PointerEvent) =>
      useUIStore.getState().setUI({
        bottomDockExpanded: false,
        bottomDockHeight: clampDockHeight(startHeight + start - next.clientY, window.innerHeight, availableHeight),
      });
    const end = () => {
      element.removeEventListener('pointermove', move);
      element.removeEventListener('pointerup', end);
      element.removeEventListener('pointercancel', end);
      element.removeEventListener('lostpointercapture', end);
    };
    element.addEventListener('pointermove', move);
    element.addEventListener('pointerup', end);
    element.addEventListener('pointercancel', end);
    element.addEventListener('lostpointercapture', end);
  };
  return (
    <DockHeaderContext.Provider value={{ element: headerSlot, onToggle: toggle }}>
      <aside
        ref={(element) => {
          dock.current = element;
          setNodeRef(element);
        }}
        data-bottom-dock
        data-layout-region="bottom"
        className={`lights-out-chrome shrink-0 flex flex-col min-h-0 rounded-lg bg-bg-secondary ${isOver ? 'ring-2 ring-accent' : ''}`}
        style={{ height: active ? renderedHeight : 36 }}
      >
        <div
          role="separator"
          aria-label={t('terminal.resizeDock')}
          aria-orientation="horizontal"
          tabIndex={active ? 0 : -1}
          className={`h-1 shrink-0 ${active ? 'cursor-row-resize bg-border-color' : ''}`}
          title={t('terminal.toggleDockHeightHint')}
          onDoubleClick={active ? toggleHeight : undefined}
          onPointerDown={active ? resize : undefined}
          onKeyDown={(event) => {
            if (active && ['Enter', ' '].includes(event.key)) {
              event.preventDefault();
              toggleHeight();
            } else if (active && ['ArrowUp', 'ArrowDown'].includes(event.key)) {
              event.preventDefault();
              useUIStore.getState().setUI({
                bottomDockExpanded: false,
                bottomDockHeight: clampDockHeight(
                  renderedHeight + (event.key === 'ArrowUp' ? 20 : -20),
                  window.innerHeight,
                  availableHeight,
                ),
              });
            }
          }}
        />
        <div data-dock-header className="flex h-8 shrink-0 items-center cursor-pointer" onClick={toggle}>
          <button
            type="button"
            data-dock-toggle
            aria-label={toggleLabel}
            aria-expanded={!!active}
            aria-controls="bottom-dock-content"
            data-tooltip={toggleLabel}
            disabled={!panels.length}
            className="h-7 w-7 p-1 flex items-center justify-center shrink-0 rounded hover:bg-surface disabled:opacity-40"
            onClick={(event) => {
              event.stopPropagation();
              toggle();
            }}
          >
            {active ? <ChevronDown size={18} /> : <ChevronUp size={18} />}
          </button>
          <div onClick={(event) => event.stopPropagation()}>
            <PanelSwitcher region="bottom" side="bottom" placement="top" compact />
          </div>
          <div ref={setHeaderSlot} data-dock-header-slot className="flex-1 min-w-0 self-stretch" />
          <div onClick={(event) => event.stopPropagation()}>
            <AgentLauncher />
          </div>
          {!panels.length && <span className="text-xs text-text-secondary px-2">{t('terminal.dropPanel')}</span>}
        </div>
        {active && (
          <div id="bottom-dock-content" className="flex-1 min-h-0">
            {renderPanel(active)}
          </div>
        )}
      </aside>
    </DockHeaderContext.Provider>
  );
}
