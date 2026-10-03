#!/usr/bin/env node
// Renders what RapidRoom adds on top of upstream RapidRAW, from rapidroom/changes.json:
// a summary table in .github/README.md (between the rapidroom-changes markers) and the full CHANGES.md.
//   node rapidroom/status.mjs            render from the cached upstream status
//   node rapidroom/status.mjs --refresh  fetch issue/PR states from GitHub first (uses GITHUB_TOKEN/GH_TOKEN if set)
//   node rapidroom/status.mjs --check    exit 1 if the rendered files are out of date
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const CHANGES = join(here, 'changes.json');
const CACHE = join(here, 'upstream-status.json');
const README = join(root, '.github', 'README.md');
const CHANGES_MD = join(root, 'CHANGES.md');
const START = '<!-- rapidroom-changes:start -->';
const END = '<!-- rapidroom-changes:end -->';

const args = new Set(process.argv.slice(2));
const { changes } = JSON.parse(readFileSync(CHANGES, 'utf8'));
let cache = existsSync(CACHE) ? JSON.parse(readFileSync(CACHE, 'utf8')) : { as_of: null, refs: {} };

const refsOf = (c) => [...(c.upstream_issues || []), ...(c.upstream_prs || [])];
const parseRef = (ref) => {
  const m = ref.match(/^([\w.-]+)\/([\w.-]+)#(\d+)$/);
  if (!m) throw new Error(`bad upstream ref "${ref}" (want owner/repo#n)`);
  return { owner: m[1], repo: m[2], number: Number(m[3]) };
};

if (args.has('--refresh')) {
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'rapidroom-status' };
  if (token) headers.Authorization = `Bearer ${token}`;
  const refs = {};
  for (const ref of new Set(changes.flatMap(refsOf))) {
    const { owner, repo, number } = parseRef(ref);
    const res = await fetch(`https://api.github.com/repos/${owner}/${repo}/issues/${number}`, { headers });
    if (!res.ok) throw new Error(`${ref}: GitHub API ${res.status}`);
    const d = await res.json();
    refs[ref] = {
      kind: d.pull_request ? 'pr' : 'issue',
      title: d.title,
      url: d.html_url,
      state: d.pull_request?.merged_at ? 'merged' : d.state,
      created: d.created_at.slice(0, 10),
      closed: (d.pull_request?.merged_at || d.closed_at || '').slice(0, 10) || null,
    };
  }
  cache = { as_of: new Date().toISOString().slice(0, 10), refs };
  writeFileSync(CACHE, JSON.stringify(cache, null, 2) + '\n');
}

