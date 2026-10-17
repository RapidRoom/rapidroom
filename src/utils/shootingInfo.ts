/** EXIF values already come from the loaded photo; do not request or alter metadata. */
export function shootingInfoParts(exif?: Record<string, unknown> | null) {
  const text = (value: unknown) => (typeof value === 'string' || typeof value === 'number' ? String(value).trim() : '');
  const mm = (value: unknown) => {
    const v = text(value);
    return v ? (/mm$/i.test(v) ? v.replace(/\s*mm$/i, ' mm') : `${v} mm`) : '';
  };
  const focal = mm(exif?.FocalLength);
  const equivalent = mm(exif?.FocalLengthIn35mmFilm);
  const focalNumber = Number.parseFloat(focal);
  const equivalentNumber = Number.parseFloat(equivalent);
  const focalInfo =
    focal && equivalent && focalNumber !== equivalentNumber ? `${focal} (${equivalent} eq.)` : focal || equivalent;
  const aperture = text(exif?.FNumber).replace(/^f\s*\/?\s*/i, '');
  const shutter = text(exif?.ExposureTime);
  const iso = text(exif?.PhotographicSensitivity) || text(exif?.ISO);
  return {
    lens: text(exif?.LensModel),
    details: [focalInfo, aperture && `f/${aperture}`, shutter, iso && `ISO ${iso}`].filter(Boolean).join(' · '),
  };
}
