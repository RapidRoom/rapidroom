import type { TransformState } from '../components/ui/AppProperties';

interface Viewport {
  width: number;
  height: number;
  offsetX: number;
  offsetY: number;
  containerWidth: number;
  containerHeight: number;
}

// Upstream 8913b91c: preserve the image point at the viewport centre and its pixel scale.
export function preserveCompareViewport(prev: Viewport, next: Viewport, state: TransformState): TransformState {
  const imgX = ((prev.containerWidth / 2 - state.positionX) / state.scale - prev.offsetX) / prev.width;
  const imgY = ((prev.containerHeight / 2 - state.positionY) / state.scale - prev.offsetY) / prev.height;
  const scale = Math.max(1, (state.scale * prev.width) / next.width);
  return {
    scale,
    positionX: next.containerWidth / 2 - (next.offsetX + imgX * next.width) * scale,
    positionY: next.containerHeight / 2 - (next.offsetY + imgY * next.height) * scale,
  };
}
