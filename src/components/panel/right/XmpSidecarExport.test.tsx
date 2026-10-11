// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '../../../i18n';
import { mockCommand } from '../../../test/tauriMock';
import { useEditorStore } from '../../../store/useEditorStore';
import { useUIStore } from '../../../store/useUIStore';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';
import XmpSidecarExport from './XmpSidecarExport';
import ExportPanel from './ExportPanel';
import { Status } from '../../ui/ExportImportProperties';

const dialog = vi.hoisted(() => ({ save: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: dialog.save }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initial = useEditorStore.getState();
const initialUI = useUIStore.getState();
let root: Root;
let container: HTMLDivElement;
let calls: Array<Record<string, unknown>>;

beforeEach(() => {
  calls = [];
  useUIStore.setState({ activeView: 'editor' });
  dialog.save.mockReset().mockResolvedValue('/owned/test.xmp');
  mockCommand('export_xmp_round_trip', (args = {}) => {
    calls.push(args);
    return {
      filePath: args.filePath,
      written: args.write,
      losses: [{ field: 'masks.0', name: 'Sky mask', reason: 'Stored, not projected.', preservedInPayload: true }],
    };
  });
  useEditorStore.setState({
    selectedImage: {
      path: '/owned/test.ARW',
      isReady: true,
      width: 6000,
      height: 4000,
      isRaw: true,
      thumbnailUrl: '',
      exif: null,
    },
    adjustments: structuredClone(INITIAL_ADJUSTMENTS),
  });
  container = document.createElement('div');
  document.body.appendChild(container);
  root = createRoot(container);
  act(() => root.render(createElement(XmpSidecarExport, { disabled: false })));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  useEditorStore.setState(initial, true);
  useUIStore.setState(initialUI, true);
});

async function click(text: string) {
  const button = Array.from(container.querySelectorAll('button')).find((button) => button.textContent === text);
  expect(button).toBeDefined();
  await act(async () => {
    button!.click();
  });
}

it('shows named losses before writing and exports the current draft without editing it', async () => {
  const draft = useEditorStore.getState().adjustments;
  await click('Review sidecar next to photo');
  expect(calls).toHaveLength(1);
  expect(calls[0].write).toBe(false);
  expect(container.textContent).toContain('Sky mask');
  await click('Write XMP sidecar');
  expect(calls[1]).toMatchObject({ imagePath: '/owned/test.ARW', filePath: '/owned/test.xmp', write: true });
  expect(useEditorStore.getState().adjustments).toBe(draft);
  expect(container.textContent).toContain('XMP sidecar written.');
});

it('does not export after the photo changes while the save dialog is open', async () => {
  let resolve: (value: string) => void = () => {};
  dialog.save.mockReturnValue(
    new Promise<string>((done) => {
      resolve = done;
    }),
  );
  await click('Choose another destination');
  act(() =>
    useEditorStore.setState({
      selectedImage: { ...useEditorStore.getState().selectedImage!, path: '/owned/other.ARW' },
    }),
  );
  await act(async () => resolve('/owned/test.xmp'));
  expect(calls).toHaveLength(0);
  expect(container.textContent).not.toContain('Write XMP sidecar');
});

it('discards the report when the draft changes before confirmation', async () => {
  await click('Review sidecar next to photo');
  act(() => useEditorStore.setState({ adjustments: { ...useEditorStore.getState().adjustments, exposure: 1 } }));
  expect(container.textContent).not.toContain('Write XMP sidecar');
  expect(calls.every((call) => call.write === false)).toBe(true);
});

it('uses a separate sidecar name for a virtual copy', async () => {
  act(() =>
    useEditorStore.setState({
      selectedImage: { ...useEditorStore.getState().selectedImage!, path: '/owned/test.ARW?vc=copy-id' },
    }),
  );
  await click('Review sidecar next to photo');
  expect(calls[0]).toMatchObject({
    imagePath: '/owned/test.ARW?vc=copy-id',
    filePath: '/owned/test-copy-copy-id.xmp',
    write: false,
  });
});

it('hides the photo sidecar export and discards its report when switching to Library', async () => {
  await click('Review sidecar next to photo');
  expect(container.textContent).toContain('Write XMP sidecar');
  act(() => useUIStore.setState({ activeView: 'library' }));
  expect(container.textContent).toBe('');
  act(() => useUIStore.setState({ activeView: 'editor' }));
  expect(container.textContent).not.toContain('Write XMP sidecar');
  expect(calls).toHaveLength(1);
});

it('rejects a pending report after switching to Library and back to Develop', async () => {
  let finish: (report: unknown) => void = () => {};
  mockCommand('export_xmp_round_trip', (args = {}) => {
    calls.push(args);
    return new Promise((done) => {
      finish = done;
    });
  });
  await click('Review sidecar next to photo');
  act(() => useUIStore.setState({ activeView: 'library' }));
  act(() => useUIStore.setState({ activeView: 'editor' }));
  await act(async () => finish({ filePath: '/owned/test.xmp', written: false, losses: [] }));
  expect(calls).toHaveLength(1);
  expect(container.textContent).not.toContain('Write XMP sidecar');
});

it('rejects a destination chosen after switching to Library and back to Develop', async () => {
  let finish: (path: string) => void = () => {};
  dialog.save.mockReturnValue(
    new Promise<string>((done) => {
      finish = done;
    }),
  );
  await click('Choose another destination');
  act(() => useUIStore.setState({ activeView: 'library' }));
  act(() => useUIStore.setState({ activeView: 'editor' }));
  await act(async () => finish('/owned/test.xmp'));
  expect(calls).toHaveLength(0);
  expect(container.textContent).not.toContain('Write XMP sidecar');
});

it('shows Develop photo sidecar controls with App onClose and no Library selection', async () => {
  await act(async () =>
    root.render(
      createElement(ExportPanel, {
        exportState: { status: Status.Idle, progress: { current: 0, total: 0 }, errorMessage: '' },
        multiSelectedPaths: [],
        selectedImage: useEditorStore.getState().selectedImage,
        setExportState: () => {},
        appSettings: null,
        onSettingsChange: () => {},
        rootPaths: [],
        isVisible: true,
        onClose: () => {},
      }),
    ),
  );
  expect(container.querySelector('section[aria-label="Lightroom XMP sidecar"]')).not.toBeNull();
});

it('keeps photo sidecar export available when raster padding is invalid', async () => {
  await act(async () =>
    root.render(
      createElement(ExportPanel, {
        exportState: { status: Status.Idle, progress: { current: 0, total: 0 }, errorMessage: '' },
        multiSelectedPaths: ['/owned/test.ARW'],
        selectedImage: useEditorStore.getState().selectedImage,
        setExportState: () => {},
        appSettings: null,
        onSettingsChange: () => {},
        rootPaths: [],
        isVisible: true,
        onClose: () => {},
      }),
    ),
  );
  const toggle = container.querySelector<HTMLInputElement>('#switch-pad-to-aspect-ratio');
  expect(toggle).not.toBeNull();
  await act(async () => toggle!.click());
  const ratio = container.querySelector<HTMLInputElement>('input[aria-label="Aspect ratio width"]');
  expect(ratio).not.toBeNull();
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(ratio, '0');
    ratio!.dispatchEvent(new Event('input', { bubbles: true }));
  });
  expect(container.querySelector('input[aria-label="Aspect ratio width"]')).toBeNull();
  await click('Review sidecar next to photo');
  expect(container.textContent).toContain('Write XMP sidecar');
  expect(calls[0].write).toBe(false);
});

it('refuses photo sidecar preview while the selected photo is loading', async () => {
  act(() =>
    useEditorStore.setState({ selectedImage: { ...useEditorStore.getState().selectedImage!, isReady: false } }),
  );
  const button = [...container.querySelectorAll('button')].find(
    (button) => button.textContent === 'Review sidecar next to photo',
  );
  expect(button?.disabled).toBe(true);
  await click('Review sidecar next to photo');
  expect(calls).toHaveLength(0);
});
