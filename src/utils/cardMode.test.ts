import { describe, expect, it } from 'vitest';
import { isPathInCardRoot } from './cardMode';

describe('isPathInCardRoot', () => {
  it('matches the card root, files under it and virtual copies', () => {
    expect(isPathInCardRoot('/media/EOS_DIGITAL', '/media/EOS_DIGITAL')).toBe(true);
    expect(isPathInCardRoot('/media/EOS_DIGITAL/DCIM/IMG_0001.CR3', '/media/EOS_DIGITAL/')).toBe(true);
    expect(isPathInCardRoot('/media/EOS_DIGITAL/DCIM/IMG_0001.CR3?vc=a1b2c3', '/media/EOS_DIGITAL')).toBe(true);
    expect(isPathInCardRoot('E:\\DCIM\\IMG_0001.CR3', 'E:\\')).toBe(true);
  });

  it('does not match other folders or when Card mode is off', () => {
    expect(isPathInCardRoot('/media/EOS_DIGITAL2/IMG_0001.CR3', '/media/EOS_DIGITAL')).toBe(false);
    expect(isPathInCardRoot('/home/me/Pictures/IMG_0001.CR3', '/media/EOS_DIGITAL')).toBe(false);
    expect(isPathInCardRoot('/media/EOS_DIGITAL/DCIM/IMG_0001.CR3', null)).toBe(false);
    expect(isPathInCardRoot(null, '/media/EOS_DIGITAL')).toBe(false);
  });
});
