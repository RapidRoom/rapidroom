import type { PointerEvent, ReactNode } from 'react';
import { useDroppable } from '@dnd-kit/core';
import { useTranslation } from 'react-i18next';
import { ChevronDown } from 'lucide-react';
import { useUIStore } from '../../store/useUIStore';
import type { Panel } from '../ui/AppProperties';
import PanelSwitcher from './PanelSwitcher';
import AgentLauncher from './AgentLauncher';

export default function BottomDock({ renderPanel }: { renderPanel: (panel: Panel) => ReactNode }) {
  const { t } = useTranslation();
  const height = useUIStore((state) => state.bottomDockHeight);
  const active = useUIStore((state) => state.activePanels.bottom);
  const panels = useUIStore((state) => state.panelLayout.bottom);
  const { setNodeRef, isOver } = useDroppable({
    id: 'layout-region-bottom',
    data: { type: 'layout-region', region: 'bottom' },
  });
  const resize = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    const start = event.clientY;
    const element = event.currentTarget;
    const move = (next: globalThis.PointerEvent) =>
      useUIStore
        .getState()
        .setUI({ bottomDockHeight: Math.max(80, Math.min(window.innerHeight * 0.7, height + start - next.clientY)) });
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
    <aside
      ref={setNodeRef}
      data-bottom-dock
      data-layout-region="bottom"
      className={`lights-out-chrome shrink-0 flex flex-col min-h-0 rounded-lg bg-bg-secondary ${isOver ? 'ring-2 ring-accent' : ''}`}
      style={{ height: active ? height : 40 }}
    >
      {active && (
        <div
          role="separator"
          aria-label={t('terminal.resizeDock')}
          aria-orientation="horizontal"
          tabIndex={0}
          className="h-1 shrink-0 cursor-row-resize bg-border-color"
          onPointerDown={resize}
          onKeyDown={(event) => {
            if (['ArrowUp', 'ArrowDown'].includes(event.key)) {
              event.preventDefault();
              useUIStore.getState().setUI({
                bottomDockHeight: Math.max(80, Math.min(800, height + (event.key === 'ArrowUp' ? 20 : -20))),
              });
            }
          }}
        />
      )}
      <div className="flex shrink-0 items-center justify-between">
        <div className="flex-1 min-w-0">
          <PanelSwitcher region="bottom" side="bottom" placement="top" />
        </div>
        <AgentLauncher />
        {active && (
          <button
            aria-label={t('terminal.collapseDock')}
            className="p-2"
            onClick={() =>
              useUIStore.getState().setUI((state) => ({ activePanels: { ...state.activePanels, bottom: null } }))
            }
          >
            <ChevronDown size={18} />
          </button>
        )}
        {!panels.length && <span className="text-xs text-text-secondary px-2">{t('terminal.dropPanel')}</span>}
      </div>
      {active && <div className="flex-1 min-h-0">{renderPanel(active)}</div>}
    </aside>
  );
}
