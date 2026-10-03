import { describe, expect, it } from 'vitest';
import { computeSortedLibrary } from './useSortedLibrary';
import { RawStatus } from '../components/ui/AppProperties';

const ratings: Record<string, number> = { '/a.raw': 0, '/b.raw': 2, '/c.raw': 3, '/d.raw': 4, '/e.raw': 5 };
const imageList = Object.keys(ratings).map((path, i) => ({ path, modified: i, is_raw: true, tags: [] }));

const filtered = (rating: number, ratingExact?: boolean) =>
  computeSortedLibrary(
    {
      imageList,
      imageRatings: ratings,
      filterCriteria: { colors: [], rating, ratingExact, rawStatus: RawStatus.All },
      searchCriteria: { tags: [], text: '', mode: 'AND' },
      sortCriteria: { key: 'name', order: 'asc' },
    },
    { appSettings: {} },
  ).map((image) => image.path);

describe('rating filter', () => {
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
