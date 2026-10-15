import assert from 'node:assert/strict';
import { spawnSync, execFileSync } from 'node:child_process';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { isAbsolute, join, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { manifest, publicSummary, sha, verifyPhoto } from './contract.mjs';

const { values } = parseArgs({
  options: Object.fromEntries(
    ['fork', 'binary', 'photo', 'out', 'build-receipt'].map((name) => [name, { type: 'string' }]),
  ),
});
async function verifiedPhoto(path) {
  assert.equal((await stat(path)).size, manifest.photo.size_bytes, 'Fixture size mismatch');
  verifyPhoto(await readFile(path));
}
for (const name of ['fork', 'binary', 'photo', 'out', 'build-receipt'])
  assert.ok(values[name] && isAbsolute(values[name]), `Pass an absolute --${name}`);
assert.ok(resolve(values.out).split(sep).includes('samples'), 'Keep artifacts under a samples/ directory');

// Linux native processing owns only the GPU lock. Compilation is a separate step.
if (process.env.RAPIDROOM_PUBLIC_REPRO_LOCKED !== '1') {
  await verifiedPhoto(values.photo);
  const child = spawnSync(
    'flock',
    ['/tmp/rapidroom-gpu.lock', 'nice', '-n', '10', process.execPath, ...process.argv.slice(1)],
    {
      env: { ...process.env, RAPIDROOM_PUBLIC_REPRO_LOCKED: '1' },
      stdio: 'inherit',
    },
  );
  process.exit(child.status ?? 1);
}

const fork = resolve(values.fork),
  workspace = resolve(values.out);
const git = (...args) => execFileSync('git', args, { cwd: fork, encoding: 'utf8' }).trim();
assert.equal(git('rev-parse', 'HEAD'), manifest.fork_commit, 'Use the pinned public fork checkout');
assert.equal(git('status', '--porcelain'), '', 'Use a clean source checkout');
await verifiedPhoto(values.photo);
const build = JSON.parse(await readFile(values['build-receipt'], 'utf8'));
const binarySha = sha(await readFile(values.binary));
assert.equal(build.source_commit, manifest.fork_commit);
assert.equal(build.source_clean, true);
assert.equal(build.binary_sha256, binarySha, 'Pinned binary differs from build receipt');
await mkdir(workspace, { recursive: false });
await mkdir(join(workspace, 'fixtures'));
const script = fileURLToPath(import.meta.url),
  scriptSha = sha(await readFile(script));
const { createNativeHarness } = await import(pathToFileURL(join(fork, 'mcp/scripts/coverage-evidence.mjs')));
const { fixturePng, decodePng, pixelDifference } = await import(
  pathToFileURL(join(fork, 'mcp/scripts/png-fixtures.mjs'))
);
const procedural = join(workspace, 'fixtures/rgb-ramp-v1.png');
await writeFile(
  procedural,
  fixturePng(512, 384, (x, y) => [32 + x / 3, 48 + y / 3, 64 + (x + y) / 6]),
  { flag: 'wx' },
);
const checks = [];
let h, error;
async function check(id, fn) {
  try {
    await fn();
    checks.push({ id, passed: true });
  } catch (failure) {
    checks.push({ id, passed: false });
    throw failure;
  }
}
async function render(session, options, name) {
  const result = await h.call('render', { session_id: session, format: 'png', ...options });
  assert.equal(result.images.length, 1);
  const bytes = Buffer.from(result.images[0].data, 'base64');
  if (name) await writeFile(join(workspace, `${name}.png`), bytes, { flag: 'wx' });
  return decodePng(bytes);
}
try {
  h = await createNativeHarness({
    suite: 'public-gap-reproduction',
    workspace,
    binary: values.binary,
    minimumFreeGiB: 25,
  });
  for (const [kind, path, dimensions] of [
    ['photo', values.photo, manifest.photo.dimensions],
    ['procedural', procedural, manifest.procedural.dimensions],
  ]) {
    const before = await stat(path, { bigint: true }),
      originalSha = sha(await readFile(path));
    await h.fixture(
      path,
      kind === 'photo' ? 'CC0 genuine capture, reused regression' : 'Generated RGB ramp, not photographic',
    );
    const session = (await h.call('open_photo', { path, inherit_sidecar: false })).data.session_id;
    const state = async () => (await h.call('get_session', { session_id: session, include_adjustments: true })).data;
    const initial = await state();
    assert.deepEqual([initial.dimensions.width, initial.dimensions.height], dimensions);
    const baseline = await render(session, { long_edge: 800 }, `${kind}-original`);
    await h.call('set_adjustments', {
      session_id: session,
      expected_revision: initial.revision,
      patch: { exposure: 0.5 },
    });
    const edited = await render(session, { long_edge: 800 }, `${kind}-global`);
    await check(`${kind}_global_edit_pixels`, () => assert.ok(pixelDifference(baseline, edited).maximum > 0));
    await h.call('undo', { session_id: session });
    await check(`${kind}_undo_exact`, async () =>
      assert.equal(pixelDifference(baseline, await render(session, { long_edge: 800 })).maximum, 0),
    );
    await h.call('redo', { session_id: session });
    await check(`${kind}_redo_exact`, async () =>
      assert.equal(pixelDifference(edited, await render(session, { long_edge: 800 })).maximum, 0),
    );
    const [width, height] = dimensions;
    const center = { x: Math.floor(width / 2) - 32, y: Math.floor(height * 0.75) - 32, width: 64, height: 64 };
    const corner = { x: 0, y: 0, width: 64, height: 64 };
    const oldCenter = await render(session, { region: center });
    const oldCorner = await render(session, { region: corner });
    await h.call('mask_create', {
      session_id: session,
      expected_revision: (await state()).revision,
      name: 'Public local lift',
      type: 'radial',
      parameters: {
        centerX: width / 2,
        centerY: height * 0.75,
        radiusX: width / 5,
        radiusY: height / 6,
        rotation: 0,
        feather: 0.2,
      },
      adjustments: { exposure: 0.5 },
    });
    await check(`${kind}_local_effect`, async () =>
      assert.ok(
        pixelDifference(oldCenter, await render(session, { region: center }, `${kind}-local-native`)).maximum > 0,
      ),
    );
    await check(`${kind}_protected_corner_exact`, async () =>
      assert.equal(
        pixelDifference(oldCorner, await render(session, { region: corner }, `${kind}-protected-native`)).maximum,
        0,
      ),
    );
    const final = await state();
    const finalPixels = await render(session, { long_edge: 800 }, `${kind}-final`);
    await h.call('save_session', { session_id: session });
    await h.call('close_session', { session_id: session });
    await h.reconnect();
    await check(`${kind}_restart_state_pixels`, async () => {
      const restored = await state();
      assert.deepEqual(restored.adjustments, final.adjustments);
      assert.equal(restored.revision, final.revision);
      assert.equal(
        pixelDifference(finalPixels, await render(session, { long_edge: 800 }, `${kind}-restarted`)).maximum,
        0,
      );
    });
    const delivery = join(workspace, 'exports', `${kind}-delivery.png`);
    await h.call('export', {
      session_id: session,
      path: delivery,
      format: 'png',
      bit_depth: 8,
      long_edge: 800,
      keep_metadata: false,
      strip_gps: true,
    });
    await check(`${kind}_export_png`, async () => {
      const decoded = decodePng(await readFile(delivery));
      assert.equal(Math.max(decoded.width, decoded.height), Math.min(800, Math.max(...dimensions)));
      assert.ok(decoded.pixels.some((v) => v > 0));
    });
    await check(`${kind}_original_preserved`, async () => {
      assert.equal(sha(await readFile(path)), originalSha);
      assert.equal((await stat(path, { bigint: true })).mtimeNs, before.mtimeNs);
    });
  }
} catch (failure) {
  error = failure;
} finally {
  if (h) {
    try {
      await h.close(error);
    } catch (failure) {
      error ??= failure;
    }
  }
}
let privateStatus = 'failed';
try {
  privateStatus = JSON.parse(await readFile(join(workspace, 'summary.json'), 'utf8')).status;
} catch {
  /* Startup failure remains failed. */
}
if (
  git('status', '--porcelain') ||
  git('rev-parse', 'HEAD') !== manifest.fork_commit ||
  sha(await readFile(script)) !== scriptSha ||
  error
)
  privateStatus = 'failed';
const hostVersion = JSON.parse(await readFile(join(fork, 'mcp/package.json'), 'utf8')).version;
const summary = publicSummary({
  checks,
  binarySha,
  build,
  privateStatus,
  hostVersion,
  bridgeVersion: h?.capabilities.bridge_version,
  scriptSha,
});
await writeFile(join(workspace, 'public-summary.json'), JSON.stringify(summary, null, 2) + '\n', { flag: 'wx' });
console.log(
  summary.passed
    ? 'Public reproduction passed; inspect matched overview/native images before quality claims.'
    : 'Public reproduction FAILED. Keep local diagnostics; publish only public-summary.json.',
);
process.exitCode = summary.passed ? 0 : 1;