const today = cache.as_of || new Date().toISOString().slice(0, 10);
const days = (a, b) => Math.round((Date.parse(b) - Date.parse(a)) / 86400000);
const short = (ref) => ref.replace(/^CyberTimon\/RapidRAW#/, '#').replace(/^CyberTimon\//, '');
const link = (ref) => `[${short(ref)}](${cache.refs[ref]?.url || urlOf(ref)})`;
const urlOf = (ref) => {
  const { owner, repo, number } = parseRef(ref);
  return `https://github.com/${owner}/${repo}/issues/${number}`;
};
const who = (c) => c.authors.map((a) => `[@${a}](https://github.com/${a})`).join(', ');

// "#850 open 412 d": how long the upstream issue has been open (or took to close).
const issueCell = (ref) => {
  const s = cache.refs[ref];
  if (!s) return link(ref);
  const age = s.state === 'open' ? `open ${days(s.created, today)} d` : `closed after ${days(s.created, s.closed)} d`;
  return `${link(ref)} ${age}`;
};
const prCell = (ref) => {
  const s = cache.refs[ref];
  return s ? `${link(ref)} ${s.state}` : link(ref);
};
const upstreamCell = (c) => {
  if (c.rapidroom_only) return 'RapidRoom only';
  const parts = [...(c.upstream_issues || []).map(issueCell), ...(c.upstream_prs || []).map((r) => `PR ${prCell(r)}`)];
  return parts.length ? parts.join('; ') : 'not yet offered';
};

const shown = changes.filter((c) => c.category !== 'ci');
const fixedIssues = shown
  .flatMap((c) => (c.upstream_issues || []).map((r) => ({ c, s: cache.refs[r] })))
  .filter((x) => x.s);
const stillOpen = fixedIssues.filter((x) => x.s.state === 'open');
const prs = shown
  .flatMap((c) => c.upstream_prs || [])
  .map((r) => cache.refs[r])
  .filter(Boolean);
const merged = prs.filter((s) => s.state === 'merged').length;
const median = (xs) => {
  if (!xs.length) return null;
  const v = [...xs].sort((a, b) => a - b);
  return v.length % 2 ? v[(v.length - 1) / 2] : Math.round((v[v.length / 2 - 1] + v[v.length / 2]) / 2);
};
const waited = median(fixedIssues.map((x) => days(x.s.created, x.c.landed)));

const summary = [
  `**${shown.length} changes on top of RapidRAW.**`,
  fixedIssues.length
    ? `${fixedIssues.length} fix${fixedIssues.length === 1 ? 'es an upstream issue' : ' upstream issues'}` +
      (waited !== null ? ` that had been open a median of ${waited} days when RapidRoom shipped the fix` : '') +
      (stillOpen.length ? `; ${stillOpen.length} of them still open upstream.` : '.')
    : '',
  prs.length ? `${prs.length} offered upstream as PRs, ${merged} merged so far.` : '',
]
  .filter(Boolean)
  .join(' ');

const cell = (s) => s.replace(/\|/g, '\\|');
const row = (c) =>
  `| ${cell(c.title)}${c.rendering_change ? ' ⚑' : ''} | ${c.category} | ${who(c)} | ${upstreamCell(c)} |`;
const table = (list) => ['| Change | Type | By | Upstream |', '|---|---|---|---|', ...list.map(row)].join('\n');
const asOf = cache.as_of ? `Upstream status as of ${cache.as_of}.` : 'Upstream status not fetched yet.';
const legend = `⚑ changes rendered output on purpose. ${asOf} Full list with sources: [CHANGES.md](../CHANGES.md).`;

const readmeBlock = `${START}\n${summary}\n\n${table(shown)}\n\n${legend}\n${END}`;

const detail = (c) =>
  [
    `### ${c.title}`,
    '',
    `- **Type:** ${c.category}${c.rendering_change ? ' (changes rendered output)' : ''}`,
    `- **Landed in RapidRoom:** ${c.landed}`,
    `- **By:** ${who(c)}${c.source && c.source !== 'rapidroom' ? `, from ${c.source}` : ''}`,
    `- **Upstream:** ${upstreamCell(c)}`,
    ...(c.commits?.length
      ? [`- **Commits:** ${c.commits.map((u) => `[${u.split('/').pop().slice(0, 7)}](${u})`).join(', ')}`]
      : []),
    ...(c.notes ? [`- **Notes:** ${c.notes}`] : []),
  ].join('\n');

const changesMd = `# What RapidRoom adds to RapidRAW

Generated from [rapidroom/changes.json](rapidroom/changes.json) by \`node rapidroom/status.mjs\`; don't edit by hand.

${summary}

${table(changes).replace(/\(\.\.\/CHANGES\.md\)/g, '(CHANGES.md)')}

⚑ changes rendered output on purpose. ${asOf}

## Details

${changes.map(detail).join('\n\n')}
`;

// Format like the rest of the repo, so `npm run format:check` and --check agree.
const prettier = await import('prettier');
const fmt = async (text, filepath) => prettier.format(text, { ...(await prettier.resolveConfig(filepath)), filepath });

const readme = readFileSync(README, 'utf8');
if (!readme.includes(START) || !readme.includes(END)) throw new Error(`${README} is missing the ${START} markers`);
const newReadme = await fmt(readme.replace(new RegExp(`${START}[\\s\\S]*?${END}`), readmeBlock), README);
const changesOut = await fmt(changesMd, CHANGES_MD);

if (args.has('--check')) {
  const stale = [
    [README, newReadme],
    [CHANGES_MD, changesOut],
  ].filter(([p, s]) => !existsSync(p) || readFileSync(p, 'utf8') !== s);
  if (stale.length) {
    console.error(`Out of date: ${stale.map(([p]) => p).join(', ')}. Run node rapidroom/status.mjs`);
    process.exit(1);
  }
} else {
  writeFileSync(README, newReadme);
  writeFileSync(CHANGES_MD, changesOut);
  console.log(summary);
}
