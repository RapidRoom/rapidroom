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
import { usePresetCatalog, setCatalogPresets } from '../store/usePresetCatalog';
import { listMcpPresets } from '../utils/mcpPresets';
import { useAppliedPresetStore } from '../utils/appliedPreset';
import { usePresetApplication } from '../utils/presetApplication';
import { useLibraryStore } from '../store/useLibraryStore';
import { useUIStore } from '../store/useUIStore';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initialState = useEditorStore.getState();
const path = '/photos/a.raw';
const image = { path, width: 6000, height: 4000, isRaw: true, isReady: true, thumbnailUrl: '', exif: {} };
let root: Root | undefined;
let stopRenderer: (() => void) | undefined;
let responses: Array<Record<string, unknown>>;

beforeEach(() => {
  vi.useFakeTimers();
  useAppliedPresetStore.setState({ records: {} });
  useEditorStore.setState(initialState, true);
  responses = [];
  usePresetCatalog.setState({ presets: [], loaded: false, isLoading: true });
  usePresetApplication.setState({ application: null });
  useLibraryStore.setState({ cardBrowseRoot: null });
  useUIStore.setState({ confirmModalState: { isOpen: false } });
  mockCommand('save_applied_preset', () => undefined);
  mockCommand('validate_mcp_adjustments', () => undefined);
  mockCommand('load_applied_preset', () => null);
  mockCommand('save_metadata_and_update_thumbnail', () => undefined);
  mockCommand('mcp_status', () => ({}));
  mockCommand('clear_editor_session', () => undefined);
  mockCommand('get_default_raw_lens_adjustments', () => ({}));
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

  it('reads an earlier snapshot without undoing or flushing a pending GUI history edit', async () => {
    const old = { ...INITIAL_ADJUSTMENTS, exposure: 0.5 };
    const current = { ...INITIAL_ADJUSTMENTS, exposure: 1.5 };
    useEditorStore.setState({ selectedImage: image, adjustments: current, history: [old, current], historyIndex: 1 });
    await mount();
    const pushHistory = vi.fn();
    useEditorStore.setState({ pushHistory });
    debouncedSetHistory(current);
    const before = useEditorStore.getState();
    await act(async () => emit('mcp-command', { requestId: 'snapshot', kind: 'history-state', path, historyIndex: 0 }));
    expect(responses[0]).toMatchObject({ error: null, response: { historyIndex: 0, adjustments: old } });
    const after = useEditorStore.getState();
    expect(after.adjustments).toBe(before.adjustments);
    expect(after.history).toBe(before.history);
    expect(after.historyIndex).toBe(1);
    expect(after.previewRenderVersion).toBe(before.previewRenderVersion);
    expect(pushHistory).not.toHaveBeenCalled();
    debouncedSetHistory.flush();
    expect(pushHistory).toHaveBeenCalledWith(current);
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

  it('records a labelled multi-mask commit once and undoes the complete batch', async () => {
    useEditorStore.setState({ selectedImage: image });
    await mount();
    renderChanges();
    const base = useEditorStore.getState().adjustments;
    const masks = ['Face', 'Sky'].map((name, i) => ({
      id: `m${i}`,
      name,
      visible: true,
      invert: false,
      opacity: 100,
      adjustments: { exposure: i + 0.2 },
      subMasks: [],
    }));
    await act(async () =>
      emit('mcp-command', {
        requestId: 'batch',
        kind: 'apply-adjustments',
        path,
        adjustments: { ...base, masks },
        baseAdjustments: base,
        label: 'AI: Portrait masks',
      }),
    );
    const after = useEditorStore.getState();
    expect(responses.at(-1)?.error).toBeNull();
    expect(after.history).toHaveLength(2);
    expect(after.adjustments.masks.map((m) => m.name)).toEqual(['Face', 'Sky']);
    expect(after.historyDetails[1]?.label).toBe('AI: Portrait masks');
    after.undo();
    expect(useEditorStore.getState().adjustments).toEqual(base);
    useEditorStore.getState().redo();
    expect(useEditorStore.getState().adjustments).toEqual(after.adjustments);
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
    mockCommand('get_default_raw_lens_adjustments', () => ({
      lensCorrectionMode: 'auto',
      cameraLensCorrectionVersion: 1,
    }));
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
    expect(useEditorStore.getState().adjustments).toMatchObject({
      contrast: 0,
      crop: null,
      aspectRatio: 0,
      lensCorrectionMode: 'auto',
      cameraLensCorrectionVersion: 1,
    });
    expect(useEditorStore.getState().history).toHaveLength(2);
    await act(async () => useEditorStore.getState().undo());
    expect(useEditorStore.getState().adjustments).toEqual(edited);
    expect(invoke.mock.calls.some(([name]) => name === 'reset_adjustments_for_paths')).toBe(false);
  });

  it.each(['selection', 'draft', 'error'])('does not overwrite an editor after a Reset lookup %s', async (reason) => {
    const edited = { ...INITIAL_ADJUSTMENTS, contrast: 30 };
    useEditorStore.setState({ selectedImage: image, adjustments: edited, history: [edited], historyIndex: 0 });
    let release!: (value: unknown) => void;
    let reject!: (error: Error) => void;
    mockCommand(
      'get_default_raw_lens_adjustments',
      () =>
        new Promise((resolve, fail) => {
          release = resolve;
          reject = fail;
        }),
    );
    await mount();
    let pending!: Promise<void>;
    await act(async () => {
      pending = emit('mcp-command', { requestId: 'request', kind: 'reset-adjustments', path });
    });
    const newer = { ...edited, contrast: 45 };
    await act(async () => {
      if (reason === 'selection') useEditorStore.setState({ selectedImage: { ...image, path: '/photos/b.raw' } });
      if (reason === 'draft') useEditorStore.setState({ adjustments: newer });
      if (reason === 'error') reject(new Error('Lens lookup failed'));
      else release({ lensCorrectionMode: 'auto', cameraLensCorrectionVersion: 1 });
      await pending;
    });
    expect(responses).toHaveLength(1);
    expect(responses[0].error).toMatch(reason === 'error' ? /Lens lookup failed/ : /Active edit changed/);
    expect(useEditorStore.getState().adjustments).toBe(reason === 'draft' ? newer : edited);
    expect(useEditorStore.getState().history).toEqual([edited]);
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

describe('protected editing workflow', () => {
  it('applies a discovered Style with the panel closed, shares its Amount baseline and refuses stale edits', async () => {
    useEditorStore.setState({ selectedImage: image, adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 1 } });
    setCatalogPresets([
      { preset: { id: 'style', name: 'Style', presetType: 'style', adjustments: { exposure: 2, contrast: 30 } } },
    ]);
    const id = (await listMcpPresets({})).presets[0].id;
    await mount();
    renderChanges();
    const apply = async (amount: number, baseAdjustments = useEditorStore.getState().adjustments) => {
      const count = responses.length;
      await act(async () => {
        await emit('mcp-command', {
          requestId: 'style',
          kind: 'apply-preset',
          path,
          baseAdjustments,
          options: { presetId: id, amount },
        });
        await vi.waitFor(() => expect(responses).toHaveLength(count + 1));
      });
    };
    await apply(100);
    expect(responses.at(-1)).toMatchObject({ error: null, response: { appliedPreset: { id, amount: 100 } } });
    await apply(50);
    expect(useEditorStore.getState().adjustments.exposure).toBe(1.5);
    await apply(100);
    expect(useEditorStore.getState().adjustments.exposure).toBe(2);
    const before = useEditorStore.getState();
    await apply(20, INITIAL_ADJUSTMENTS);
    expect(responses.at(-1)?.error).toContain('changed');
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(usePresetApplication.getState().application?.amount).toBe(100);
  });

  it('lists and removes the shared ordered stack even after catalog deletion, preserving manual keys and rejecting a fourth atomically', async () => {
    useEditorStore.setState({ selectedImage: image, adjustments: { ...INITIAL_ADJUSTMENTS, exposure: 0.7 } });
    setCatalogPresets(
      ['one', 'two', 'three', 'four'].map((id, i) => ({
        preset: {
          id,
          name: id,
          presetType: 'style' as const,
          adjustments: i === 0 ? { contrast: 40 } : i === 1 ? { shadows: 30 } : { clarity: 10 + i },
        },
      })),
    );
    const catalog = (await listMcpPresets({})).presets;
    const ids = Object.fromEntries(catalog.map((p) => [p.name, p.id]));
    await mount();
    renderChanges();
    const stackCommand = async (kind: string, options?: Record<string, unknown>) => {
      const count = responses.length;
      await act(async () => {
        await emit('mcp-command', {
          requestId: 'stack',
          kind,
          path,
          baseAdjustments: useEditorStore.getState().adjustments,
          options,
        });
        await vi.waitFor(() => expect(responses).toHaveLength(count + 1));
      });
      return responses.at(-1)!;
    };
    for (const name of ['one', 'two', 'three'])
      await stackCommand('apply-preset', { presetId: ids[name], amount: 100 });
    const before = useEditorStore.getState();
    const refused = await stackCommand('apply-preset', { presetId: ids.four });
    expect(refused.error).toBe('Up to 3 Styles. Remove one first.');
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
    const listed = await stackCommand('list-applied-styles');
    expect(listed).toMatchObject({
      error: null,
      response: {
        limit: 3,
        styles: [
          { presetId: ids.one, position: 1 },
          { presetId: ids.two, position: 2 },
          { presetId: ids.three, position: 3 },
        ],
      },
    });
    await act(async () => {
      const manual = { ...useEditorStore.getState().adjustments, exposure: 1.25, contrast: 7 };
      useEditorStore.setState({ adjustments: manual });
      useEditorStore.getState().pushHistory(manual);
    });
    setCatalogPresets([]);
    const removed = await stackCommand('remove-style', { presetId: ids.two });
    expect(removed).toMatchObject({
      error: null,
      response: { removed: true, styleCount: 2, removedStyle: { id: ids.two, name: 'two' } },
    });
    expect(useEditorStore.getState().adjustments).toMatchObject({
      exposure: 1.25,
      contrast: 7,
      shadows: 0,
      clarity: 12,
    });
    expect(useEditorStore.getState().historyDetails.at(-1)).toMatchObject({
      actor: 'assistant',
      label: 'AI: Style 2 removed: two',
    });
    const remaining = await stackCommand('list-applied-styles');
    expect(remaining).toMatchObject({
      response: {
        styles: [
          { presetId: ids.one, position: 1 },
          { presetId: ids.three, position: 2 },
        ],
      },
    });
    expect(JSON.stringify(remaining)).not.toContain('overrides');
    await stackCommand('reset-adjustments');
    expect(await stackCommand('list-applied-styles')).toMatchObject({ response: { styles: [] } });
    await stackCommand('undo');
    expect(await stackCommand('list-applied-styles')).toMatchObject({
      response: { styles: [{ presetId: ids.one }, { presetId: ids.three }] },
    });
  });

  it('commits and removes an Amount0 stack entry without waiting for an unchanged preview', async () => {
    const base = { ...INITIAL_ADJUSTMENTS, exposure: 0.7 };
    useEditorStore.setState({ selectedImage: image, adjustments: base, history: [base], historyIndex: 0 });
    setCatalogPresets([
      { preset: { id: 'zero-look', name: 'Zero look', presetType: 'style', adjustments: { contrast: 40 } } },
    ]);
    const id = (await listMcpPresets({})).presets[0].id;
    await mount();
    const change = async (kind: string, options: Record<string, unknown>) => {
      const count = responses.length;
      await act(async () => {
        await emit('mcp-command', {
          requestId: 'zero',
          kind,
          path,
          baseAdjustments: useEditorStore.getState().adjustments,
          options,
        });
        await vi.waitFor(() => expect(responses).toHaveLength(count + 1));
      });
      expect(responses.at(-1)).toMatchObject({ error: null, response: { renderPending: false } });
    };
    await change('apply-preset', { presetId: id, amount: 0 });
    expect(useEditorStore.getState().adjustments).toEqual(base);
    expect(useEditorStore.getState().history).toHaveLength(2);
    await change('remove-style', { presetId: id });
    expect(useEditorStore.getState().adjustments).toEqual(base);
    expect(useEditorStore.getState().history).toHaveLength(3);
  });

  it('rejects an edit racing native preflight before setting the marker or committing a Style', async () => {
    useEditorStore.setState({ selectedImage: image });
    setCatalogPresets([{ preset: { id: 'style', name: 'Style', adjustments: { exposure: 2 } } }]);
    await mount();
    const base = useEditorStore.getState().adjustments;
    mockCommand('validate_mcp_adjustments', () => useEditorStore.setState({ adjustments: { ...base, contrast: 12 } }));
    await act(async () => {
      await emit('mcp-command', {
        requestId: 'race',
        kind: 'apply-preset',
        path,
        baseAdjustments: base,
        options: { name: 'Style' },
      });
      await vi.waitFor(() => expect(responses).toHaveLength(1));
    });
    expect(responses.at(-1)?.error).toContain('changed');
    expect(useEditorStore.getState().adjustments).toMatchObject({ exposure: 0, contrast: 12 });
    expect(usePresetApplication.getState().application).toBeNull();
  });

  it('copies the current unsaved draft, leaving source state/history untouched and reports the real new path', async () => {
    const draft = { ...INITIAL_ADJUSTMENTS, exposure: 1.7 };
    useEditorStore.setState({ selectedImage: image, adjustments: draft });
    mockCommand('create_mcp_virtual_copy', (args) => {
      expect(args).toEqual({ path, adjustments: draft });
      return path + '?vc=abc123';
    });
    await mount();
    const before = useEditorStore.getState();
    await act(async () =>
      emit('mcp-command', { requestId: 'copy', kind: 'create-virtual-copy', path, baseAdjustments: draft }),
    );
    expect(responses.at(-1)).toMatchObject({
      error: null,
      response: { imagePath: path + '?vc=abc123', virtualCopy: 'abc123', isSelected: false },
    });
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
    expect(useEditorStore.getState().selectedImage?.path).toBe(path);
  });

  it('does not create copies or mutate metadata in Card mode', async () => {
    useEditorStore.setState({ selectedImage: image });
    useLibraryStore.setState({ cardBrowseRoot: '/photos' });
    await mount();
    await act(async () =>
      emit('mcp-command', {
        requestId: 'card',
        kind: 'create-virtual-copy',
        path,
        baseAdjustments: INITIAL_ADJUSTMENTS,
      }),
    );
    expect(responses.at(-1)?.error).toContain('read-only');
    await act(async () =>
      emit('mcp-command', {
        requestId: 'card-metadata',
        kind: 'set-image-metadata',
        path,
        baseAdjustments: INITIAL_ADJUSTMENTS,
        imagePaths: [path],
        changes: { rating: 4 },
      }),
    );
    expect(responses.at(-1)?.error).toContain('read-only');
    expect(
      invoke.mock.calls.some(([name]) => ['create_mcp_virtual_copy', 'set_mcp_image_metadata'].includes(name)),
    ).toBe(false);
  });

  it('assesses substantial edits read-only and opens a Compare versions wipe', async () => {
    const draft = {
      ...INITIAL_ADJUSTMENTS,
      contrast: 12,
      colorGrading: { ...INITIAL_ADJUSTMENTS.colorGrading, balance: 20 },
    };
    useEditorStore.setState({ selectedImage: image, adjustments: draft });
    await mount();
    const before = useEditorStore.getState();
    await act(async () =>
      emit('mcp-command', { requestId: 'assess', kind: 'assess-edits', path, baseAdjustments: draft }),
    );
    expect(responses.at(-1)).toMatchObject({
      error: null,
      response: { assessment: { recommendVirtualCopy: true, hasColorGrading: true } },
    });
    await act(async () =>
      emit('mcp-command', {
        requestId: 'reference',
        kind: 'open-reference-view',
        path,
        baseAdjustments: draft,
        referencePath: path + '?vc=base',
      }),
    );
    expect(useEditorStore.getState().referenceView).toMatchObject({
      mode: 'wipe',
      reference: { path: path + '?vc=base' },
    });
    expect(useEditorStore.getState().adjustments).toBe(before.adjustments);
    expect(useEditorStore.getState().history).toBe(before.history);
  });

  it('requests the app confirmation callback and never invokes a delete command', async () => {
    useEditorStore.setState({ selectedImage: image });
    const requestDeletePaths = vi.fn();
    function Harness() {
      useMcpBridge(
        vi.fn(async () => undefined),
        { requestDeletePaths },
      );
      return null;
    }
    root = createRoot(document.createElement('div'));
    await act(async () => root!.render(createElement(Harness)));
    await act(async () =>
      emit('mcp-command', {
        requestId: 'delete',
        kind: 'request-delete-image',
        path,
        baseAdjustments: INITIAL_ADJUSTMENTS,
      }),
    );
    expect(requestDeletePaths).toHaveBeenCalledExactlyOnceWith([path], true);
    expect(responses.at(-1)).toMatchObject({
      error: null,
      response: { confirmationPending: true, includesVirtualCopies: true },
    });
    expect(invoke.mock.calls.some(([name]) => name.startsWith('delete_'))).toBe(false);
  });
});

describe('adopt AI version without deleting the original RAW', () => {
  const copyPath = path + '?vc=a11234';
  const base = { ...INITIAL_ADJUSTMENTS, exposure: 0.25, contrast: 5 };
  const edited = { ...INITIAL_ADJUSTMENTS, exposure: 1.5, contrast: 22 };
  async function adoptionHarness(revision = 'base-revision') {
    useEditorStore.setState({
      selectedImage: { ...image, path: copyPath },
      adjustments: edited,
      history: [edited],
      historyDetails: [null],
      historyIndex: 0,
    });
    const requestDeletePaths = vi.fn();
    const open = vi.fn(async (target: string) => {
      useEditorStore.getState().resetHistory(base);
      useEditorStore.setState({ selectedImage: { ...image, path: target } });
    });
    mockCommand('sync_editor_state', (args) => ({
      imagePath: args!.path,
      adjustments: args!.adjustments,
      editRevision: args!.adjustments === base ? revision : 'adopted-revision',
      validationError: null,
    }));
    function Harness() {
      useMcpBridge(open, { requestDeletePaths });
      return null;
    }
    root = createRoot(document.createElement('div'));
    await act(async () => root!.render(createElement(Harness)));
    renderChanges();
    return { open, requestDeletePaths };
  }
  async function adopt(deleteCopyAfterAdoption = true) {
    await act(async () => {
      await emit('mcp-command', {
        requestId: 'adopt',
        kind: 'adopt-virtual-copy',
        path: copyPath,
        baseAdjustments: edited,
        originalExpectedRevision: 'base-revision',
        deleteCopyAfterAdoption,
      });
      await vi.waitFor(() => expect(responses).toHaveLength(1));
    });
  }
  it('adopts one labelled undoable recipe onto original, then requests confirmation only for AI-copy deletion', async () => {
    const { open, requestDeletePaths } = await adoptionHarness();
    await adopt();
    expect(open).toHaveBeenCalledExactlyOnceWith(path, true);
    expect(useEditorStore.getState().selectedImage?.path).toBe(path);
    expect(useEditorStore.getState().adjustments).toEqual(edited);
    expect(useEditorStore.getState().history).toEqual([base, edited]);
    expect(useEditorStore.getState().historyDetails[1]).toMatchObject({
      actor: 'assistant',
      label: 'AI: adopted AI edit',
    });
    expect(requestDeletePaths).toHaveBeenCalledExactlyOnceWith([copyPath], true);
    expect(responses[0]).toMatchObject({
      error: null,
      response: { adopted: true, imagePath: path, sourceCopyPath: copyPath, copyDeletionConfirmationPending: true },
    });
    useEditorStore.getState().undo();
    expect(useEditorStore.getState().adjustments).toEqual(base);
    expect(invoke.mock.calls.some(([name]) => name.startsWith('delete_'))).toBe(false);
    expect(
      invoke.mock.calls.filter(([name]) => name === 'save_metadata_and_update_thumbnail').map(([, args]) => args?.path),
    ).toEqual([copyPath, path]);
  });
  it('refuses a changed original and leaves both recipes available without a deletion request', async () => {
    const { requestDeletePaths } = await adoptionHarness('new-user-revision');
    await adopt();
    expect(responses[0].error).toContain('Original edit changed');
    expect(useEditorStore.getState().adjustments).toEqual(base);
    expect(useEditorStore.getState().history).toEqual([base]);
    expect(requestDeletePaths).not.toHaveBeenCalled();
  });
  it('rechecks original changes racing revision validation before writing an AI recipe', async () => {
    const { requestDeletePaths } = await adoptionHarness();
    mockCommand('sync_editor_state', (args) => {
      if (args?.path === path) useEditorStore.setState({ adjustments: { ...base, shadows: 16 } });
      return { imagePath: args?.path, editRevision: 'base-revision' };
    });
    await adopt();
    expect(responses[0].error).toContain('Original changed');
    expect(useEditorStore.getState().adjustments).toMatchObject({ exposure: 0.25, shadows: 16 });
    expect(requestDeletePaths).not.toHaveBeenCalled();
  });
  it('retains the AI copy if persisting the adopted original fails', async () => {
    const { requestDeletePaths } = await adoptionHarness();
    mockCommand('save_metadata_and_update_thumbnail', (args) => {
      if (args?.path === path) throw new Error('disk full');
    });
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    try {
      await adopt();
      expect(responses[0]).toMatchObject({
        error: null,
        response: { adopted: true, originalSaved: false, copyDeletionConfirmationPending: false },
      });
      expect(useEditorStore.getState().adjustments).toEqual(edited);
      expect(requestDeletePaths).not.toHaveBeenCalled();
    } finally {
      error.mockRestore();
    }
  });

  it('keeps the AI copy when deletion was not requested', async () => {
    const { requestDeletePaths } = await adoptionHarness();
    await adopt(false);
    expect(responses[0]).toMatchObject({
      error: null,
      response: { adopted: true, copyDeletionConfirmationPending: false },
    });
    expect(useEditorStore.getState().adjustments).toEqual(edited);
    expect(requestDeletePaths).not.toHaveBeenCalled();
  });
});
