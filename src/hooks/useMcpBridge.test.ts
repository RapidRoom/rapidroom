// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { emit, invoke, listen, listenerCount, mockCommand } from '../test/tauriMock';
import { useEditorStore } from '../store/useEditorStore';
import { INITIAL_ADJUSTMENTS, type Adjustments } from '../utils/adjustments';
import { composeParametricCurves } from '../utils/presetAmount';
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
  it('preserves an imported nested-look curve during an unrelated MCP edit', async () => {
    const lookCurve = [
      { x: 0, y: 12 },
      { x: 128, y: 165 },
      { x: 255, y: 255 },
    ];
    const nestedLookCurves = [{ amount: 1, points: lookCurve }];
    const imported: Adjustments = {
      ...INITIAL_ADJUSTMENTS,
      curveMode: 'parametric',
      curves: { ...INITIAL_ADJUSTMENTS.curves, luma: lookCurve },
      pointCurves: INITIAL_ADJUSTMENTS.curves,
      nestedLookCurves,
    };
    useEditorStore.setState({ selectedImage: image, adjustments: imported });
    useEditorStore.getState().resetHistory(imported);
    await mount();
    renderChanges();

    const incoming: Adjustments = { ...imported, exposure: 0.5 };
    delete incoming.nestedLookCurves;
    await command('apply-adjustments', incoming);

    expect(responses.at(-1)?.error).toBeNull();
    expect(useEditorStore.getState().adjustments.curves.luma).toEqual(lookCurve);
    expect(useEditorStore.getState().adjustments.exposure).toBe(0.5);
  });

  it('keeps the imported nested-look contribution when a parametric curve changes', async () => {
    const lookCurve = [
      { x: 0, y: 12 },
      { x: 128, y: 165 },
      { x: 255, y: 255 },
    ];
    const nestedLookCurves = [{ amount: 1, points: lookCurve }];
    const imported: Adjustments = {
      ...INITIAL_ADJUSTMENTS,
      curveMode: 'parametric',
      curves: { ...INITIAL_ADJUSTMENTS.curves, luma: lookCurve },
      pointCurves: INITIAL_ADJUSTMENTS.curves,
      nestedLookCurves,
    };
    useEditorStore.setState({ selectedImage: image, adjustments: imported });
    useEditorStore.getState().resetHistory(imported);
    await mount();
    renderChanges();

    const incoming: Adjustments = {
      ...imported,
      parametricCurve: {
        ...imported.parametricCurve!,
        luma: { ...imported.parametricCurve!.luma, shadows: 20 },
      },
    };
    delete incoming.nestedLookCurves;
    await command('apply-adjustments', incoming);

    expect(responses.at(-1)?.error).toBeNull();
    const result = useEditorStore.getState().adjustments;
    expect(result.curves).toEqual(
      composeParametricCurves(result.parametricCurve!, result.pointCurves!, nestedLookCurves),
    );
    expect(result.curves).not.toEqual(composeParametricCurves(result.parametricCurve!, result.pointCurves!));
    expect(useEditorStore.getState().adjustments.nestedLookCurves).toEqual(nestedLookCurves);
  });

  it('treats an explicit MCP curves change as a raw point-curve edit with unchanged Look metadata', async () => {
    const lookCurve = [
      { x: 0, y: 12 },
      { x: 128, y: 165 },
      { x: 255, y: 255 },
    ];
    const nestedLookCurves = [{ amount: 1, points: lookCurve }];
    const imported: Adjustments = {
      ...INITIAL_ADJUSTMENTS,
      curveMode: 'parametric',
      curves: { ...INITIAL_ADJUSTMENTS.curves, luma: lookCurve },
      pointCurves: INITIAL_ADJUSTMENTS.curves,
      nestedLookCurves,
    };
    useEditorStore.setState({ selectedImage: image, adjustments: imported });
    useEditorStore.getState().resetHistory(imported);
    await mount();
    renderChanges();

    const editedPointCurves = {
      ...INITIAL_ADJUSTMENTS.curves,
      luma: [
        { x: 0, y: 0 },
        { x: 128, y: 110 },
        { x: 255, y: 255 },
      ],
    };
    const incoming: Adjustments = { ...imported, curves: editedPointCurves };
    delete incoming.nestedLookCurves;
    await command('apply-adjustments', incoming);

    const result = useEditorStore.getState().adjustments;
    expect(responses.at(-1)?.error).toBeNull();
    expect(result.pointCurves).toEqual(editedPointCurves);
    expect(result.curves).toEqual(
      composeParametricCurves(result.parametricCurve!, editedPointCurves, nestedLookCurves),
    );
  });

  it('applies explicit MCP curves edits in ordinary point mode with stale pointCurves metadata', async () => {
    const initial: Adjustments = {
      ...INITIAL_ADJUSTMENTS,
      curveMode: 'point',
      curves: INITIAL_ADJUSTMENTS.curves,
      pointCurves: INITIAL_ADJUSTMENTS.pointCurves,
    };
    useEditorStore.setState({ selectedImage: image, adjustments: initial });
    useEditorStore.getState().resetHistory(initial);
    await mount();
    renderChanges();

    const editedCurves = {
      ...INITIAL_ADJUSTMENTS.curves,
      luma: [
        { x: 0, y: 0 },
        { x: 128, y: 110 },
        { x: 255, y: 255 },
      ],
    };
    await command('apply-adjustments', { ...initial, curves: editedCurves });

    const result = useEditorStore.getState().adjustments;
    expect(responses.at(-1)?.error).toBeNull();
    expect(result.curves.luma).toEqual(editedCurves.luma);
    expect(result.pointCurves?.luma).toEqual(editedCurves.luma);
  });

  it('applies explicit MCP curves edits in point mode over a retained nested Look', async () => {
    const lookCurve = [
      { x: 0, y: 12 },
      { x: 128, y: 165 },
      { x: 255, y: 255 },
    ];
    const nestedLookCurves = [{ amount: 1, points: lookCurve }];
    const imported: Adjustments = {
      ...INITIAL_ADJUSTMENTS,
      curveMode: 'point',
      curves: { ...INITIAL_ADJUSTMENTS.curves, luma: lookCurve },
      pointCurves: INITIAL_ADJUSTMENTS.curves,
      nestedLookCurves,
    };
    useEditorStore.setState({ selectedImage: image, adjustments: imported });
    useEditorStore.getState().resetHistory(imported);
    await mount();
    renderChanges();

    const editedCurves = {
      ...INITIAL_ADJUSTMENTS.curves,
      luma: [
        { x: 0, y: 0 },
        { x: 128, y: 110 },
        { x: 255, y: 255 },
      ],
    };
    const incoming: Adjustments = { ...imported, curves: editedCurves };
    delete incoming.nestedLookCurves;
    await command('apply-adjustments', incoming);

    const result = useEditorStore.getState().adjustments;
    expect(responses.at(-1)?.error).toBeNull();
    expect(result.pointCurves).toEqual(editedCurves);
    expect(result.curves).toEqual(
      composeParametricCurves(INITIAL_ADJUSTMENTS.parametricCurve!, editedCurves, nestedLookCurves),
    );
  });

  it('delivers commands during navigation callback changes and uses the latest selector', async () => {
    useEditorStore.setState({ selectedImage: image });
    const first = vi.fn(async () => undefined);
    const latest = vi.fn(async () => undefined);
    function Harness({ select }: { select: typeof first }) {
      useMcpBridge(select);
      return null;
    }
    root = createRoot(document.createElement('div'));
    await act(async () => root!.render(createElement(Harness, { select: first })));
    const register = listen.getMockImplementation()!;
    let release!: () => void;
    const blocked = new Promise<void>((resolve) => {
      release = resolve;
    });
    listen.mockImplementationOnce(async (...args) => {
      await blocked;
      return register(...args);
    });
    try {
      await act(async () => root!.render(createElement(Harness, { select: latest })));
      await command('editor-context');
      expect(responses).toHaveLength(1);
      expect(responses[0].error).toBeNull();
      await command('select-image');
      expect(latest).toHaveBeenCalledWith(path, true);
      expect(first).not.toHaveBeenCalled();
      expect(responses).toHaveLength(2);
    } finally {
      release();
      await act(async () => {
        await blocked;
      });
      listen.mockReset().mockImplementation(register);
    }
  });

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

  it('rejects a queued single-mask snapshot after a user edit without overwriting it', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    const base = useEditorStore.getState().adjustments;
    const edited = { ...base, exposure: 1 };
    await act(async () => useEditorStore.setState({ adjustments: edited }));
    debouncedSetHistory(edited);
    await act(async () =>
      emit('mcp-command', {
        requestId: 'mask-request',
        kind: 'apply-adjustments',
        path,
        adjustments: { ...base, contrast: 20 },
        baseAdjustments: base,
      }),
    );
    expect(responses.at(-1)?.error).toMatch(/edit changed/);
    expect(useEditorStore.getState().adjustments).toEqual(edited);
    expect(useEditorStore.getState().history.map((entry) => entry.exposure)).toEqual([0, 1]);
  });

  it('rejects a queued single-mask snapshot after navigation without selecting the old photo', async () => {
    useEditorStore.setState({ selectedImage: { ...image, path: '/photos/b.raw' } });
    const select = await mount();
    const before = useEditorStore.getState();
    await act(async () =>
      emit('mcp-command', {
        requestId: 'mask-request',
        kind: 'apply-adjustments',
        path,
        adjustments: { ...INITIAL_ADJUSTMENTS, contrast: 20 },
        baseAdjustments: INITIAL_ADJUSTMENTS,
      }),
    );
    expect(responses.at(-1)?.error).toMatch(/image changed/);
    expect(select).not.toHaveBeenCalled();
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
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
      response: { imagePath: null, editRevision: null, dimensions: null, crop: null, masks: [] },
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
        editRevision: 'revision',
        dimensions: { width: 6000, height: 4000 },
        exif: { Make: 'Sony', Model: 'A7C II' },
        virtualCopy: null,
      },
    });
    expect((responses.at(-1)?.response as { exif: unknown }).exif).not.toHaveProperty('PrivateUnknown');
  });

  it('reads current revision and retained snapshots without committing pending GUI history', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    const gui = { ...INITIAL_ADJUSTMENTS, exposure: 1 };
    useEditorStore.setState({ adjustments: gui });
    debouncedSetHistory(gui);
    const before = useEditorStore.getState();
    await command('revision-state');
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(useEditorStore.getState().adjustments).toBe(gui);
    expect(responses.at(-1)).toMatchObject({
      response: {
        state: { editRevision: 'revision', adjustments: gui },
        actor: 'user',
        history: before.history.map((adjustments) => ({ adjustments })),
      },
      error: null,
    });
  });

  it('reports the actual assistant history actor for a committed edit', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    const ai = { ...INITIAL_ADJUSTMENTS, exposure: 0.2 };
    useEditorStore.getState().setEditor({ adjustments: ai });
    useEditorStore
      .getState()
      .pushHistory(ai, { actor: 'assistant', changedKeys: ['exposure'], label: 'AI: Exposure +0.2', timestamp: 1 });
    await command('revision-state');
    expect(responses.at(-1)).toMatchObject({
      response: { actor: 'assistant', state: { adjustments: ai } },
      error: null,
    });
  });

  it('refuses a revision read for a photo that is no longer active', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    useEditorStore.setState({ selectedImage: { ...image, path: '/photos/b.raw' } });
    const before = useEditorStore.getState();
    await command('revision-state');
    expect(responses.at(-1)).toMatchObject({ error: 'Active image changed before the MCP revision read' });
    expect(useEditorStore.getState()).toBe(before);
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
