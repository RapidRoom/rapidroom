import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

describe('generated MCP adjustment schema', () => {
  it('matches source defaults and slider ranges and fails when a source slider drifts', () => {
    const root = process.cwd();
    const script = path.join(root, 'rapidroom/generate-adjustment-schema.mjs');
    execFileSync(process.execPath, [script, '--check']);
    const schema = JSON.parse(fs.readFileSync(path.join(root, 'rapidroom/adjustment-schema.json'), 'utf8'));
    expect(schema.parameters.exposure.default).toBe(0);
    expect(schema.parameters.exposure.uiRanges).toContainEqual(
      expect.objectContaining({ minimum: -5, maximum: 5, step: 0.01 }),
    );
    expect(schema.parameters['hsl.reds.hue'].uiRanges).toContainEqual(
      expect.objectContaining({ minimum: -100, maximum: 100 }),
    );
    expect(schema.controls.every((control) => control.adjustmentKeys.length > 0)).toBe(true);
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'rapidroom-schema-'));
    try {
      for (const name of ['rapidroom/adjustment-schema.json', ...schema.generatedFrom]) {
        const destination = path.join(fixture, name);
        fs.mkdirSync(path.dirname(destination), { recursive: true });
        fs.copyFileSync(path.join(root, name), destination);
      }
      execFileSync(process.execPath, [script, '--source-root', fixture, '--check']);
      const basic = path.join(fixture, 'src/components/adjustments/Basic.tsx');
      fs.writeFileSync(basic, fs.readFileSync(basic, 'utf8').replace('max={5}', 'max={6}'));
      expect(() =>
        execFileSync(process.execPath, [script, '--source-root', fixture, '--check'], { stdio: 'pipe' }),
      ).toThrow();
    } finally {
      fs.rmSync(fixture, { recursive: true, force: true });
    }
  });
});
