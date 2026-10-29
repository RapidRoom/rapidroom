import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, symlinkSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

it('allows existing debt, rejects per-file/rule increases and fails closed on broken configuration', () => {
  const root = process.cwd();
  const fixture = mkdtempSync(path.join(tmpdir(), 'lint-ratchet-fixture-'));
  const git = (...args) => execFileSync('git', args, { cwd: fixture, encoding: 'utf8', stdio: 'pipe' }).trim();
  const config =
    "module.exports = [{ignores:['node_modules/**']}, {files:['**/*.js'], rules:{'no-debugger':'error'}}];\n";
  try {
    git('init', '--quiet');
    git('config', 'user.name', 'yojen7');
    git('config', 'user.email', '86065973+yojen7@users.noreply.github.com');
    writeFileSync(path.join(fixture, '.gitignore'), 'node_modules\n');
    writeFileSync(path.join(fixture, 'eslint.config.cjs'), config);
    writeFileSync(path.join(fixture, 'debt.js'), 'debugger;\n');
    git('add', '.');
    git('commit', '--quiet', '-m', 'fixture baseline');
    const base = git('rev-parse', 'HEAD');
    symlinkSync(path.join(root, 'node_modules'), path.join(fixture, 'node_modules'), 'junction');
    const check = (reference = base) =>
      spawnSync(
        process.execPath,
        [fileURLToPath(new URL('../../rapidroom/check-lint-ratchet.mjs', import.meta.url)), '--base', reference],
        { cwd: fixture, encoding: 'utf8' },
      );
    expect(check().status).toBe(0);
    writeFileSync(path.join(fixture, 'debt.js'), 'debugger; debugger;\n');
    const increase = check();
    expect(increase.status).toBe(1);
    expect(increase.stderr).toContain('New ESLint errors');
    // Moving debt to a new file cannot hide behind an unchanged total.
    writeFileSync(path.join(fixture, 'debt.js'), '');
    writeFileSync(path.join(fixture, 'new.js'), 'debugger;\n');
    expect(check().status).toBe(1);
    writeFileSync(path.join(fixture, 'new.js'), '');
    expect(check().status).toBe(0);
    writeFileSync(path.join(fixture, 'new.js'), 'const = invalid syntax;\n');
    const syntax = check();
    expect(syntax.status).toBe(1);
    expect(syntax.stderr).toContain('Parsing error');
    writeFileSync(path.join(fixture, 'new.js'), '');
    expect(check('missing-base-ref').status).toBe(1);
    writeFileSync(path.join(fixture, 'eslint.config.cjs'), "throw new Error('broken config');\n");
    const broken = check();
    expect(broken.status).toBe(1);
    expect(broken.stderr).toContain('broken config');
    git('add', '.');
    git('commit', '--quiet', '-m', 'fixture broken baseline');
    const brokenBase = git('rev-parse', 'HEAD');
    writeFileSync(path.join(fixture, 'eslint.config.cjs'), config);
    const baselineFailure = check(brokenBase);
    expect(baselineFailure.status).toBe(1);
    expect(baselineFailure.stderr).toContain('broken config');
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
}, 30000);
