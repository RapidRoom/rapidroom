import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';
import { deepMerge } from './rapidroom.mjs';

it('extracts against inherited and overlaid strings, writes only overlays, and keeps dry runs read-only', () => {
  const fixture = mkdtempSync(path.join(tmpdir(), 'locale-overlay-test-'));
  const localeDir = path.join(fixture, 'locales');
  mkdirSync(path.join(localeDir, 'rapidroom'), { recursive: true });
  const upstream = { inherited: 'Upstream', nested: { sibling: 'Keep me', title: 'Upstream title' } };
  const overlay = { added: 'RapidRoom', nested: { title: 'RapidRoom title' } };
  const source = path.join(fixture, 'app.ts');
  const config = path.join(fixture, 'i18next.config.mjs');
  const plugin = fileURLToPath(new URL('../../rapidroom/i18n-extraction.mjs', import.meta.url));
  const cli = path.resolve('node_modules/i18next-cli/dist/esm/cli.js');
  const catalog = path.join(localeDir, 'en.json');
  const overlayPath = path.join(localeDir, 'rapidroom/en.json');
  const run = (...args) =>
    spawnSync(process.execPath, [cli, 'extract', '--config', config, ...args], { cwd: fixture, encoding: 'utf8' });
  try {
    writeFileSync(catalog, JSON.stringify(upstream));
    writeFileSync(overlayPath, JSON.stringify(overlay));
    const before = readFileSync(catalog);
    writeFileSync(source, "t('inherited'); t('nested.title'); t('nested.sibling'); t('added');\n");
    writeFileSync(
      config,
      `import { overlayExtraction } from ${JSON.stringify(plugin)};\nconst overlays = overlayExtraction(['en'], ${JSON.stringify(localeDir)});\nexport default {locales:['en'],plugins:[overlays.plugin],extract:{input:[${JSON.stringify(source)}],output:overlays.output,mergeNamespaces:true,defaultNS:false,removeUnusedKeys:false,sort:true,defaultValue:''}};\n`,
    );
    const unchanged = run('--ci', '--dry-run');
    expect(unchanged.status, unchanged.stdout + unchanged.stderr).toBe(0);
    writeFileSync(source, "t('inherited'); t('nested.title'); t('nested.sibling'); t('added'); t('newKey');\n");
    const originalOverlay = readFileSync(overlayPath);
    expect(run('--ci', '--dry-run').status).not.toBe(0);
    expect(readFileSync(overlayPath)).toEqual(originalOverlay);
    const extracted = run();
    expect(extracted.status, extracted.stdout + extracted.stderr).toBe(0);
    expect(readFileSync(catalog)).toEqual(before);
    const result = JSON.parse(readFileSync(overlayPath, 'utf8'));
    expect(result).toEqual({ ...overlay, newKey: '' });
    expect(deepMerge(upstream, result)).toEqual({
      inherited: 'Upstream',
      nested: { title: 'RapidRoom title', sibling: 'Keep me' },
      added: 'RapidRoom',
      newKey: '',
    });
    expect(run('--ci', '--dry-run').status).toBe(0);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
}, 30000);
