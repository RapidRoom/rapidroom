import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { catalogue, decide, moves, projectControls } from '../plugin/skills/rapidroom/scripts/judge.mjs';
const script = new URL('../plugin/skills/rapidroom/scripts/judge.mjs', import.meta.url);
const editRevision = '7'.repeat(64);
const schema = JSON.parse(readFileSync(new URL('../adjustment-schema.json', import.meta.url)));
function input() {
  const accepted = {
    id: 'current',
    renderSha256: 'a'.repeat(64),
    revision: editRevision,
    score: 70,
    adjustments: { exposure: 0 },
  };
  return {
    strength: 'balanced',
    round: 1,
    liveRevision: editRevision,
    accepted,
    candidate: {
      id: 'proposal',
      renderSha256: 'b'.repeat(64),
      revision: editRevision,
      adjustments: { exposure: 0.3 },
      judgement: {
        score: 75,
        decision: 'revise',
        critique: 'Subject remains too dark',
        hint: { move: 'lift_exposure', reason: 'Lift the subject' },
      },
    },
  };
}
test('equal and higher scores retain the new best; lower or rejected scores retain the old candidate', () => {
  for (const score of [70, 75, 100]) {
    const data = input();
    data.candidate.judgement.score = score;
    const result = decide(data);
    assert.equal(result.adopted, true);
    assert.equal(result.bestId, 'proposal');
    assert.equal(result.bestScore, score);
  }
  for (const change of [{ score: 69.9 }, { decision: 'reject', score: 100 }]) {
    const data = input();
    Object.assign(data.candidate.judgement, change);
    const result = decide(data);
    assert.equal(result.adopted, false);
    assert.equal(result.bestId, 'current');
    assert.equal(result.changes, undefined);
  }
});
test('caps are hard total judged-proposal limits at 1, 2 and 3; next round always advances', () => {
  for (const [strength, cap] of [
    ['restrained', 1],
    ['balanced', 2],
    ['committed', 3],
  ]) {
    for (let round = 1; round <= cap; round++) {
      const data = input();
      Object.assign(data, { strength, round });
      const result = decide(data);
      assert.equal(result.action, round === cap ? 'retain' : 'render_proposal');
      if (round < cap) assert.equal(result.nextRound, round + 1);
    }
    const data = input();
    Object.assign(data, { strength, round: cap + 1 });
    assert.throws(() => decide(data));
  }
});
test('an accept or absent hint never spends another round; blank/unknown/raw-slider hints fail', () => {
  for (const change of [{ decision: 'accept' }, { hint: null }]) {
    const data = input();
    Object.assign(data.candidate.judgement, change);
    assert.equal(decide(data).action, 'retain');
  }
  for (const hint of [
    { move: 'lift_exposure', reason: ' ' },
    { move: 'run_shell', reason: 'bad' },
    { move: 'lift_exposure', reason: 'bad', exposure: 4 },
  ]) {
    const data = input();
    data.candidate.judgement.hint = hint;
    assert.throws(() => decide(data));
  }
  const data = input();
  data.candidate.judgement.adjustments = { exposure: 4 };
  assert.throws(() => decide(data));
});
test('concurrent user edits or a candidate rendered against another revision require a refresh', () => {
  for (const edit of [
    (data) => (data.liveRevision = 'c'.repeat(64)),
    (data) => (data.candidate.revision = 'd'.repeat(64)),
  ]) {
    const data = input();
    edit(data);
    const result = decide(data);
    assert.equal(result.action, 'refresh');
    assert.equal(result.adopted, false);
    assert.equal(result.changes, undefined);
  }
});
test('moves are fixed bounded deltas from the selected snapshot; no-op stops and input remains untouched', () => {
  const data = input(),
    before = structuredClone(data);
  assert.deepEqual(decide(data).changes, { exposure: 0.45 });
  assert.deepEqual(data, before);
  const { maximum } = projectControls(schema).exposure;
  data.candidate.adjustments.exposure = maximum - 0.01;
  assert.equal(decide(data).changes.exposure, maximum);
  data.candidate.adjustments.exposure = maximum;
  assert.equal(decide(data).reason, 'move_unchanged');
  for (const value of [maximum + 1, null, '1', NaN, Infinity]) {
    data.candidate.adjustments.exposure = value;
    assert.throws(() => decide(data));
  }
});
test('schema, prompt catalogue and numeric table cover every registry move and generated app range', () => {
  const controls = projectControls(schema);
  const bundled = JSON.parse(
    readFileSync(new URL('../plugin/skills/rapidroom/scripts/judge-controls.json', import.meta.url)),
  );
  assert.deepEqual(bundled, controls);
  for (const strength of ['restrained', 'balanced', 'committed']) {
    const output = catalogue(strength);
    assert.deepEqual(output.judgementSchema.properties.hint.anyOf[1].properties.move.enum, Object.keys(moves));
    assert.deepEqual(
      output.moveTable.map((row) => row.id),
      Object.keys(moves),
    );
    for (const row of output.moveTable)
      assert.deepEqual({ minimum: row.minimum, maximum: row.maximum }, controls[row.key]);
  }
  const bad = structuredClone(schema);
  bad.parameters.exposure.uiRanges[0].minimum = null;
  assert.throws(() => projectControls(bad));
});
test('nonfinite/out-of-range scores, missing render identity and malformed state fail closed', () => {
  for (const value of [NaN, Infinity, -1, 101, '80', null]) {
    const data = input();
    data.candidate.judgement.score = value;
    assert.throws(() => decide(data));
  }
  for (const edit of [
    (data) => delete data.candidate.renderSha256,
    (data) => (data.accepted.score = NaN),
    (data) => (data.candidate.revision = -1),
    (data) => (data.strength = 'anything'),
    (data) => (data.round = 0),
    (data) => (data.candidate.id = data.accepted.id),
  ]) {
    const data = input();
    edit(data);
    assert.throws(() => decide(data));
  }
});
test('CLI works outside the checkout, outputs proposals only, and never echoes bad private input', () => {
  const output = JSON.parse(
    execFileSync(process.execPath, [script.pathname, 'decide'], {
      input: JSON.stringify(input()),
      cwd: '/tmp',
      encoding: 'utf8',
    }),
  );
  assert.equal(output.action, 'render_proposal');
  assert.equal(output.expectedRevision, editRevision);
  for (const input of ['PRIVATE_PHOTO_PATH secret', JSON.stringify({ private: 'PRIVATE_PHOTO_PATH secret' })]) {
    const result = spawnSync(process.execPath, [script.pathname, 'decide'], { input, encoding: 'utf8', cwd: '/tmp' });
    assert.equal(result.status, 1);
    assert.equal(result.stdout, '');
    assert.ok(!result.stderr.includes('PRIVATE_PHOTO_PATH'));
  }
});

test('opaque revisions preserve real app strings and match the actual MCP expectedRevision schema', () => {
  const tools = readFileSync(new URL('../../src-tauri/src/mcp/tools.rs', import.meta.url), 'utf8');
  const update = tools.split('"name": "update_adjustments"')[1].split('json!(')[0];
  const expectedRevisionSchema = JSON.parse(/"expectedRevision":\s*(\{[^}]+\})/.exec(update)[1]);
  for (const revision of [editRevision, '7', ' token ', 'a'.repeat(128), '🌈'.repeat(32)]) {
    const data = input();
    data.liveRevision = data.accepted.revision = data.candidate.revision = revision;
    const result = decide(data);
    assert.equal(result.expectedRevision, revision);
    assert.equal(typeof result.expectedRevision, expectedRevisionSchema.type);
  }
  for (const revision of [7, -1, null, undefined, '', 'a'.repeat(129), '🌈'.repeat(33)]) {
    for (const field of ['live', 'accepted', 'candidate']) {
      const data = input();
      if (field === 'live') data.liveRevision = revision;
      else data[field].revision = revision;
      assert.throws(() => decide(data));
    }
  }
});
