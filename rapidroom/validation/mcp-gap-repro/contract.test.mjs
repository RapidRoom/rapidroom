import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expectedChecks, manifest, publicSummary, verifyPhoto } from './contract.mjs';

const binarySha = 'a'.repeat(64),
  scriptSha = 'b'.repeat(64);
const valid = {
  checks: expectedChecks.map((id) => ({ id, passed: true })),
  binarySha,
  scriptSha,
  build: { source_commit: manifest.fork_commit, source_clean: true, binary_sha256: binarySha },
  privateStatus: 'passed',
  hostVersion: '0.2.0',
  bridgeVersion: '1.2.0',
};

test('a report cannot pass with missing, duplicated or failed assertions or private close failures', () => {
  assert.equal(publicSummary(valid).passed, true);
  for (const changes of [
    { checks: valid.checks.slice(1) },
    { checks: [...valid.checks, valid.checks[0]] },
    { checks: valid.checks.map((c, i) => (i ? c : { ...c, passed: false })) },
    { privateStatus: 'failed' },
    { build: { ...valid.build, binary_sha256: 'c'.repeat(64) } },
    { build: { ...valid.build, source_clean: false } },
  ])
    assert.equal(publicSummary({ ...valid, ...changes }).passed, false);
});
test('native metadata, paths, errors and malformed version strings cannot leak into public reports', () => {
  const polluted = {
    ...valid,
    native: { token: 'SECRET', path: '/home/private/photo.ARW' },
    hostVersion: '/home/private/token',
    bridgeVersion: 'SECRET',
    checks: valid.checks.map((c) => ({ ...c, path: '/home/private', error: 'SECRET' })),
  };
  const text = JSON.stringify(publicSummary(polluted));
  assert.ok(!text.includes('/home/private') && !text.includes('SECRET'));
});
test('wrong or edited photographs fail before any native call', () => {
  assert.throws(() => verifyPhoto(Buffer.alloc(1)), /size/);
  assert.throws(() => verifyPhoto(Buffer.alloc(manifest.photo.size_bytes)), /checksum/);
});

test('downloader refuses an existing file before networking and preserves its bytes', () => {
  const dir = mkdtempSync(join(tmpdir(), 'rapidroom-public-repro-'));
  const target = join(dir, 'keep.ARW');
  try {
    writeFileSync(target, 'do not replace');
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./fetch.mjs', import.meta.url)), target], {
      timeout: 2000,
    });
    assert.notEqual(result.status, 0);
    assert.equal(result.error, undefined);
    assert.equal(readFileSync(target, 'utf8'), 'do not replace');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
