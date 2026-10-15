export const DEFAULT_DOCK_HEIGHT = 240;

export function clampDockHeight(height: number, viewportHeight = 1000): number {
  const maximum = Math.max(80, Math.min(400, viewportHeight * 0.45));
  return Math.max(80, Math.min(maximum, Number.isFinite(height) ? height : DEFAULT_DOCK_HEIGHT));
}
