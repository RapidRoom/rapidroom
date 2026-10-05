// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { emit, invoke, listenerCount, mockCommand } from '../test/tauriMock';
import { useEditorStore } from '../store/useEditorStore';
import { INITIAL_ADJUSTMENTS, type Adjustments } from '../utils/adjustments';
import { debouncedSetHistory } from './useEditorActions';
import { useMcpBridge } from './useMcpBridge';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialState = useEditorStore.getState();
const path = '/photos/a.raw';
const image = { path, width: 6000, height: 4000, isRaw: true, isReady: true, thumbnailUrl: '', exif: {} };
let root: Root | undefined;
let stopRenderer: (() => void) | undefined;
let responses: Array<Record<string, unknown>>;

beforeEach(() => {
  vi.useFakeTimers();
  useEditorStore.setState(initialState, true);
  responses = [];
  mockCommand('mcp_status', () => ({}));
  mockCommand('clear_editor_session', () => undefined);
  mockCommand('sync_editor_state', () => ({
    imagePath: useEditorStore.getState().selectedImage?.path,
    adjustments: useEditorStore.getState().adjustments,
    editRevision: 'revision',
    validationError: null,
  }));
  mockCommand('ui_response', (args) => responses.push(args!));
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = undefined;
  stopRenderer?.();
  stopRenderer = undefined;
  debouncedSetHistory.cancel();
  useEditorStore.setState(initialState, true);
  vi.useRealTimers();
});

async function mount(select = vi.fn(async () => useEditorStore.setState({ selectedImage: image }))) {
  function Harness() {
    useMcpBridge(select);
    return null;
  }
  root = createRoot(document.createElement('div'));
  await act(async () => root!.render(createElement(Harness)));
  return select;
}

async function command(kind: string, adjustments?: Adjustments) {
  await act(async () => {
    await emit('mcp-command', { requestId: 'request', kind, path, adjustments });
  });
}

function renderChanges() {
  stopRenderer = useEditorStore.subscribe((state, previous) => {
    if (state.adjustments !== previous.adjustments) {
      useEditorStore.setState({
        lastRenderedAdjustments: state.adjustments,
        previewRenderVersion: state.previewRenderVersion + 1,
      });
    }
  });
}

