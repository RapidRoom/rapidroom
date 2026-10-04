// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../../test/tauriMock';
import { Invokes } from '../ui/AppProperties';
import { RenamePreview, finalizeRenameTemplate, pathMapper } from '../../utils/batchRename';
import RenameFileModal from './RenameFileModal';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => (options ? `${key} ${JSON.stringify(options)}` : key),
  }),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
let container: HTMLDivElement;

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  container?.remove();
});

const PAIR_PREVIEW: RenamePreview = {
  entries: [
    { from: '/p/IMG_1.ARW', to: '/p/trip_1.ARW', kind: 'image', photo: 0, conflict: null },
    { from: '/p/IMG_1.JPG', to: '/p/trip_1.JPG', kind: 'image', photo: 0, conflict: null },
    { from: '/p/IMG_1.ARW.xmp', to: '/p/trip_1.ARW.xmp', kind: 'sidecar', photo: 0, conflict: null },
    { from: '/p/IMG_2.ARW', to: '/p/trip_2.ARW', kind: 'image', photo: 1, conflict: null },
  ],
  errors: [],
  photoCount: 2,
  conflictCount: 0,
};

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 200));
  });
}

async function mount({
  files = ['/p/IMG_1.ARW', '/p/IMG_2.ARW'],
  preview = (): RenamePreview => PAIR_PREVIEW,
  lastRename = null as unknown,
} = {}) {
  mockCommand(Invokes.GetLastRename, () => lastRename);
  mockCommand(Invokes.PreviewRenameFiles, () => preview());
  const onSave = vi.fn();
  const onClose = vi.fn();
  const onUndo = vi.fn();
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () =>
    root?.render(
      <RenameFileModal filesToRename={files} isOpen={true} onClose={onClose} onSave={onSave} onUndo={onUndo} />,
    ),
  );
  await settle();
  return { onSave, onClose, onUndo };
}

function button(label: string) {
  const found = Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.startsWith(label));
  if (!found) throw new Error(`Button ${label} not found`);
  return found;
}

function rows() {
  return Array.from(container.querySelectorAll('[data-testid="rename-preview"] li'));
}

