import { describe, expect, it, vi } from 'vitest';
import { CullingPreviewQueue, cullingPreviewWindow, type PreparedCullingPreview } from './cullingPreviewQueue';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function settle() {
  for (let i = 0; i < 20; i++) await Promise.resolve();
}
function request(path: string) {
  return { path, thumbKey: 'thumb-' + path };
}
function harness() {
  const saved = new Map<string, PreparedCullingPreview>();
  const services = {
    beginBatch: vi.fn(async () => 1),
    cached: vi.fn(({ path }: { path: string }) => saved.has(path)),
    loadAdjustments: vi.fn(async (_path: string): Promise<unknown> => ({ exposure: 1 })),
    render: vi.fn(
      async (_path: string, _adjustments: unknown, _generation: number): Promise<Uint8Array> => new Uint8Array([1]),
    ),
    prepare: vi.fn(async (_bytes: Uint8Array) => ({ url: 'blob:prepared', width: 7008, height: 4672 })),
    commit: vi.fn(({ path }: { path: string }, preview: PreparedCullingPreview) => {
      saved.set(path, preview);
      return true;
    }),
    release: vi.fn(),
    failed: vi.fn(),
  };
  return { saved, services, queue: new CullingPreviewQueue(services) };
}

describe('culling preview navigation work', () => {
  it('drains a superseded decode without rendering obsolete neighbours or retrying cancellation', async () => {
    const { services, saved, queue } = harness();
    const old = deferred<Uint8Array>();
    services.render.mockImplementationOnce(() => old.promise);
    queue.update([request('A'), request('B'), request('C')]);
    await settle();
    expect(services.render.mock.calls.map((x) => x[0])).toEqual(['A']);
    queue.update([request('Z'), request('Y')]);
    await settle();
    expect(services.beginBatch).toHaveBeenCalledTimes(2);
    expect(services.render).toHaveBeenCalledTimes(1);
    old.reject(new Error('Culling preview cancelled'));
    await settle();
    expect(services.render.mock.calls.map((x) => x[0])).toEqual(['A', 'Z', 'Y']);
    expect([...saved.keys()]).toEqual(['Z', 'Y']);
    expect(services.failed).not.toHaveBeenCalled();
  });

  it('adopts an in-flight neighbour on forward navigation instead of cancelling and decoding it again', async () => {
    const { services, saved, queue } = harness();
    const pending = deferred<Uint8Array>();
    services.render.mockImplementationOnce(() => pending.promise);
    queue.update([request('B'), request('C')]);
    await settle();
    queue.update([request('B'), request('D')]);
    await settle();
    expect(services.beginBatch).toHaveBeenCalledTimes(1);
    pending.resolve(new Uint8Array([1]));
    await settle();
    expect(services.render.mock.calls.map((x) => x[0])).toEqual(['B', 'D']);
    expect([...saved.keys()]).toEqual(['B', 'D']);
  });

  it('revokes a decoded blob that becomes obsolete while the browser prepares it', async () => {
    const { services, saved, queue } = harness();
    const decoded = deferred<PreparedCullingPreview>();
    services.prepare.mockImplementationOnce(() => decoded.promise);
    queue.update([request('A')]);
    await settle();
    queue.update([request('B')]);
    decoded.resolve({ url: 'blob:old', width: 7008, height: 4672 });
    await settle();
    expect(services.release).toHaveBeenCalledWith('blob:old');
    expect([...saved.keys()]).toEqual(['B']);
  });

  it('falls back once for saved-edit render failure and continues after a permanent failure', async () => {
    const { services, saved, queue } = harness();
    services.render.mockRejectedValueOnce(new Error('bad edits')).mockRejectedValueOnce(new Error('bad file'));
    queue.update([request('A'), request('B')]);
    await settle();
    expect(services.render.mock.calls.map((x) => [x[0], x[1]])).toEqual([
      ['A', { exposure: 1 }],
      ['A', {}],
      ['B', { exposure: 1 }],
    ]);
    expect([...saved.keys()]).toEqual(['B']);
    expect(services.failed).toHaveBeenCalledTimes(1);
  });

  it('stops a view without publishing its late result and bounds the selected plus adjacent work', async () => {
    const { services, saved, queue } = harness();
    const pending = deferred<Uint8Array>();
    const paths = Array.from({ length: 30 }, (_, i) => String(i));
    const selected = ['0', '1', '2', '3', '4', '10'];
    const plan = cullingPreviewWindow(paths, selected, '10');
    expect(plan).toEqual([...selected, '11', '12', '9']);
    services.render.mockImplementationOnce(() => pending.promise);
    queue.update(plan.map(request));
    await settle();
    queue.stop();
    pending.resolve(new Uint8Array([1]));
    await settle();
    expect(saved.size).toBe(0);
    expect(services.render).toHaveBeenCalledTimes(1);
  });
});