describe('MCP editor bridge review', () => {
  it('stays idle in a build without the MCP command', async () => {
    mockCommand('mcp_status', () => {
      throw new Error('unsupported command');
    });
    await mount();
    expect(listenerCount('mcp-command')).toBe(0);
    expect(invoke.mock.calls.map(([name]) => name)).toEqual(['mcp_status']);
  });

  it('opens the first photo without an existing editor session', async () => {
    expect(useEditorStore.getState().selectedImage).toBeNull();
    const select = await mount();
    await command('select-image');
    expect(select).toHaveBeenCalledWith(path, true);
    expect(responses).toHaveLength(1);
    expect(responses[0]).toMatchObject({ error: null, response: { imagePath: path, renderPending: false } });
  });

  it('acknowledges a no-op immediately without history or a new render', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    const before = useEditorStore.getState();
    await command('apply-adjustments', { ...INITIAL_ADJUSTMENTS });
    expect(responses).toHaveLength(1);
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(useEditorStore.getState().previewRenderVersion).toBe(before.previewRenderVersion);
    expect(responses[0]).toMatchObject({ response: { renderPending: false } });
  });

  it('flushes a pending GUI edit before the MCP edit, preserving both undo steps', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    renderChanges();
    const gui = { ...INITIAL_ADJUSTMENTS, exposure: 1 };
    await act(async () => useEditorStore.setState({ adjustments: gui }));
    debouncedSetHistory(gui);
    await command('apply-adjustments', { ...gui, contrast: 20 });
    expect(responses).toHaveLength(1);
    expect(useEditorStore.getState().history.map((entry) => [entry.exposure, entry.contrast])).toEqual([
      [0, 0],
      [1, 0],
      [1, 20],
    ]);
    await act(async () => vi.advanceTimersByTimeAsync(1000));
    expect(useEditorStore.getState().history).toHaveLength(3);
    await act(async () => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments).toEqual(gui);
    await act(async () => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments).toEqual(INITIAL_ADJUSTMENTS);
  });

  it('resets as one undoable step without a direct sidecar reset command', async () => {
    const edited: Adjustments = {
      ...INITIAL_ADJUSTMENTS,
      contrast: 30,
      crop: { unit: '%', x: 10, y: 10, width: 80, height: 80 },
    };
    useEditorStore.setState({ selectedImage: image, adjustments: edited, history: [edited], historyIndex: 0 });
    await mount();
    renderChanges();
    await command('reset-adjustments');
    expect(responses).toHaveLength(1);
    expect(useEditorStore.getState().adjustments).toMatchObject({ contrast: 0, crop: null, aspectRatio: 1.5 });
    expect(useEditorStore.getState().history).toHaveLength(2);
    await act(async () => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments).toEqual(edited);
    expect(invoke.mock.calls.some(([name]) => name === 'reset_adjustments_for_paths')).toBe(false);
  });

  it('lists AI entries without changing history and undoes/redoes through the shared stack', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    renderChanges();
    await command('apply-adjustments', { ...INITIAL_ADJUSTMENTS, exposure: 0.4 });
    await command('apply-adjustments', { ...INITIAL_ADJUSTMENTS, exposure: 0.4, highlights: -30 });
    const before = useEditorStore.getState();
    await command('history-list');
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(responses.at(-1)).toMatchObject({
      response: {
        historyIndex: 2,
        entries: [
          { label: 'Initial State' },
          { actor: 'assistant', label: 'AI: Exposure +0.4' },
          { actor: 'assistant', label: 'AI: Highlights -30' },
        ],
      },
    });
    await command('undo');
    expect(useEditorStore.getState().adjustments).toMatchObject({ exposure: 0.4, highlights: 0 });
    await command('undo');
    expect(useEditorStore.getState().adjustments).toEqual(INITIAL_ADJUSTMENTS);
    await command('redo');
    expect(useEditorStore.getState().adjustments).toMatchObject({ exposure: 0.4, highlights: 0 });
    expect(useEditorStore.getState().history).toHaveLength(3);
  });

  it('reports an empty editor context before a photo is open', async () => {
    await mount();
    await act(async () => emit('mcp-command', { requestId: 'empty', kind: 'editor-context', path: '' }));
    expect(responses.at(-1)).toMatchObject({
      response: { imagePath: null, dimensions: null, crop: null, masks: [] },
      error: null,
    });
  });

  it('reads bounded editor context without committing pending GUI history', async () => {
    useEditorStore.setState({
      selectedImage: { ...image, exif: { Make: 'Sony', Model: 'A7C II', PrivateUnknown: 'omitted' } },
    });
    await mount();
    const before = useEditorStore.getState();
    debouncedSetHistory({ ...INITIAL_ADJUSTMENTS, exposure: 1 });
    await command('editor-context');
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(responses.at(-1)).toMatchObject({
      response: {
        imagePath: path,
        dimensions: { width: 6000, height: 4000 },
        exif: { Make: 'Sony', Model: 'A7C II' },
        virtualCopy: null,
      },
    });
    expect((responses.at(-1)?.response as { exif: unknown }).exif).not.toHaveProperty('PrivateUnknown');
  });

  it('does not mirror a new photo under the old path when it changes during a render wait', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    await command('apply-adjustments', { ...INITIAL_ADJUSTMENTS, contrast: 20 });
    expect(responses).toHaveLength(0);
    const nextImage = { ...image, path: '/photos/b.raw' };
    const nextEdit = { ...INITIAL_ADJUSTMENTS, exposure: 2 };
    await act(async () => {
      useEditorStore.setState({ selectedImage: nextImage, adjustments: nextEdit });
      await vi.advanceTimersByTimeAsync(250);
    });
    expect(responses).toHaveLength(1);
    expect(responses[0]).toMatchObject({ error: expect.stringContaining('Active image changed') });
    expect(useEditorStore.getState().selectedImage?.path).toBe(nextImage.path);
    expect(useEditorStore.getState().adjustments).toEqual(nextEdit);
    const writes = invoke.mock.calls.filter(([name]) => name === 'sync_editor_state');
    expect(writes).not.toContainEqual([
      'sync_editor_state',
      expect.objectContaining({ path, adjustments: expect.objectContaining({ exposure: 2 }) }),
    ]);
  });

  it('reports an applied edit as renderPending at 40 seconds instead of losing it', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    await command('apply-adjustments', { ...INITIAL_ADJUSTMENTS, contrast: 20 });
    expect(responses).toHaveLength(0);
    await act(async () => vi.advanceTimersByTimeAsync(40000));
    expect(responses).toHaveLength(1);
    expect(responses[0]).toMatchObject({ error: null, response: { renderPending: true } });
    expect(useEditorStore.getState().adjustments.contrast).toBe(20);
    expect(useEditorStore.getState().history).toHaveLength(2);
  });
});
