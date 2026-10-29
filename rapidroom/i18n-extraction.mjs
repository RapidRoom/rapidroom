import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { deepMerge } from '../src/i18n/rapidroom.mjs';

const read = (filename) => JSON.parse(readFileSync(filename, 'utf8'));
const sorted = (value) =>
  isObject(value)
    ? Object.fromEntries(
        Object.keys(value)
          .sort()
          .map((key) => [key, sorted(value[key])]),
      )
    : value;
const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);

export function localeOverlay(upstream, merged) {
  return Object.fromEntries(
    Object.entries(merged)
      .filter(([key, value]) => !isDeepStrictEqual(upstream[key], value))
      .map(([key, value]) => [
        key,
        isObject(upstream[key]) && isObject(value) ? localeOverlay(upstream[key], value) : value,
      ]),
  );
}

// The CLI compares/extracts complete catalogs in scratch space. Only their
// differences are written back, so extraction never edits upstream-owned files.
export function overlayExtraction(locales, localeDir = 'src/i18n/locales') {
  let scratch;
  return {
    output: (language) => {
      if (!scratch) throw new Error('Locale extraction scratch catalogs are not initialized');
      return path.join(scratch, `${language}.json`);
    },
    plugin: {
      name: 'rapidroom-locale-overlays',
      onEnd() {
        if (scratch) rmSync(scratch, { recursive: true, force: true });
        scratch = mkdtempSync(path.join(tmpdir(), 'rapidroom-i18n-'));
        for (const locale of locales) {
          const filename = `${locale}.json`;
          const merged = deepMerge(
            read(path.join(localeDir, filename)),
            read(path.join(localeDir, 'rapidroom', filename)),
          );
          writeFileSync(path.join(scratch, filename), JSON.stringify(sorted(merged), null, 2) + '\n');
        }
      },
      afterSync(results) {
        try {
          if (process.argv.includes('--dry-run') || process.argv.includes('--ci')) return;
          for (const result of results) {
            if (!result.updated) continue;
            const filename = `${result.locale}.json`;
            const overlay = localeOverlay(read(path.join(localeDir, filename)), result.newTranslations);
            writeFileSync(path.join(localeDir, 'rapidroom', filename), JSON.stringify(overlay, null, 2) + '\n');
          }
        } finally {
          rmSync(scratch, { recursive: true, force: true });
          scratch = undefined;
        }
      },
    },
  };
}
