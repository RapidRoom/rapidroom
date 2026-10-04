import { describe, expect, it } from 'vitest';
import { FlagStatus, ImageFile, ImageFlag } from '../components/ui/AppProperties';
import { matchesFlagStatus, restoreFlags, toggledFlag, withFlag } from './imageFlags';

const image = (path: string, flag: ImageFlag | null = null) => ({ path, flag }) as ImageFile;
const flags = (list: ImageFile[]) => Object.fromEntries(list.map((i) => [i.path, i.flag]));

describe('withFlag', () => {
  const list = [image('/a'), image('/b', ImageFlag.Pick), image('/c', ImageFlag.Reject)];

  it('sets the flag only on the given paths', () => {
    expect(flags(withFlag(list, ['/a', '/c'], ImageFlag.Pick))).toEqual({
      '/a': ImageFlag.Pick,
      '/b': ImageFlag.Pick,
      '/c': ImageFlag.Pick,
    });
    expect(flags(withFlag(list, ['/b'], null))).toEqual({ '/a': null, '/b': null, '/c': ImageFlag.Reject });
  });

  it('with onlyFrom, changes only images that have that flag', () => {
    expect(flags(withFlag(list, ['/a', '/b', '/c'], null, ImageFlag.Reject))).toEqual({
      '/a': null,
      '/b': ImageFlag.Pick,
      '/c': null,
    });
  });

  it('keeps unchanged images as the same objects', () => {
    const updated = withFlag(list, ['/b'], ImageFlag.Pick);
    expect(updated).toEqual(list);
    updated.forEach((img, i) => expect(img).toBe(list[i]));
  });
});

describe('restoreFlags', () => {
  it('puts back the previous flags on the given paths only', () => {
    const before = [image('/a'), image('/b', ImageFlag.Pick), image('/c')];
    const after = withFlag(before, ['/a', '/b', '/c'], ImageFlag.Reject);
    expect(flags(restoreFlags(after, before, ['/a', '/b']))).toEqual({
      '/a': null,
      '/b': ImageFlag.Pick,
      '/c': ImageFlag.Reject,
    });
  });

  it('ignores paths that were not in the previous list', () => {
    const after = [image('/new', ImageFlag.Pick)];
    expect(flags(restoreFlags(after, [], ['/new']))).toEqual({ '/new': ImageFlag.Pick });
  });
});

describe('toggledFlag', () => {
  it('sets a flag, and removes it when it is already set', () => {
    expect(toggledFlag(null, ImageFlag.Pick)).toBe(ImageFlag.Pick);
    expect(toggledFlag(ImageFlag.Reject, ImageFlag.Pick)).toBe(ImageFlag.Pick);
    expect(toggledFlag(ImageFlag.Pick, ImageFlag.Pick)).toBeNull();
  });
});

describe('matchesFlagStatus', () => {
  const cases: Array<[FlagStatus | undefined, Array<ImageFlag | null | undefined>]> = [
    [undefined, [null, undefined, ImageFlag.Pick, ImageFlag.Reject]],
    [FlagStatus.All, [null, undefined, ImageFlag.Pick, ImageFlag.Reject]],
    [FlagStatus.Picked, [ImageFlag.Pick]],
    [FlagStatus.Unflagged, [null, undefined]],
    [FlagStatus.ExcludeRejected, [null, undefined, ImageFlag.Pick]],
    [FlagStatus.Rejected, [ImageFlag.Reject]],
  ];

  it.each(cases)('%s matches exactly %j', (status, expected) => {
    const all = [null, undefined, ImageFlag.Pick, ImageFlag.Reject];
    expect(all.filter((flag) => matchesFlagStatus(flag, status))).toEqual(expected);
  });
});
