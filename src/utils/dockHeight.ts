export const DEFAULT_DOCK_HEIGHT = 240;
export const MIN_EDITOR_HEIGHT = 160;

export function clampDockHeight(
  height: number,
  viewportHeight = typeof window === 'undefined' ? 1000 : window.innerHeight,
  availableHeight = viewportHeight,
): number {
  const maximum = Math.max(80, Math.min(viewportHeight * 0.85, availableHeight - MIN_EDITOR_HEIGHT));
  return Math.max(80, Math.min(maximum, Number.isFinite(height) ? height : DEFAULT_DOCK_HEIGHT));
}
