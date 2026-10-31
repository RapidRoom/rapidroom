#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { mkdtemp, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { ESLint } from 'eslint';

const git = (cwd, args) =>
  execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();

function errorCounts(results, root) {
  const counts = new Map();
  for (const result of results) {
    const file = path.relative(root, result.filePath).split(path.sep).join('/');
    for (const message of result.messages) {
      if (message.fatal) throw new Error(`${file}: ${message.message}`);
      if (message.severity !== 2) continue;
      const key = `${file}\t${message.ruleId ?? '(no rule)'}`;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  return counts;
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length !== 2 || args[0] !== '--base')
    throw new Error('Usage: node rapidroom/check-lint-ratchet.mjs --base <commit>');
  const root = git(process.cwd(), ['rev-parse', '--show-toplevel']);
  const base = git(root, ['rev-parse', '--verify', `${args[1]}^{commit}`]);
  const scratch = await mkdtemp(path.join(tmpdir(), 'rapidroom-lint-'));
  const baseline = path.join(scratch, 'base');
  let attached = false;
  try {
    git(root, ['worktree', 'add', '--detach', '--quiet', baseline, base]);
    attached = true;
    // Compare both trees with the installed candidate toolchain, without a second install.
    await symlink(path.join(root, 'node_modules'), path.join(baseline, 'node_modules'), 'junction');
    const engine = (cwd) =>
      new ESLint({
        cwd,
        overrideConfig: { languageOptions: { parserOptions: { tsconfigRootDir: cwd } } },
      });
    const candidateEngine = engine(root);
    const baselineEngine = engine(baseline);
    const runs = await Promise.allSettled([candidateEngine.lintFiles(['.']), baselineEngine.lintFiles(['.'])]);
    for (const run of runs) if (run.status === 'rejected') throw run.reason;
    const [candidateResults, baselineResults] = runs.map((run) => run.value);
    const candidate = errorCounts(candidateResults, root);
    const previous = errorCounts(baselineResults, baseline);
    const formatter = await candidateEngine.loadFormatter('stylish');
    const report = formatter.format(candidateResults);
    if (report) console.log(report);
    const total = (counts) => [...counts.values()].reduce((sum, count) => sum + count, 0);
    console.log(`ESLint errors: base ${total(previous)}, candidate ${total(candidate)} (per file/rule ratchet)`);
    const added = [...candidate].filter(([key, count]) => count > (previous.get(key) ?? 0));
    for (const [key, count] of added) console.error(`${key}: ${previous.get(key) ?? 0} → ${count}`);
    if (added.length) throw new Error(`New ESLint errors in ${added.length} file/rule group(s)`);
  } finally {
    try {
      if (attached) git(root, ['worktree', 'remove', '--force', baseline]);
    } finally {
      await rm(scratch, { recursive: true, force: true });
    }
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
