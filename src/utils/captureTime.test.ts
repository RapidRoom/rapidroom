import { describe, expect, it } from 'vitest';
import {
  captureClock,
  captureSortTime,
  displayCaptureClock,
  folderCaptureShift,
  parseCaptureOffset,
} from './captureTime';

const hawaii = { DateTimeOriginal: '2026:10:19 23:30:00', OffsetTimeOriginal: '-10:00' };

describe('capture clock and recorded zones', () => {
  it('preserves camera time independent of laptop zone and rolls a display-only Japan shift forward', () => {
    expect(displayCaptureClock(hawaii)).toEqual({ date: '2026-10-19', time: '23:30:00', zone: 'UTC-10:00' });
    expect(displayCaptureClock(hawaii, 19 * 60)).toEqual({ date: '2026-10-20', time: '18:30:00', zone: 'UTC-10:00' });
    expect(hawaii).toEqual({ DateTimeOriginal: '2026:10:19 23:30:00', OffsetTimeOriginal: '-10:00' });
  });
  it('orders true instants even when the camera-clock dates are reversed', () => {
    const japan = { DateTimeOriginal: '2026-10-20 12:00:00', OffsetTimeOriginal: '+09:00' };
    expect(captureSortTime(japan)).toBe(Date.parse('2026-10-20T03:00:00Z'));
    expect(captureSortTime(hawaii)).toBe(Date.parse('2026-10-20T09:30:00Z'));
    expect(captureSortTime(japan)!).toBeLessThan(captureSortTime(hawaii)!);
    expect(captureSortTime({ DateTimeOriginal: '2026-10-20 18:30:00', OffsetTimeOriginal: '+09:00' })).toBe(
      captureSortTime(hawaii),
    );
  });
  it('keeps legacy clock order without a zone and accepts fractional zones with a safe fallback', () => {
    expect(displayCaptureClock({ DateTimeOriginal: hawaii.DateTimeOriginal })).toMatchObject({
      time: '23:30:00',
      zone: null,
    });
    expect(parseCaptureOffset('+05:45')).toBe(345);
    expect(parseCaptureOffset('-03:30')).toBe(-210);
    for (const invalid of ['+24:00', '-10:60', '9:00', 'bad']) expect(parseCaptureOffset(invalid)).toBeNull();
    expect(captureClock({ ...hawaii, OffsetTimeOriginal: 'bad', OffsetTime: '+09:00' })?.offset).toBe(540);
    expect(captureClock({ DateTimeOriginal: '2026:02:30 12:00:00' })).toBeNull();
  });
  it('shifts only the source folder, including virtual copies and Windows paths', () => {
    const shifts = { '/trip': 1140, '/trip/day': -60, 'C:/photos': 30 };
    expect(folderCaptureShift('/trip/a.ARW?vc=copy', shifts)).toBe(1140);
    expect(folderCaptureShift('/trip/day/b.jpg', shifts)).toBe(-60);
    expect(folderCaptureShift('/trip/other/b.jpg', shifts)).toBe(0);
    expect(folderCaptureShift('C:\\photos\\a.ARW', shifts)).toBe(30);
    expect(folderCaptureShift('/trip/a.ARW', { '/trip': Infinity })).toBe(0);
    expect(folderCaptureShift('/trip/a.ARW', { '/trip': 2881 })).toBe(0);
  });
});
