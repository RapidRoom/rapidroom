import { mkdir, open, readFile, rm } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { Readable, Transform } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { manifest, verifyPhoto } from './contract.mjs';

const out = process.argv[2] && resolve(process.argv[2]);
if (!out) throw new Error('Usage: node fetch.mjs NEW_OUTPUT.ARW');
await mkdir(dirname(out), { recursive: true });
// Exclusive creation prevents a download or failure from replacing an existing file.
const handle = await open(out, 'wx');
const file = handle.createWriteStream();
try {
  const response = await fetch(manifest.photo.url, { signal: AbortSignal.timeout(120000) });
  if (!response.ok || !response.body) throw new Error('Fixture download failed');
  let bytes = 0;
  await pipeline(
    Readable.fromWeb(response.body),
    new Transform({
      transform(chunk, encoding, done) {
        bytes += chunk.length;
        done(bytes > manifest.photo.size_bytes ? new Error('Fixture exceeds expected size') : null, chunk);
      },
    }),
    file,
  );
  verifyPhoto(await readFile(out));
  console.log(`Verified ${manifest.photo.id} (${manifest.photo.license})`);
} catch (error) {
  file.destroy();
  await rm(out, { force: true });
  throw error;
}
