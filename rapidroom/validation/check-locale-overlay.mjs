// One-off migration proof: node rapidroom/validation/check-locale-overlay.mjs BASE UPSTREAM
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { deepMerge } from '../../src/i18n/rapidroom.mjs';

const [base, upstream] = process.argv.slice(2);
assert(base && upstream, 'Pass the original RapidRoom main and upstream commit SHAs');
const git = (...args) => execFileSync('git', args);
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const locales = readdirSync('src/i18n/locales')
  .filter((name) => name.endsWith('.json'))
  .sort();
assert.equal(locales.length, 16);
const rows = locales.map((filename) => {
  const path = `src/i18n/locales/${filename}`;
  const previous = git('show', `${base}:${path}`);
  const original = git('show', `${upstream}:${path}`);
  const current = readFileSync(path);
  assert.deepEqual(current, original, `${filename}: upstream bytes changed`);
  const overlay = JSON.parse(readFileSync(`src/i18n/locales/rapidroom/${filename}`, 'utf8'));
  assert.deepEqual(
    deepMerge(JSON.parse(current), overlay),
    JSON.parse(previous),
    `${filename}: effective catalog changed`,
  );
  return {
    locale: filename.slice(0, -5),
    previous_sha256: sha256(previous),
    upstream_sha256: sha256(original),
    merged_equals_previous: true,
  };
});
console.log(JSON.stringify({ rapidroom_base: base, upstream_commit: upstream, locales: rows }, null, 2));
