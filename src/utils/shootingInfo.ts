const text = (value: unknown) => (typeof value === 'string' || typeof value === 'number' ? String(value).trim() : '');

const lensAliases: Record<string, string> = {
  SEL35F14GM: 'Sony 35mm f1.4 GM',
  SEL2470GM2: 'Sony 24-70mm f2.8 GM II',
  SEL70200G2: 'Sony 70-200mm f4 Macro G II',
  A058: 'Tamron 35-150mm f2-2.8',
};

/** Display names only. Never change the EXIF used for optical correction or export. */
export function friendlyLensName(value: unknown, maker?: unknown) {
  const raw = text(value);
  if (!raw) return '';
  const code = raw.match(/\b(SEL35F14GM|SEL2470GM2|SEL70200G2|A058[ZS]?)\b/i)?.[1].toUpperCase();
  if (code) return lensAliases[code.replace(/^A058[ZS]$/, 'A058')];

  // FE/E alone is a mount, not a maker: third-party E-mount lenses must not become Sony lenses.
  const brand = `${text(maker)} ${raw}`.match(/\b(Sony|Tamron|Sigma)\b/i)?.[1];
  const sonyDesign = /^(?:FE|E)\s+.*\b(?:GM|G|ZA)\b/i.test(raw);
  const name = brand ? brand[0].toUpperCase() + brand.slice(1).toLowerCase() : sonyDesign ? 'Sony' : '';
  const optical = raw.match(
    /(\d+(?:\.\d+)?(?:\s*[-–]\s*\d+(?:\.\d+)?)?)\s*mm\s+f\s*\/?\s*(\d+(?:\.\d+)?(?:\s*[-–]\s*(?:f\s*\/?\s*)?\d+(?:\.\d+)?)?)/i,
  );
  if (!name || !optical) return raw;
  const focal = optical[1].replace(/\s/g, '').replace(/–/g, '-');
  const aperture = optical[2].replace(/[\sf/]/gi, '').replace(/–/g, '-');
  const suffix = raw.slice(optical.index! + optical[0].length).replace(/\bDi\s+(?:IV|III|II|I)\b/gi, '');
  // Preserve optical design/generation and familiar product families, discard mount/motor/model codes.
  const qualifiers = suffix.match(/\b(?:Macro|GM|G|ZA|IV|III|II|Art|Sports|Contemporary)\b/gi) ?? [];
  if (name === 'Sigma' && /\|\s*A\b/.test(suffix) && !qualifiers.some((q) => /^art$/i.test(q))) qualifiers.push('Art');
  const words = qualifiers.map((q) =>
    /^(macro|art|sports|contemporary)$/i.test(q) ? q[0].toUpperCase() + q.slice(1).toLowerCase() : q.toUpperCase(),
  );
  return [name, `${focal}mm`, `f${aperture}`, ...words].join(' ');
}

export function formatFocalLength(value: unknown) {
  const raw = text(value);
  const match = raw.match(/^(\d+(?:\.\d+)?)\s*(?:mm)?$/i);
  return match ? `${match[1]}mm` : raw;
}

export function formatShutterSpeed(value: unknown) {
  const raw = text(value);
  const clean = raw.replace(/\s*(?:s|sec(?:ond)?s?)$/i, '').trim();
  const fraction = clean.match(/^(\d+(?:\.\d+)?)\s*\/\s*(\d+(?:\.\d+)?)$/);
  const seconds = fraction
    ? Number(fraction[1]) / Number(fraction[2])
    : /^\d*\.?\d+$/.test(clean)
      ? Number(clean)
      : NaN;
  if (!Number.isFinite(seconds) || seconds <= 0) return raw;
  if (seconds >= 1) return `${seconds}s`;
  if (fraction) return `${fraction[1]}/${fraction[2]}`;
  const reciprocal = Math.round(1 / seconds);
  return Math.abs(seconds * reciprocal - 1) < 0.000001 ? `1/${reciprocal}` : clean;
}

/** EXIF values already come from the loaded photo; do not request or alter metadata. */
export function shootingInfoParts(exif?: Record<string, unknown> | null) {
  const focal = formatFocalLength(exif?.FocalLength);
  const equivalent = formatFocalLength(exif?.FocalLengthIn35mmFilm);
  const focalNumber = Number.parseFloat(focal);
  const equivalentNumber = Number.parseFloat(equivalent);
  const focalInfo =
    focal && equivalent && focalNumber !== equivalentNumber ? `${focal} (${equivalent} eq.)` : focal || equivalent;
  const aperture = text(exif?.FNumber).replace(/^f\s*\/?\s*/i, '');
  const shutter = formatShutterSpeed(exif?.ExposureTime);
  const iso = text(exif?.PhotographicSensitivity) || text(exif?.ISO) || text(exif?.ISOSpeedRatings);
  return {
    lens: friendlyLensName(exif?.LensModel, exif?.LensMake),
    details: [focalInfo, aperture && `f/${aperture}`, shutter, iso && `ISO ${iso}`].filter(Boolean).join(' · '),
  };
}