function setTemplate(value: string) {
  const input = container.querySelector<HTMLInputElement>('[data-testid="rename-template"]')!;
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
  setter.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

function lastPreviewArgs() {
  const calls = invoke.mock.calls.filter(([cmd]) => cmd === Invokes.PreviewRenameFiles);
  return calls[calls.length - 1][1] as { nameTemplate: string; options: { groupMode: string } };
}

describe('RenameFileModal', () => {
  it('shows old → new for every file, sidecars included, and saves', async () => {
    const { onSave, onClose } = await mount();
    expect(rows().map((row) => row.textContent)).toEqual([
      'IMG_1.ARWtrip_1.ARW',
      'IMG_1.JPGtrip_1.JPG',
      'IMG_1.ARW.xmptrip_1.ARW.xmp',
      'IMG_2.ARWtrip_2.ARW',
    ]);
    expect(rows()[2].getAttribute('data-kind')).toBe('sidecar');
    expect(container.textContent).toContain('"photos":2,"files":4');

    const save = button('modals.renameFile.save');
    expect(save.disabled).toBe(false);
    await act(async () => save.click());
    expect(onSave).toHaveBeenCalledWith('{original_filename}', { groupMode: 'auto', groupSeconds: 1 });
    expect(onClose).toHaveBeenCalled();
  });

  it('flags collisions and blocks the rename', async () => {
    const { onSave } = await mount({
      preview: () => ({
        ...PAIR_PREVIEW,
        entries: [
          { from: '/p/a.jpg', to: '/p/x.jpg', kind: 'image', photo: 0, conflict: 'inBatch' },
          { from: '/p/b.jpg', to: '/p/x.jpg', kind: 'image', photo: 1, conflict: 'inBatch' },
          { from: '/p/c.jpg', to: '/p/taken.jpg', kind: 'image', photo: 2, conflict: 'existing' },
        ],
        conflictCount: 3,
      }),
    });
    expect(rows().map((row) => row.getAttribute('data-conflict'))).toEqual(['inBatch', 'inBatch', 'existing']);
    expect(rows()[2].textContent).toContain('modals.renameFile.conflictExisting');
    expect(container.textContent).toContain('modals.renameFile.conflicts {"count":3}');
    const save = button('modals.renameFile.save');
    expect(save.disabled).toBe(true);
    await act(async () => save.click());
    expect(onSave).not.toHaveBeenCalled();
  });

  it('shows unknown tokens as errors and blocks the rename', async () => {
    await mount({ preview: () => ({ ...PAIR_PREVIEW, entries: [], errors: ['Unknown token {sequense}'] }) });
    await act(async () => setTemplate('trip_{sequense}'));
    await settle();
    expect(container.querySelector('[data-testid="rename-errors"]')?.textContent).toBe('Unknown token {sequense}');
    expect(button('modals.renameFile.save').disabled).toBe(true);
  });

  it('shows a backend refusal, such as Card mode, instead of a preview', async () => {
    await mount({
      preview: () => {
        throw new Error('The card is read-only');
      },
    });
    expect(container.querySelector('[data-testid="rename-errors"]')?.textContent).toContain('read-only');
    expect(button('modals.renameFile.save').disabled).toBe(true);
  });

  it('previews the template that will be applied, with group options only when grouping', async () => {
    const { onSave } = await mount();
    await act(async () => setTemplate('trip'));
    await settle();
    expect(lastPreviewArgs().nameTemplate).toBe('trip_{sequence}');
    expect(container.querySelector('[data-testid="rename-group-options"]')).toBeNull();

    await act(async () => setTemplate('{YYYY}{MM}{DD}_{group}-{member}'));
    await settle();
    expect(lastPreviewArgs().nameTemplate).toBe('{YYYY}{MM}{DD}_{group}-{member}');
    const options = container.querySelector('[data-testid="rename-group-options"]');
    expect(options).not.toBeNull();

    const [, selectionRadio] = Array.from(options!.querySelectorAll<HTMLInputElement>('input[type="radio"]'));
    await act(async () => selectionRadio.click());
    await settle();
    expect(lastPreviewArgs().options.groupMode).toBe('selection');

    await act(async () => button('modals.renameFile.save').click());
    expect(onSave).toHaveBeenCalledWith('{YYYY}{MM}{DD}_{group}-{member}', { groupMode: 'selection', groupSeconds: 1 });
  });

  it('offers undo for the last rename when there is one', async () => {
    const { onUndo, onClose } = await mount({
      lastRename: { renamedAt: '2026-10-04T10:00:00Z', photoCount: 3, fileCount: 7 },
    });
    await act(async () => button('modals.renameFile.undoLast').click());
    expect(onUndo).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('has no undo button when nothing was renamed', async () => {
    await mount();
    expect(() => button('modals.renameFile.undoLast')).toThrow();
  });

  it('prefills a single file with its name and does not add a sequence', async () => {
    await mount({ files: ['/p/IMG_1.ARW?vc=abc123'] });
    const input = container.querySelector<HTMLInputElement>('[data-testid="rename-template"]')!;
    expect(input.value).toBe('IMG_1');
    expect(lastPreviewArgs().nameTemplate).toBe('IMG_1');
  });
});

describe('batch rename helpers', () => {
  it('adds a sequence only when several images would otherwise share a name', () => {
    expect(finalizeRenameTemplate('trip', false)).toBe('trip_{sequence}');
    expect(finalizeRenameTemplate('trip', true)).toBe('trip');
    expect(finalizeRenameTemplate('trip_{member}', false)).toBe('trip_{member}');
    expect(finalizeRenameTemplate('  ', false)).toBe('');
  });

  it('maps library paths, virtual copies included, through a rename', () => {
    const map = pathMapper({
      files: [],
      images: [
        { from: '/p/a.ARW', to: '/p/b.ARW' },
        { from: '/p/a.ARW?vc=1', to: '/p/b.ARW?vc=1' },
      ],
    });
    expect(map('/p/a.ARW')).toBe('/p/b.ARW');
    expect(map('/p/a.ARW?vc=1')).toBe('/p/b.ARW?vc=1');
    expect(map('/p/other.ARW')).toBe('/p/other.ARW');
  });
});
