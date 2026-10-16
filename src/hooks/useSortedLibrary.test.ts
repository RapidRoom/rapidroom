import { describe, expect, it } from 'vitest';
import { computeSortedLibrary } from './useSortedLibrary';
import { FlagStatus, ImageFlag, RawStatus, type ImageFile } from '../components/ui/AppProperties';

const ratings: Record<string, number> = { '/a.raw': 0, '/b.raw': 2, '/c.raw': 3, '/d.raw': 4, '/e.raw': 5 };
const makeImage = (path: string, modified: number, flag: ImageFile['flag'] = null): ImageFile => ({
  exif: null,
  flag,
  group_id: null,
  is_cloud_placeholder: false,
  is_edited: false,
  is_raw: true,
  is_virtual_copy: false,
  modified,
  path,
  rating: 0,
  tags: [],
});

const imageList = Object.keys(ratings).map((path, i) => makeImage(path, i));

const filtered = (rating: number, ratingExact?: boolean, ratingOperator?: 'gte' | 'eq' | 'lte') =>
  computeSortedLibrary(
    {
      imageList,
      imageRatings: ratings,
      filterCriteria: { colors: [], rating, ratingExact, ratingOperator, rawStatus: RawStatus.All },
      searchCriteria: { tags: [], text: '', mode: 'AND' },
      sortCriteria: { key: 'name', order: 'asc' },
    },
    { appSettings: {} },
  ).map((image) => image.path);

describe('rating filter', () => {
  it('supports upstream operators while respecting old exact preferences unless explicitly overridden', () => {
    expect(filtered(3, false, 'eq')).toEqual(['/c.raw']);
    expect(filtered(3, false, 'lte')).toEqual(['/a.raw', '/b.raw', '/c.raw']);
    expect(filtered(3, true, 'gte')).toEqual(['/c.raw', '/d.raw', '/e.raw']);
  });

  it('shows the rating and higher by default', () => {
    expect(filtered(3)).toEqual(['/c.raw', '/d.raw', '/e.raw']);
    expect(filtered(5)).toEqual(['/e.raw']);
  });

  it('shows only the exact rating when ratingExact is set', () => {
    expect(filtered(3, true)).toEqual(['/c.raw']);
    expect(filtered(2, true)).toEqual(['/b.raw']);
    expect(filtered(1, true)).toEqual([]);
  });

  it('keeps All and Unrated unchanged', () => {
    expect(filtered(0, true)).toHaveLength(5);
    expect(filtered(-1, true)).toEqual(['/a.raw']);
  });
});

describe('flag filter and rating sort', () => {
  const flagged: ImageFile[] = [
    makeImage('/pick.raw', 0, ImageFlag.Pick),
    makeImage('/none.raw', 1),
    makeImage('/reject.raw', 2, ImageFlag.Reject),
    makeImage('/old.raw', 3),
  ];
  const stars: Record<string, number> = { '/pick.raw': 2, '/none.raw': 1, '/reject.raw': 5, '/old.raw': 3 };

  const list = (flagStatus?: FlagStatus, sortCriteria = { key: 'name', order: 'asc' }) =>
    computeSortedLibrary(
      {
        imageList: flagged,
        imageRatings: stars,
        filterCriteria: { colors: [], rating: 0, rawStatus: RawStatus.All, flagStatus },
        searchCriteria: { tags: [], text: '', mode: 'AND' },
        sortCriteria,
      },
      { appSettings: {} },
    ).map((image) => image.path);

  it('filters by flag; images from before flags existed count as unflagged', () => {
    expect(list()).toHaveLength(4);
    expect(list(FlagStatus.All)).toHaveLength(4);
    expect(list(FlagStatus.Picked)).toEqual(['/pick.raw']);
    expect(list(FlagStatus.Unflagged)).toEqual(['/none.raw', '/old.raw']);
    expect(list(FlagStatus.ExcludeRejected)).toEqual(['/none.raw', '/old.raw', '/pick.raw']);
    expect(list(FlagStatus.Rejected)).toEqual(['/reject.raw']);
  });

  it('sorts rejected images below unrated ones by rating, whatever their stars', () => {
    expect(list(FlagStatus.All, { key: 'rating', order: 'desc' })).toEqual([
      '/old.raw',
      '/pick.raw',
      '/none.raw',
      '/reject.raw',
    ]);
  });
});
