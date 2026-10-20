import { invoke } from '@tauri-apps/api/core';
import { Invokes, type SelectedImage } from '../components/ui/AppProperties';
import { createResetAdjustmentsForImage, type Adjustments } from './adjustments';

export async function createResetAdjustmentsWithLensDefaults(
  image: (Pick<SelectedImage, 'path' | 'width' | 'height'> & Partial<Pick<SelectedImage, 'isRaw'>>) | null,
): Promise<Adjustments> {
  const adjustments = createResetAdjustmentsForImage(image);
  if (!image || image.isRaw === false) return adjustments;
  const lens = await invoke<Partial<Adjustments>>(Invokes.GetDefaultRawLensAdjustments, { path: image.path });
  return { ...adjustments, ...lens };
}
