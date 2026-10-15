import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

export const manifest = JSON.parse(readFileSync(new URL('./fixtures.json', import.meta.url)));
export const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
export const assertions = [
  'global_edit_pixels',
  'undo_exact',
  'redo_exact',
  'local_effect',
  'protected_corner_exact',
  'restart_state_pixels',
  'export_png',
  'original_preserved',
];
export const expectedChecks = ['photo', 'procedural'].flatMap((kind) => assertions.map((name) => `${kind}_${name}`));

export function verifyPhoto(bytes) {
  assert.equal(bytes.length, manifest.photo.size_bytes, 'Fixture size mismatch');
  assert.equal(sha(bytes), manifest.photo.sha256, 'Fixture checksum mismatch');
}

// Build a new object. Native records, metadata, errors and paths never enter it.
export function publicSummary({ checks, binarySha, build, privateStatus, hostVersion, bridgeVersion, scriptSha }) {
  assert.match(binarySha, /^[0-9a-f]{64}$/);
  assert.match(scriptSha, /^[0-9a-f]{64}$/);
  const sourceMatches =
    build.source_commit === manifest.fork_commit && build.source_clean === true && build.binary_sha256 === binarySha;
  const checked = expectedChecks.map((id) => ({
    id,
    passed: checks.filter((c) => c.id === id).length === 1 && checks.find((c) => c.id === id).passed === true,
  }));
  const version = (value) =>
    typeof value === 'string' && /^\d+\.\d+\.\d+(?:[-+][a-zA-Z0-9.-]{1,32})?$/.test(value) ? value : null;
  return {
    format: 'rapidroom-public-mcp-repro-v1',
    passed:
      sourceMatches &&
      privateStatus === 'passed' &&
      checked.every((c) => c.passed) &&
      checks.length === expectedChecks.length,
    fork_source_commit: manifest.fork_commit,
    source_and_binary_match_build_receipt: sourceMatches,
    binary_sha256: binarySha,
    replay_script_sha256: scriptSha,
    host_version: version(hostVersion),
    bridge_version: version(bridgeVersion),
    fixtures: [manifest.photo, manifest.procedural],
    checks: checked,
    photographic_quality: 'not_scored',
    ai_mask_quality: 'not_run',
    full_assessment_coverage: 'not_claimed',
    rapidroom_renderer_parity: 'not_claimed',
  };
}
