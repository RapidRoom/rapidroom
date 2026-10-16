import type { AppSettings } from '../components/ui/AppProperties';
import type { InstagramRatio } from '../components/ui/ExportImportProperties';

export const INSTAGRAM_DIMENSIONS: Record<InstagramRatio, readonly [number, number]> = {
  '4:5': [1080, 1350],
  '3:4': [1080, 1440],
  '9:16': [1080, 1920],
  '1.91:1': [1080, 565],
};
export function lastInstagramRatio(settings: AppSettings | null): InstagramRatio {
  const id = settings?.lastInstagramExportPresetId;
  const saved = settings?.exportPresets?.find((preset) => preset.id === id)?.instagramRatio;
  if (saved && saved in INSTAGRAM_DIMENSIONS) return saved;
  return (
    (
      {
        'recipe-instagram-crop-4x5': '4:5',
        'recipe-instagram-crop-9x16': '9:16',
        'recipe-instagram-crop-landscape': '1.91:1',
      } as Record<string, InstagramRatio>
    )[id ?? ''] ?? '3:4'
  );
}
export function centeredInstagramFrame(width: number, height: number, ratio: InstagramRatio) {
  const [rw, rh] = INSTAGRAM_DIMENSIONS[ratio];
  const imageRatio = width / Math.max(height, 1);
  const w = Math.min(1, rw / rh / imageRatio);
  const h = Math.min(1, imageRatio / (rw / rh));
  return { x: (1 - w) / 2, y: (1 - h) / 2, width: w, height: h };
}
