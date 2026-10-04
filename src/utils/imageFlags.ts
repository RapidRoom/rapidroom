import { Ban, Flag } from 'lucide-react';
import { FlagStatus, ImageFile, ImageFlag } from '../components/ui/AppProperties';

export const FLAG_ICONS = {
  [ImageFlag.Pick]: Flag,
  [ImageFlag.Reject]: Ban,
};

export const getImageFlag = (imageList: ImageFile[], path?: string | null): ImageFlag | null =>
  imageList.find((image) => image.path === path)?.flag ?? null;

// Sets `flag` on `paths`; with `onlyFrom`, only on images that currently have that flag.
export const withFlag = (imageList: ImageFile[], paths: string[], flag: ImageFlag | null, onlyFrom?: ImageFlag) => {
  const pathSet = new Set(paths);
  return imageList.map((image) =>
    pathSet.has(image.path) && image.flag !== flag && (!onlyFrom || image.flag === onlyFrom)
      ? { ...image, flag }
      : image,
  );
};

// Puts back the flags `previous` had on `paths`, for when the backend refuses a change.
export const restoreFlags = (imageList: ImageFile[], previous: ImageFile[], paths: string[]) => {
  const pathSet = new Set(paths);
  const before = new Map(previous.filter((image) => pathSet.has(image.path)).map((image) => [image.path, image.flag]));
  return imageList.map((image) =>
    before.has(image.path) && image.flag !== before.get(image.path)
      ? { ...image, flag: before.get(image.path) ?? null }
      : image,
  );
};

export const toggledFlag = (current: ImageFlag | null, flag: ImageFlag): ImageFlag | null =>
  current === flag ? null : flag;

export const matchesFlagStatus = (flag: ImageFlag | null | undefined, status?: FlagStatus) => {
  switch (status) {
    case FlagStatus.Picked:
      return flag === ImageFlag.Pick;
    case FlagStatus.Unflagged:
      return !flag;
    case FlagStatus.ExcludeRejected:
      return flag !== ImageFlag.Reject;
    case FlagStatus.Rejected:
      return flag === ImageFlag.Reject;
    default:
      return true;
  }
};
