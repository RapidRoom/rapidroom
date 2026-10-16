// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { SearchInput } from './LibraryHeader';
import { useLibraryStore } from '../../../store/useLibraryStore';
import { computeSortedLibrary } from '../../../hooks/useSortedLibrary';
import { type ImageFile } from '../../ui/AppProperties';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string, options?: { filter?: string }) => `${key}${options?.filter ? ` ${options.filter}` : ''}` }),
}));

let root: Root | undefined;
let container: HTMLDivElement;
const originalState = useLibraryStore.getState();

afterEach(async () => {
  await act(async () => root?.unmount());
  container?.remove();
  root = undefined;
  useLibraryStore.setState(originalState, true);
});

async function mount(tags: string[], text = '') {
  useLibraryStore.setState({ searchCriteria: { tags, text, mode: 'AND' } });
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root!.render(createElement(SearchInput, { isIndexing: false, indexingProgress: {} })));
}
const input = () => container.querySelector('input')!;
const edits = () => [...container.querySelectorAll<HTMLButtonElement>('[data-filter-chip-edit]')];
const removes = () => [...container.querySelectorAll<HTMLButtonElement>('button[aria-label^="library.header.search.removeFilter"]')];
const criteria = () => useLibraryStore.getState().searchCriteria;
const click = async (node: HTMLElement) => act(async () => node.click());
const key = async (node: HTMLElement, value: string) => act(async () => {
  node.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: value }));
});
const type = async (value: string) => act(async () => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input(), value);
  input().dispatchEvent(new Event('input', { bubbles: true }));
});

it('edits numeric and text queries in their original positions and updates the real filter results', async () => {
  await mount(['iso:>3200', 'lens:35mm']);
  const image = (path: string, iso: string, lens: string): ImageFile => ({
    path, exif: { PhotographicSensitivity: iso, LensModel: lens }, flag: null, group_id: null,
    is_cloud_placeholder: false, is_edited: false, is_raw: false, is_virtual_copy: false,
    modified: 0, rating: 0, tags: [],
  });
  useLibraryStore.setState({ imageList: [image('/a.jpg', '6400', '35mm'), image('/b.jpg', '12800', '50mm')] });
  const visible = () => computeSortedLibrary(useLibraryStore.getState(), { appSettings: { grouping: 'off' } }).map(i => i.path);
  expect(visible()).toEqual(['/a.jpg']);
  await click(edits()[0]);
  expect(input().value).toBe('iso:>3200');
  expect(document.activeElement).toBe(input());
  await type('iso:>10000');
  await key(input(), 'Enter');
  expect(criteria().tags).toEqual(['iso:>10000', 'lens:35mm']);
  expect(visible()).toEqual([]);
  await click(edits()[1]);
  await type('lens:50mm');
  await key(input(), 'Enter');
  expect(criteria().tags).toEqual(['iso:>10000', 'lens:50mm']);
  expect(visible()).toEqual(['/b.jpg']);
});

it('the separate x deletes only its indexed duplicate without starting an edit', async () => {
  await mount(['iso:>3200', 'lens:35mm', 'iso:>3200']);
  await click(removes()[2]);
  expect(criteria()).toEqual({ tags: ['iso:>3200', 'lens:35mm'], text: '', mode: 'AND' });
  await key(input(), 'Escape');
  expect(criteria().tags).toEqual(['iso:>3200', 'lens:35mm']);
});

it('Escape restores the edited duplicate by index and preserves an earlier input draft', async () => {
  await mount(['iso:>3200', 'lens:35mm', 'iso:>3200'], 'unfinished');
  await click(edits()[2]);
  await type('iso:>9000');
  await key(input(), 'Escape');
  expect(criteria()).toEqual({ tags: ['iso:>3200', 'lens:35mm', 'iso:>3200'], text: 'unfinished', mode: 'AND' });
});

it('switching chip edits restores the previous chip and keeps its input draft', async () => {
  await mount(['iso:>3200', 'lens:35mm', 'camera:Sony'], 'draft');
  await click(edits()[0]);
  await type('iso:>9999');
  await click(edits()[1]);
  expect(criteria()).toEqual({ tags: ['iso:>3200', 'lens:35mm'], text: 'camera:Sony', mode: 'AND' });
  await key(input(), 'Escape');
  expect(criteria()).toEqual({ tags: ['iso:>3200', 'lens:35mm', 'camera:Sony'], text: 'draft', mode: 'AND' });
});

it('deleting an earlier chip while editing preserves the correct restoration position', async () => {
  await mount(['iso:>3200', 'lens:35mm', 'camera:Sony']);
  await click(edits()[1]);
  await click(removes()[0]);
  await key(input(), 'Escape');
  expect(criteria().tags).toEqual(['lens:35mm', 'camera:Sony']);
});

it('Backspace uses the cancellable edit path and clearing does not resurrect a pending chip', async () => {
  await mount(['iso:>3200', 'lens:35mm']);
  await key(input(), 'Backspace');
  expect(criteria()).toEqual({ tags: ['iso:>3200'], text: 'lens:35mm', mode: 'AND' });
  await key(input(), 'Escape');
  expect(criteria().tags).toEqual(['iso:>3200', 'lens:35mm']);
  await click(edits()[0]);
  await click(container.querySelector<HTMLButtonElement>('button[data-tooltip="library.header.search.tooltipClearSearch"]')!);
  await key(input(), 'Escape');
  expect(criteria()).toEqual({ tags: [], text: '', mode: 'OR' });
});

it('Left and Right move focus between chips and the input without modifying filters', async () => {
  await mount(['iso:>3200', 'lens:35mm']);
  input().focus();
  input().setSelectionRange(0, 0);
  await key(input(), 'ArrowLeft');
  expect(document.activeElement).toBe(edits()[1]);
  await key(edits()[1], 'ArrowLeft');
  expect(document.activeElement).toBe(edits()[0]);
  await key(edits()[0], 'ArrowRight');
  expect(document.activeElement).toBe(edits()[1]);
  await key(edits()[1], 'ArrowRight');
  expect(document.activeElement).toBe(input());
  expect(criteria()).toEqual({ tags: ['iso:>3200', 'lens:35mm'], text: '', mode: 'AND' });
});
