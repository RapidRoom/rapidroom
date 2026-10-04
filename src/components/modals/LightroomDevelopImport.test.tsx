// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../../test/tauriMock';
import { Invokes } from '../ui/AppProperties';
import LightroomDevelopImport, { type DevelopPhoto, type DevelopPreview } from './LightroomDevelopImport';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let root: Root | undefined;
let container: HTMLDivElement;
afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  container?.remove();
});
const photo = (id: number, extra: Partial<DevelopPhoto> = {}): DevelopPhoto => ({
  id,
  path: `/photos/${id}.ARW`,
  copyName: '',
  virtualCopy: false,
  found: true,
  existingSidecar: false,
  existingEdits: false,
  rating: 3,
  adjustments: { exposure: 0.5 },
  unsupported: ['profileLook'],
  error: null,
  ...extra,
});
const preview: DevelopPreview = {
  catalogName: 'Catalog',
  fingerprint: 'snapshot-a',
  photos: [
    photo(1),
    photo(2, { existingSidecar: true, existingEdits: true }),
    photo(3, { found: false }),
    photo(4, { error: 'Unreadable saved edits' }),
  ],
};
const args = { path: '/Catalog.lrcat', mappings: {} };
function calls(cmd: string) {
  return invoke.mock.calls.filter(([name]) => name === cmd).map(([, options]) => options);
}
function button(key: string) {
  const item = [...container.querySelectorAll('button')].find(
    (node) => node.textContent === `modals.lightroomDevelop.${key}`,
  );
  if (!item) throw new Error(`Missing ${key}`);
  return item;
}
function option(key: string) {
  const item = [...container.querySelectorAll('label')]
    .find((node) => node.textContent === `modals.lightroomDevelop.${key}`)
    ?.querySelector('input');
  if (!item) throw new Error(`Missing ${key}`);
  return item;
}
async function mount(isCardMode = false) {
  mockCommand(Invokes.PreviewLightroomDevelop, () => preview);
  mockCommand(Invokes.ImportLightroomDevelop, () => ({ imported: 1, preserved: 0, virtualCopies: 0, errors: [] }));
  const onImported = vi.fn(async () => {}),
    onClose = vi.fn(),
    onBusyChange = vi.fn();
  const props = { catalogPath: args.path, mappings: args.mappings, isCardMode, onImported, onClose, onBusyChange };
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root?.render(<LightroomDevelopImport {...props} />));
  return props;
}

describe('Lightroom catalog photo edits', () => {
  it('previews all rows, selects only new readable photos, and imports only after an explicit click', async () => {
    const props = await mount();
    expect(calls(Invokes.ImportLightroomDevelop)).toEqual([]);
    expect(container.textContent).toContain('exposure');
    expect(container.textContent).toContain('modals.lightroomDevelop.unsupported');
    expect(container.textContent).toContain('Unreadable saved edits');
    expect(container.querySelectorAll('li input:checked')).toHaveLength(1);
    await act(async () => button('apply').click());
    expect(calls(Invokes.ImportLightroomDevelop)).toEqual([
      {
        ...args,
        fingerprint: 'snapshot-a',
        selections: [1],
        replaceEdits: false,
        importRatings: true,
        replaceRatings: false,
      },
    ]);
    expect(props.onImported).toHaveBeenCalledOnce();
    expect(props.onClose).toHaveBeenCalledOnce();
    expect(props.onBusyChange.mock.calls).toEqual([[true], [false]]);
  });
  it('keeps edit and rating replacement independent, and excludes missing or malformed rows from select-all', async () => {
    await mount();
    await act(async () => button('selectAvailable').click());
    await act(async () => option('replaceRatings').click());
    await act(async () => button('apply').click());
    expect(calls(Invokes.ImportLightroomDevelop)[0]).toMatchObject({
      selections: [1, 2],
      replaceEdits: false,
      importRatings: true,
      replaceRatings: true,
    });
  });
  it('disables replacement ratings when rating import is off', async () => {
    await mount();
    await act(async () => option('replaceRatings').click());
    await act(async () => option('importRatings').click());
    expect(option('replaceRatings').disabled).toBe(true);
    await act(async () => option('replaceEdits').click());
    await act(async () => button('apply').click());
    expect(calls(Invokes.ImportLightroomDevelop)[0]).toMatchObject({
      replaceEdits: true,
      importRatings: false,
      replaceRatings: false,
    });
  });
  it('refreshes after a partial write failure and leaves the report open', async () => {
    const props = await mount();
    mockCommand(Invokes.ImportLightroomDevelop, () => ({
      imported: 1,
      preserved: 0,
      virtualCopies: 1,
      errors: ['/photos/2.ARW: Permission denied'],
    }));
    mockCommand(Invokes.PreviewLightroomDevelop, () => ({
      ...preview,
      fingerprint: 'snapshot-b',
      photos: [photo(1, { existingSidecar: true }), photo(2)],
    }));
    await act(async () => button('apply').click());
    expect(props.onImported).toHaveBeenCalledOnce();
    expect(props.onClose).not.toHaveBeenCalled();
    expect(container.textContent).toContain('Permission denied');
    expect(calls(Invokes.PreviewLightroomDevelop)).toHaveLength(2);
    expect(container.querySelectorAll('li input:checked')).toHaveLength(1);
    await act(async () => button('apply').click());
    expect(calls(Invokes.ImportLightroomDevelop)[1]).toMatchObject({ fingerprint: 'snapshot-b', selections: [2] });
  });
  it('shows stale-preview errors and fetches a fresh fingerprint before retrying', async () => {
    const props = await mount();
    mockCommand(Invokes.ImportLightroomDevelop, () => {
      throw new Error('Preview changed; refresh.');
    });
    mockCommand(Invokes.PreviewLightroomDevelop, () => ({ ...preview, fingerprint: 'snapshot-c' }));
    await act(async () => button('apply').click());
    expect(container.textContent).toContain('Preview changed; refresh.');
    expect(props.onClose).not.toHaveBeenCalled();
    expect(props.onImported).not.toHaveBeenCalled();
    await act(async () => button('apply').click());
    expect(calls(Invokes.ImportLightroomDevelop)[1]).toMatchObject({ fingerprint: 'snapshot-c' });
  });
  it('prevents writes in Card mode', async () => {
    await mount(true);
    expect(button('apply').disabled).toBe(true);
    expect(option('replaceEdits').disabled).toBe(true);
    expect(container.textContent).toContain('modals.lightroomDevelop.cardMode');
    await act(async () => button('apply').click());
    expect(calls(Invokes.ImportLightroomDevelop)).toEqual([]);
  });
  it('uses relinked paths in the refreshed preview and apply request', async () => {
    const props = await mount();
    const mappings = { 'C:/Pictures': '/photos' };
    await act(async () => root?.render(<LightroomDevelopImport {...props} mappings={mappings} />));
    expect(calls(Invokes.PreviewLightroomDevelop)[1]).toEqual({ path: args.path, mappings });
    await act(async () => button('apply').click());
    expect(calls(Invokes.ImportLightroomDevelop)[0]).toMatchObject({ mappings });
  });
});
