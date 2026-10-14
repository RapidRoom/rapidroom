import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { loadChanges, checkPullRequest, checkLegacyFile } from './status-records.mjs';

const record = (id, landed = '2026-10-13') => ({
  id,
  title: id,
  category: 'fix',
  landed,
  authors: ['yojen7'],
  source: 'rapidroom',
  upstream_issues: [],
  upstream_prs: [],
  rendering_change: false,
});
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'rapidroom-records-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const dir = join(root, 'rapidroom/changes');
  mkdirSync(dir, { recursive: true });
  const put = (name, value) => writeFileSync(join(dir, name), JSON.stringify(value));
  return { root, dir, put };
}

test('independent records retain legacy order and deterministically order new entries', (t) => {
  const { dir, put } = fixture(t);
  put('legacy-000-oldest-first.json', record('oldest-first', '2026-01-01'));
  put('legacy-001-old-second.json', record('old-second', '2026-10-13'));
  put('272-newer.json', record('newer'));
  put('271-new.json', record('new', '2026-10-12'));
  assert.deepEqual(
    loadChanges(dir).map((c) => c.id),
    ['newer', 'new', 'oldest-first', 'old-second'],
  );
});

test('invalid PR record cannot hide behind valid historical records', (t) => {
  const { dir, put } = fixture(t);
  put('legacy-000-historical.json', record('historical'));
  for (const bad of [
    { ...record('new'), rendering_change: 'false' },
    { ...record('new'), landed: '2026-02-30' },
    { ...record('new'), authors: [] },
    { ...record('new'), typo: true },
    { ...record('new'), upstream_issues: ['not-a-ref'] },
    { id: 'new' },
    [record('new')],
  ]) {
    put('271-new.json', bad);
    assert.throws(() => loadChanges(dir));
  }
});

test('duplicate IDs, malformed JSON and malformed names fail closed', (t) => {
  const { dir, put } = fixture(t);
  put('271-new.json', record('same'));
  put('272-other.json', record('same'));
  assert.throws(() => loadChanges(dir), /Duplicate/);
  rmSync(join(dir, '272-other.json'));
  writeFileSync(join(dir, '272-other.json'), '{');
  assert.throws(() => loadChanges(dir), SyntaxError);
  rmSync(join(dir, '272-other.json'));
  put('shared.json', record('shared'));
  assert.throws(() => loadChanges(dir), /filename/);
});

function gitFixture(t, legacy) {
  const f = fixture(t);
  const git = (...args) =>
    execFileSync('git', args, { cwd: f.root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  mkdirSync(join(f.root, '.github'));
  writeFileSync(join(f.root, 'CHANGES.md'), 'Release snapshot\n');
  writeFileSync(
    join(f.root, '.github/README.md'),
    'Editable introduction\n<!-- rapidroom-changes:start -->\nSnapshot\n<!-- rapidroom-changes:end -->\n',
  );
  if (legacy)
    writeFileSync(
      join(f.root, 'rapidroom/changes.json'),
      JSON.stringify({ changes: Array.isArray(legacy) ? legacy : [record('legacy')] }),
    );
  git('init');
  git('add', '.');
  git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '-m', 'base');
  return { ...f, base: git('rev-parse', 'HEAD') };
}

test('migration preserves every field from the base and allows new records', (t) => {
  const { root, base } = gitFixture(t, true);
  assert.doesNotThrow(() => checkPullRequest(root, base, [record('legacy'), record('new')]));
  assert.throws(() => checkPullRequest(root, base, []), /dropped/);
  assert.throws(() => checkPullRequest(root, base, [{ ...record('legacy'), rendering_change: true }]), /changed/);
});

test('migration cannot reorder historical records', (t) => {
  const records = [record('first'), record('second')];
  const { root, base } = gitFixture(t, records);
  assert.doesNotThrow(() => checkPullRequest(root, base, records));
  assert.throws(() => checkPullRequest(root, base, records.toReversed()), /display order/);
});

test('PR snapshots are frozen but README edits outside the generated block are allowed', (t) => {
  const { root, base } = gitFixture(t, false);
  assert.doesNotThrow(() => checkPullRequest(root, base, [record('new')]));
  const p = join(root, '.github/README.md');
  writeFileSync(p, readFileSync(p, 'utf8').replace('Editable introduction', 'Edited introduction'));
  assert.doesNotThrow(() => checkPullRequest(root, base, [record('new')]));
  writeFileSync(p, readFileSync(p, 'utf8').replace('Snapshot', 'Changed status'));
  assert.throws(() => checkPullRequest(root, base, []), /status block/);
  writeFileSync(join(root, 'CHANGES.md'), 'New generated changelog');
  assert.throws(() => checkPullRequest(root, base, []), /CHANGES/);
  assert.throws(() => checkPullRequest(root, 'main', []), /exact PR base/);
});

test('retired monolithic record fails with conversion instructions', (t) => {
  const { root } = gitFixture(t, true);
  assert.throws(() => checkLegacyFile(root), /convert your entry/);
});
