import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { projectControls } from '../plugin/skills/rapidroom/scripts/judge.mjs';
const source = new URL('../adjustment-schema.json', import.meta.url);
const target = new URL('../plugin/skills/rapidroom/scripts/judge-controls.json', import.meta.url);
const content = JSON.stringify(projectControls(JSON.parse(readFileSync(source, 'utf8'))), null, 2) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), content, 'Stale judge controls');
else writeFileSync(target, content);
