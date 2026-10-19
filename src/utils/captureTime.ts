export type CaptureExif = Record<string, string> | null | undefined;

const clean = (value: string) =>
  value
    .trim()
    .replace(/^["']|["']$/g, '')
    .trim();

export function parseCaptureOffset(value: string | undefined): number | null {
  if (!value) return null;
  const text = clean(value);
  if (text === 'Z') return 0;
  const match = /^([+-])(\d{2}):(\d{2})$/.exec(text);
  if (!match || Number(match[2]) > 23 || Number(match[3]) > 59) return null;
  return (Number(match[2]) * 60 + Number(match[3])) * (match[1] === '-' ? -1 : 1);
}

export function captureClock(exif: CaptureExif) {
  const original = exif?.DateTimeOriginal;
  const text = original || exif?.CreateDate;
  if (!text) return null;
  const match = /^(\d{4})[:-](\d{2})[:-](\d{2})[ T](\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?$/.exec(clean(text));
  if (!match) return null;
  const [, y, m, d, h, min, sec, fraction] = match;
  const date = new Date(0);
  date.setUTCFullYear(Number(y), Number(m) - 1, Number(d));
  date.setUTCHours(Number(h), Number(min), Number(sec), 0);
  if (
    date.getUTCFullYear() !== Number(y) ||
    date.getUTCMonth() !== Number(m) - 1 ||
    date.getUTCDate() !== Number(d) ||
    date.getUTCHours() !== Number(h) ||
    date.getUTCMinutes() !== Number(min) ||
    date.getUTCSeconds() !== Number(sec)
  )
    return null;
  const subsec = fraction ?? exif?.SubSecTimeOriginal ?? '';
  const subseconds = /^\d+$/.test(subsec) ? Number('0.' + subsec.slice(0, 9)) * 1000 : 0;
  const offset =
    [original ? exif?.OffsetTimeOriginal : exif?.OffsetTimeDigitized, exif?.OffsetTime]
      .map(parseCaptureOffset)
      .find((offset) => offset !== null) ?? null;
  return { wall: date.getTime() + subseconds, offset };
}

export function captureSortTime(exif: CaptureExif): number | null {
  const clock = captureClock(exif);
  // With no recorded zone, preserve camera-clock ordering rather than inventing one.
  return clock ? clock.wall - (clock.offset ?? 0) * 60_000 : null;
}

export function captureFolder(path: string): string {
  const source = path.split('?vc=')[0].replace(/\\/g, '/');
  return source.slice(0, source.lastIndexOf('/')) || '/';
}

export function folderCaptureShift(path: string, shifts: Record<string, number> | undefined): number {
  const minutes = shifts?.[captureFolder(path)] ?? 0;
  return Number.isInteger(minutes) && Math.abs(minutes) <= 2880 ? minutes : 0;
}

export function formatClockOffset(minutes: number): string {
  const absolute = Math.abs(minutes);
  return `${minutes < 0 ? '-' : '+'}${String(Math.floor(absolute / 60)).padStart(2, '0')}:${String(absolute % 60).padStart(2, '0')}`;
}

export function displayCaptureClock(exif: CaptureExif, shiftMinutes = 0) {
  const clock = captureClock(exif);
  if (!clock) return null;
  const date = new Date(clock.wall + shiftMinutes * 60_000);
  return {
    date: date.toISOString().slice(0, 10),
    time: date.toISOString().slice(11, 19),
    zone: clock.offset === null ? null : `UTC${formatClockOffset(clock.offset)}`,
  };
}
