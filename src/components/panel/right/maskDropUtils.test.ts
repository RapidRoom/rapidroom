import { describe, expect, it } from 'vitest';
import { SubMaskMode } from './Masks';
import { getCreatedSubMaskDropOptions, insertSubMask } from './maskDropUtils';

describe('created sub-mask drop options', () => {
  it('uses additive mode and the index of the row that received the drop', () => {
    const options = getCreatedSubMaskDropOptions([{ id: 'before' }, { id: 'target' }, { id: 'after' }], 'target');

    expect(options).toEqual({ mode: SubMaskMode.Additive, insertIndex: 1 });
    const next = insertSubMask(
      [{ id: 'before' }, { id: 'target' }, { id: 'after' }],
      {
        id: 'added',
        mode: options.mode,
      },
      options.insertIndex,
    );

    expect(next.map(({ id }) => id)).toEqual(['before', 'added', 'target', 'after']);
    expect(next[1].mode).toBe(SubMaskMode.Additive);
  });

  it('falls back to append when a stale target is no longer in the container', () => {
    const options = getCreatedSubMaskDropOptions([{ id: 'current' }], 'removed');

    expect(options).toEqual({ mode: SubMaskMode.Additive, insertIndex: -1 });
  });
});
