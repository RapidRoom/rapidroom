import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { isDeepStrictEqual } from 'node:util';

const required = [
  'id',
  'title',
  'category',
  'landed',
  'authors',
  'source',
  'upstream_issues',
  'upstream_prs',
  'rendering_change',
];
const optional = ['commits', 'notes', 'highlight', 'rapidroom_only'];
const categories = new Set(['fix', 'feature', 'performance', 'privacy', 'platform', 'ci']);
const refPattern = /^[\w.-]+\/[\w.-]+#\d+$/;

function validate(record, filename) {
  const fail = (reason) => {
    throw new Error(`${filename}: ${reason}`);
  };
  if (!record || Array.isArray(record) || typeof record !== 'object') fail('expected one change object');
  for (const key of required) if (!(key in record)) fail(`missing ${key}`);
  for (const key of Object.keys(record)) if (![...required, ...optional].includes(key)) fail(`unknown field ${key}`);
  for (const key of ['id', 'title', 'source', 'landed']) {
    if (typeof record[key] !== 'string' || !record[key].trim()) fail(`${key} must be a nonempty string`);
  }
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(record.id)) fail('invalid id');
  if (!categories.has(record.category)) fail('invalid category');
  if (
    !/^\d{4}-\d{2}-\d{2}$/.test(record.landed) ||
    Number.isNaN(Date.parse(record.landed)) ||
    new Date(record.landed).toISOString().slice(0, 10) !== record.landed
  )
    fail('invalid landed date');
  for (const key of ['authors', 'upstream_issues', 'upstream_prs', 'commits']) {
    if (key === 'commits' && !(key in record)) continue;
    if (!Array.isArray(record[key]) || record[key].some((v) => typeof v !== 'string' || !v.trim()))
      fail(`${key} must be an array of nonempty strings`);
  }
  if (!record.authors.length) fail('authors must not be empty');
  for (const ref of [...record.upstream_issues, ...record.upstream_prs])
    if (!refPattern.test(ref)) fail(`invalid upstream reference ${ref}`);
  for (const url of record.commits || []) {
    try {
      if (new URL(url).protocol !== 'https:') fail('commit URL must use https');
    } catch {
      fail(`invalid commit URL ${url}`);
    }
  }
  if (typeof record.rendering_change !== 'boolean') fail('rendering_change must be boolean');
  if ('rapidroom_only' in record && typeof record.rapidroom_only !== 'boolean') fail('rapidroom_only must be boolean');
  for (const key of ['notes', 'highlight'])
    if (key in record && typeof record[key] !== 'string') fail(`${key} must be a string`);
}

export function loadChanges(directory) {
  const names = readdirSync(directory)
    .filter((name) => name.endsWith('.json'))
    .sort();
  if (!names.length) throw new Error('No changelog records found');
  const ids = new Set();
  const rows = names.map((name) => {
    if (!/^(?:legacy-\d{3}-|[1-9]\d*-)[a-z0-9]+(?:-[a-z0-9]+)*\.json$/.test(name))
      throw new Error(`Invalid record filename: ${name}`);
    const change = JSON.parse(readFileSync(join(directory, name), 'utf8'));
    validate(change, name);
    if (ids.has(change.id)) throw new Error(`Duplicate change id: ${change.id}`);
    ids.add(change.id);
    return { name, change, legacy: name.startsWith('legacy-') };
  });
  // Preserve the old list's exact order; new PR records appear above it.
  rows.sort(
    (a, b) =>
      Number(a.legacy) - Number(b.legacy) ||
      (a.legacy
        ? a.name.localeCompare(b.name)
        : b.change.landed.localeCompare(a.change.landed) || a.name.localeCompare(b.name)),
  );
  return rows.map(({ change }) => change);
}

export function checkPullRequest(root, base, changes) {
  if (!/^[a-f0-9]{40}$/.test(base)) throw new Error('Expected the exact PR base SHA');
  const show = (path) =>
    execFileSync('git', ['show', `${base}:${path}`], {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  let old;
  try {
    old = JSON.parse(show('rapidroom/changes.json'));
  } catch {
    /* Already migrated base. */
  }
  if (old) {
    const byId = new Map(changes.map((change) => [change.id, change]));
    for (const change of old.changes)
      if (!isDeepStrictEqual(change, byId.get(change.id))) throw new Error(`Migration changed or dropped ${change.id}`);
    const oldIds = new Set(old.changes.map((change) => change.id));
    if (
      !isDeepStrictEqual(
        old.changes,
        changes.filter((change) => oldIds.has(change.id)),
      )
    )
      throw new Error('Migration changed historical display order');
    return;
  }
  if (readFileSync(join(root, 'CHANGES.md'), 'utf8') !== show('CHANGES.md'))
    throw new Error('Do not edit generated CHANGES.md in PRs; add a record instead');
  const block = (text) => {
    const match = text.match(/<!-- rapidroom-changes:start -->[\s\S]*?<!-- rapidroom-changes:end -->/);
    if (!match) throw new Error('README generated status markers missing');
    return match[0];
  };
  if (block(readFileSync(join(root, '.github/README.md'), 'utf8')) !== block(show('.github/README.md')))
    throw new Error('Do not edit the generated README status block in PRs; add a record instead');
}

export function checkLegacyFile(root) {
  if (existsSync(join(root, 'rapidroom/changes.json')))
    throw new Error('rapidroom/changes.json is retired; convert your entry to rapidroom/changes/<issue>-<slug>.json');
}
