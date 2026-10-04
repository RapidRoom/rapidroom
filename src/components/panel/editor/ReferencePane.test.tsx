// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../../../test/tauriMock';
import { Invokes } from '../../ui/AppProperties';
import ReferencePane from './ReferencePane';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let blobCount = 0;
const revoked: string[] = [];

beforeEach(() => {
  container = document.createElement('div');
  document.body.appendChild(container);
  root = createRoot(container);
  blobCount = 0;
  revoked.length = 0;
  URL.createObjectURL = vi.fn(() => `blob:ref-${++blobCount}`);
  URL.revokeObjectURL = vi.fn((url: string) => {
    revoked.push(url);
  });
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function renderPane(path: string | null, isChooserOpen = false) {
  await act(async () => {
    root.render(
      createElement(ReferencePane, {
        isChooserOpen,
        reference: path ? { label: path.split('/').pop()!, path } : null,
        onCancelChooser: () => undefined,
        onChoose: () => undefined,
        onClear: () => undefined,
        onExit: () => undefined,
      }),
    );
  });
}

const flush = () => act(async () => new Promise((resolve) => setTimeout(resolve, 0)));

describe('ReferencePane', () => {
  it('renders the reference with its saved adjustments through read-only commands only', async () => {
    const savedAdjustments = { exposure: 1.5 };
    mockCommand(Invokes.LoadMetadata, () => ({ adjustments: savedAdjustments }));
    const previewArgs: unknown[] = [];
    mockCommand(Invokes.GeneratePreviewForPath, (args) => {
      previewArgs.push(args);
      return new Uint8Array([1, 2, 3]);
    });

    await renderPane('/photos/ref.raw');
    await flush();

    expect(previewArgs).toEqual([{ path: '/photos/ref.raw', jsAdjustments: savedAdjustments }]);
    expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual([Invokes.LoadMetadata, Invokes.GeneratePreviewForPath]);
    expect(container.querySelector('img')?.getAttribute('src')).toBe('blob:ref-1');
    expect(container.querySelector('[data-testid="editor-reference-pane"]')?.getAttribute('data-reference-state')).toBe(
      'ready',
    );
  });

  it('drops a stale render when the reference changes before it finishes', async () => {
    const slow = deferred<Uint8Array>();
    mockCommand(Invokes.LoadMetadata, () => null);
    mockCommand(Invokes.GeneratePreviewForPath, (args) =>
      args?.path === '/photos/first.raw' ? slow.promise : new Uint8Array([2]),
    );

    await renderPane('/photos/first.raw');
    await flush();
    await renderPane('/photos/second.raw');
    await flush();
    expect(container.querySelector('img')?.getAttribute('src')).toBe('blob:ref-1');

    slow.resolve(new Uint8Array([1]));
    await flush();

    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    expect(container.querySelector('img')?.getAttribute('src')).toBe('blob:ref-1');
    expect(container.querySelector('img')?.getAttribute('alt')).toBe('second.raw');
  });

  it('revokes the preview when the reference is cleared', async () => {
    mockCommand(Invokes.LoadMetadata, () => null);
    mockCommand(Invokes.GeneratePreviewForPath, () => new Uint8Array([1]));

    await renderPane('/photos/ref.raw');
    await flush();
    await renderPane(null, true);

    expect(revoked).toEqual(['blob:ref-1']);
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('[data-testid="editor-reference-pane"]')?.getAttribute('data-reference-state')).toBe(
      'choose-reference',
    );
  });

  it('shows an unavailable state when the reference file is missing', async () => {
    mockCommand(Invokes.LoadMetadata, () => null);
    mockCommand(Invokes.GeneratePreviewForPath, () => {
      throw new Error('No such file or directory');
    });
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    await renderPane('/photos/deleted.raw');
    await flush();

    expect(container.querySelector('[data-testid="editor-reference-pane"]')?.getAttribute('data-reference-state')).toBe(
      'error',
    );
    expect(container.textContent).toContain('editor.referenceView.unavailable');
    consoleError.mockRestore();
  });
});
