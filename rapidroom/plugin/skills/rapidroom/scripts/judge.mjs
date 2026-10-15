// Decision policy adapted from skymanbp/autoshade@a1f3507808d1a9d61c44593244a5e6ee2c38cae3.
// Copyright (c) 2026 skymanbp. MIT notice: ../../../LICENSES/autoshade-MIT.txt.
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// Move identity/direction lives here; numeric control bounds come from the app schema.
export const moves = Object.freeze({
  lift_exposure: { key: 'exposure', delta: 0.15, purpose: 'Lift overall exposure' },
  lower_exposure: { key: 'exposure', delta: -0.15, purpose: 'Lower overall exposure' },
  recover_highlights: { key: 'highlights', delta: -10, purpose: 'Recover bright detail' },
  lift_shadows: { key: 'shadows', delta: 10, purpose: 'Open shadow detail' },
  add_colour: { key: 'vibrance', delta: 8, purpose: 'Strengthen colour separation' },
  calm_colour: { key: 'vibrance', delta: -8, purpose: 'Calm distracting colour' },
  warm: { key: 'temperature', delta: 4, purpose: 'Warm the balance' },
  cool: { key: 'temperature', delta: -4, purpose: 'Cool the balance' },
});
export const tiers = Object.freeze({ restrained: 1, balanced: 2, committed: 3 });
export function projectControls(schema) {
  return Object.fromEntries(
    [...new Set(Object.values(moves).map((move) => move.key))].sort().map((key) => {
      const parameter = schema.parameters[key];
      assert.equal(parameter?.defaultType, 'number', 'Move requires a numeric app control');
      assert.ok(parameter.uiRanges?.length, 'Move requires a resolved UI range');
      assert.ok(
        parameter.uiRanges.every(
          (range) =>
            typeof range.minimum === 'number' &&
            typeof range.maximum === 'number' &&
            Number.isFinite(range.minimum) &&
            Number.isFinite(range.maximum),
        ),
        'Unresolved UI range',
      );
      const minimum = Math.max(...parameter.uiRanges.map((range) => range.minimum));
      const maximum = Math.min(...parameter.uiRanges.map((range) => range.maximum));
      assert.ok(Number.isFinite(minimum) && Number.isFinite(maximum) && minimum < maximum);
      return [key, { minimum, maximum }];
    }),
  );
}
const controls = JSON.parse(readFileSync(new URL('./judge-controls.json', import.meta.url), 'utf8'));
function keys(value, required) {
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), 'Expected an object');
  assert.deepEqual(Object.keys(value).sort(), [...required].sort(), 'Unexpected or missing fields');
}
function score(value) {
  assert.ok(typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= 100, 'Invalid score');
}
function text(value) {
  assert.ok(typeof value === 'string' && value.trim().length > 0 && value.length <= 2000, 'Invalid review text');
}
function tier(strength) {
  assert.ok(Object.hasOwn(tiers, strength), 'Unknown strength');
  return tiers[strength];
}
export function catalogue(strength) {
  const cap = tier(strength);
  return {
    strength,
    roundCap: cap,
    rubric: `Score 0–100 against the user's request and destination, using actual rendered overview and detail. Strength: ${strength}. Preserve requested subject detail; penalise artifacts and unrequested changes. For committed, penalise a timid result when a strong look was requested. For restrained, penalise excess. Never penalise the requested style merely for being stylised. Scores express a judgement, not measured photographic quality.`,
    judgementSchema: {
      type: 'object',
      additionalProperties: false,
      required: ['score', 'decision', 'critique', 'hint'],
      properties: {
        score: { type: 'number', minimum: 0, maximum: 100 },
        decision: { type: 'string', enum: ['accept', 'revise', 'reject'] },
        critique: { type: 'string', minLength: 1, maxLength: 2000 },
        hint: {
          anyOf: [
            { type: 'null' },
            {
              type: 'object',
              additionalProperties: false,
              required: ['move', 'reason'],
              properties: {
                move: { type: 'string', enum: Object.keys(moves) },
                reason: { type: 'string', minLength: 1, maxLength: 2000 },
              },
            },
          ],
        },
      },
    },
    moveTable: Object.entries(moves).map(([id, move]) => ({ id, ...move, ...controls[move.key] })),
  };
}
function judgement(value) {
  keys(value, ['score', 'decision', 'critique', 'hint']);
  score(value.score);
  assert.ok(['accept', 'revise', 'reject'].includes(value.decision), 'Unknown decision');
  text(value.critique);
  if (value.hint !== null) {
    keys(value.hint, ['move', 'reason']);
    assert.ok(Object.hasOwn(moves, value.hint.move), 'Unknown move');
    text(value.hint.reason);
  }
}
function revision(value) {
  assert.ok(
    typeof value === 'string' && value.length > 0 && Buffer.byteLength(value, 'utf8') <= 128,
    'Revision must be a nonempty opaque string of at most 128 UTF-8 bytes',
  );
}
function candidate(value, reviewed) {
  keys(
    value,
    reviewed
      ? ['id', 'renderSha256', 'revision', 'adjustments', 'judgement']
      : ['id', 'renderSha256', 'revision', 'adjustments', 'score'],
  );
  assert.match(value.id, /^[a-zA-Z0-9_-]{1,80}$/);
  assert.match(value.renderSha256, /^[a-f0-9]{64}$/);
  revision(value.revision);
  assert.ok(value.adjustments && typeof value.adjustments === 'object' && !Array.isArray(value.adjustments));
  if (reviewed) judgement(value.judgement);
  else score(value.score);
}
// Pure policy: render hashes/revisions are caller attestations, not verified engine receipts.
// This returns a proposal only. It never connects to MCP or applies an adjustment.
export function decide(input) {
  keys(input, ['strength', 'round', 'liveRevision', 'accepted', 'candidate']);
  const cap = tier(input.strength);
  assert.ok(Number.isSafeInteger(input.round) && input.round >= 1 && input.round <= cap, 'Invalid round');
  revision(input.liveRevision);
  candidate(input.accepted, false);
  candidate(input.candidate, true);
  assert.notEqual(input.accepted.id, input.candidate.id, 'Candidate must have a distinct identity');
  const old = input.accepted,
    proposed = input.candidate,
    review = proposed.judgement;
  const retain = (reason) => ({ action: 'retain', reason, adopted: false, bestId: old.id, bestScore: old.score });
  if (input.liveRevision !== old.revision || proposed.revision !== old.revision)
    return { ...retain('revision_changed'), action: 'refresh' };
  if (review.decision === 'reject') return retain('rejected');
  if (review.score < old.score) return retain('score_decreased');
  const adopted = { action: 'retain', adopted: true, bestId: proposed.id, bestScore: review.score };
  if (review.decision === 'accept') return { ...adopted, reason: 'accepted' };
  if (input.round === cap) return { ...adopted, reason: 'round_cap' };
  if (!review.hint) return { ...adopted, reason: 'no_hint' };
  const move = moves[review.hint.move],
    range = controls[move.key],
    previous = proposed.adjustments[move.key];
  assert.ok(
    typeof previous === 'number' && Number.isFinite(previous) && previous >= range.minimum && previous <= range.maximum,
    'Move base is outside the app UI range',
  );
  const next = Math.max(range.minimum, Math.min(range.maximum, Number((previous + move.delta).toFixed(8))));
  if (next === previous) return { ...adopted, reason: 'move_unchanged' };
  return {
    ...adopted,
    action: 'render_proposal',
    reason: 'hint',
    move: review.hint.move,
    changes: { [move.key]: next },
    expectedRevision: old.revision,
    nextRound: input.round + 1,
  };
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const command = process.argv[2];
    assert.ok(process.argv.length <= 4);
    let result;
    if (command === 'catalogue') result = catalogue(process.argv[3]);
    else {
      assert.equal(command, 'decide');
      assert.equal(process.argv.length, 3);
      let bytes = 0,
        input = '';
      for await (const chunk of process.stdin) {
        bytes += chunk.length;
        assert.ok(bytes <= 1024 * 1024, 'Input exceeds limit');
        input += chunk.toString('utf8');
      }
      result = decide(JSON.parse(input));
    }
    console.log(JSON.stringify(result, null, 2));
  } catch {
    // Raw input/errors can contain private photo data. Never echo them.
    console.error('Invalid judge input. Retain the prior edit; inspect local input and the documented contract.');
    process.exitCode = 1;
  }
}
