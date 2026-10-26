import { SubMaskMode, type SubMask } from './Masks';

export interface CreatedSubMaskDropOptions {
  insertIndex: number;
  mode: SubMaskMode;
}

export function getCreatedSubMaskDropOptions(
  subMasks: readonly Pick<SubMask, 'id'>[],
  targetId: string | number,
): CreatedSubMaskDropOptions {
  return {
    insertIndex: subMasks.findIndex((subMask) => subMask.id === targetId),
    mode: SubMaskMode.Additive,
  };
}

export function insertSubMask<T>(subMasks: readonly T[], subMask: T, insertIndex: number): T[] {
  const nextSubMasks = [...subMasks];
  if (insertIndex >= 0) nextSubMasks.splice(insertIndex, 0, subMask);
  else nextSubMasks.push(subMask);
  return nextSubMasks;
}
