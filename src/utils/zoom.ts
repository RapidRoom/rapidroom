// Physical zoom: 1.0 means one image pixel per device pixel.
export const MIN_ZOOM_PERCENT = 0.01;
export const MAX_ZOOM_PERCENT = 4;
const ZOOM_STOPS = [0.01, 0.02, 0.05, 0.1, 0.25, 0.33, 0.5, 0.67, 1, 1.5, 2, 3, MAX_ZOOM_PERCENT];

export interface Size {
  width: number;
  height: number;
}

export const getDpr = (): number => (typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1);
export const fitPercent = (renderScale: number, dpr: number): number => renderScale * dpr;

// Tiny images retain room to zoom to twice their fitted size.
export const maxZoomPercent = (fit = 0): number => Math.max(MAX_ZOOM_PERCENT, fit * 2);
export const clampZoomPercent = (percent: number, fit = 0): number =>
  Math.max(
    Math.min(MIN_ZOOM_PERCENT, fit || MIN_ZOOM_PERCENT),
    Math.min(maxZoomPercent(fit), Number.isFinite(percent) ? percent : 1),
  );

export const percentFromTransform = (renderScale: number, transformScale: number, dpr: number): number =>
  renderScale * transformScale * dpr;
export const transformFromPercent = (percent: number, renderScale: number, dpr: number): number => {
  const fit = fitPercent(renderScale, dpr);
  return fit > 0 ? clampZoomPercent(percent, fit) / fit : 1;
};
export const zoomLimits = (renderScale: number, dpr: number): { minScale: number; maxScale: number } => {
  const fit = fitPercent(renderScale, dpr);
  return fit > 0
    ? { minScale: Math.min(1, MIN_ZOOM_PERCENT / fit), maxScale: maxZoomPercent(fit) / fit }
    : { minScale: 1, maxScale: 2 };
};

export const zoomStops = (fit = 0): number[] => {
  const max = maxZoomPercent(fit);
  return [...new Set([...ZOOM_STOPS.filter((stop) => stop <= max), ...(fit > 0 ? [fit] : []), max])].sort(
    (a, b) => a - b,
  );
};
export const stepZoomIn = (percent: number, fit = 0): number =>
  zoomStops(fit).find((stop) => stop > percent + 1e-6) ?? maxZoomPercent(fit);
export const stepZoomOut = (percent: number, fit = 0): number =>
  zoomStops(fit).findLast((stop) => stop < percent - 1e-6) ?? Math.min(MIN_ZOOM_PERCENT, fit || MIN_ZOOM_PERCENT);

// DOM has no device-type flag. Recognise line/page notches and the common
// 100/120-pixel wheel ticks; fractional/small pixel deltas and pinch stay smooth.
export const isDiscreteWheel = (event: Pick<WheelEvent, 'ctrlKey' | 'deltaMode' | 'deltaX' | 'deltaY'>): boolean => {
  if (event.ctrlKey || event.deltaY === 0 || event.deltaX !== 0) return false;
  const delta = Math.abs(event.deltaY);
  return event.deltaMode !== 0 || (Number.isInteger(delta) && (delta % 100 === 0 || delta % 120 === 0));
};
export const isAtMaxZoom = (scale: number, maximum: number): boolean => scale >= maximum * (1 - 1e-6);

export const zoomReferenceSize = (originalSize: Size, orientationSteps: number, crop?: Size | null): Size => {
  if (crop) return { width: crop.width, height: crop.height };
  return orientationSteps === 1 || orientationSteps === 3
    ? { width: originalSize.height, height: originalSize.width }
    : { width: originalSize.width, height: originalSize.height };
};
