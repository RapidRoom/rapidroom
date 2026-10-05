import { afterEach, describe, expect, it } from 'vitest';
import { useUIStore, reconcileWorkspace, WORKSPACE_LAYOUT_VERSION } from './useUIStore';
import { Panel, WorkspaceState } from '../components/ui/AppProperties';

const initialState = useUIStore.getState();

afterEach(() => {
  useUIStore.setState(initialState, true);
});

describe('useUIStore.cycleLightsOut', () => {
  it('cycles Normal, Dim, Black and back to Normal', () => {
    const { cycleLightsOut } = useUIStore.getState();
    expect(useUIStore.getState().lightsOutMode).toBe('off');
    cycleLightsOut();
    expect(useUIStore.getState().lightsOutMode).toBe('dim');
    cycleLightsOut();
    expect(useUIStore.getState().lightsOutMode).toBe('black');
    cycleLightsOut();
    expect(useUIStore.getState().lightsOutMode).toBe('off');
  });

  it('cycles backward', () => {
    const { cycleLightsOut } = useUIStore.getState();
    cycleLightsOut(-1);
    expect(useUIStore.getState().lightsOutMode).toBe('black');
    cycleLightsOut(-1);
    expect(useUIStore.getState().lightsOutMode).toBe('dim');
  });
});

const savedWorkspace = (overrides: Partial<WorkspaceState>): WorkspaceState => ({
  leftPanelWidth: 300,
  rightPanelWidth: 400,
  leftTopHeight: 450,
  rightTopHeight: 450,
  panelLayout: {
    leftTop: [Panel.Metadata, Panel.FolderTree, Panel.Export],
    leftBottom: [],
    rightTop: [Panel.Adjustments, Panel.Crop, Panel.Masks, Panel.Ai, Panel.Presets],
    rightBottom: [],
  },
  activePanels: { leftTop: Panel.FolderTree, leftBottom: null, rightTop: Panel.Adjustments, rightBottom: null },
  panelSwitcherPlacement: { leftTop: 'bottom', leftBottom: 'bottom', rightTop: 'right', rightBottom: 'right' },
  ...overrides,
});

describe('reconcileWorkspace', () => {
  it('adds a collapsed terminal dock to legacy workspaces only in supported builds', () => {
    const legacy = savedWorkspace({ layoutVersion: WORKSPACE_LAYOUT_VERSION });
    const enabled = reconcileWorkspace(legacy, false, true);
    expect(enabled.panelLayout.bottom).toEqual([Panel.Terminal]);
    expect(enabled.activePanels.bottom).toBeNull();
    expect(enabled.panelLayout.leftTop).toEqual(legacy.panelLayout.leftTop);
    expect(reconcileWorkspace(legacy, false, false).panelLayout.bottom).toEqual([]);
  });

  it('preserves a moved terminal and collapsed dock while filtering duplicate tabs', () => {
    const saved = savedWorkspace({
      layoutVersion: WORKSPACE_LAYOUT_VERSION,
      panelLayout: {
        leftTop: [Panel.FolderTree, Panel.Terminal],
        leftBottom: [],
        rightTop: [Panel.Adjustments],
        rightBottom: [],
        bottom: [Panel.Terminal, Panel.Metadata],
      },
      activePanels: {
        leftTop: Panel.Terminal,
        leftBottom: null,
        rightTop: Panel.Adjustments,
        rightBottom: null,
        bottom: null,
      },
      bottomDockHeight: 360,
    });
    const workspace = reconcileWorkspace(saved, false, true);
    expect(workspace.panelLayout.leftTop).toContain(Panel.Terminal);
    expect(workspace.panelLayout.bottom).toEqual([Panel.Metadata]);
    expect(workspace.activePanels.bottom).toBeNull();
    expect(workspace.bottomDockHeight).toBe(360);
    const off = reconcileWorkspace(saved, false, false);
    expect(Object.values(off.panelLayout).flat()).not.toContain(Panel.Terminal);
  });
  it('puts Presets in the left sidebar for a new workspace', () => {
    const workspace = reconcileWorkspace(undefined, false);
    expect(workspace.panelLayout.leftTop).toContain(Panel.Presets);
    expect(workspace.panelLayout.rightTop).not.toContain(Panel.Presets);
    expect(workspace.layoutVersion).toBe(WORKSPACE_LAYOUT_VERSION);
  });

  it('moves Presets from its old default region once', () => {
    const workspace = reconcileWorkspace(
      savedWorkspace({
        activePanels: { leftTop: Panel.FolderTree, leftBottom: null, rightTop: Panel.Presets, rightBottom: null },
      }),
      false,
    );
    expect(workspace.panelLayout.leftTop).toEqual([Panel.Metadata, Panel.FolderTree, Panel.Export, Panel.Presets]);
    expect(workspace.panelLayout.rightTop).toEqual([Panel.Adjustments, Panel.Crop, Panel.Masks, Panel.Ai]);
    expect(workspace.activePanels.rightTop).toBe(Panel.Adjustments);
    expect(workspace.layoutVersion).toBe(WORKSPACE_LAYOUT_VERSION);
  });

  it('keeps the rest of a custom layout when it migrates', () => {
    const workspace = reconcileWorkspace(
      savedWorkspace({
        panelLayout: {
          leftTop: [Panel.FolderTree],
          leftBottom: [Panel.Metadata],
          rightTop: [Panel.Presets, Panel.Adjustments, Panel.Masks],
          rightBottom: [Panel.Crop, Panel.Ai, Panel.Export],
        },
        activePanels: {
          leftTop: Panel.FolderTree,
          leftBottom: Panel.Metadata,
          rightTop: Panel.Masks,
          rightBottom: Panel.Ai,
        },
      }),
      false,
    );
    expect(workspace.panelLayout).toEqual({
      leftTop: [Panel.FolderTree, Panel.Presets],
      leftBottom: [Panel.Metadata],
      rightTop: [Panel.Adjustments, Panel.Masks],
      rightBottom: [Panel.Crop, Panel.Ai, Panel.Export],
      bottom: [],
    });
    expect(workspace.activePanels).toEqual({
      leftTop: Panel.FolderTree,
      leftBottom: Panel.Metadata,
      rightTop: Panel.Masks,
      rightBottom: Panel.Ai,
      bottom: null,
    });
    expect(workspace.leftPanelWidth).toBe(300);
    expect(workspace.rightPanelWidth).toBe(400);
  });

  it('keeps Presets where the user put it outside its old default region', () => {
    const workspace = reconcileWorkspace(
      savedWorkspace({
        panelLayout: {
          leftTop: [Panel.Metadata, Panel.FolderTree, Panel.Export],
          leftBottom: [],
          rightTop: [Panel.Adjustments, Panel.Crop, Panel.Masks, Panel.Ai],
          rightBottom: [Panel.Presets],
        },
        activePanels: {
          leftTop: Panel.FolderTree,
          leftBottom: null,
          rightTop: Panel.Adjustments,
          rightBottom: Panel.Presets,
        },
      }),
      false,
    );
    expect(workspace.panelLayout.rightBottom).toEqual([Panel.Presets]);
    expect(workspace.panelLayout.leftTop).not.toContain(Panel.Presets);
    expect(workspace.activePanels.rightBottom).toBe(Panel.Presets);
  });

  it('leaves an already migrated layout alone, even with Presets on the right', () => {
    const workspace = reconcileWorkspace(savedWorkspace({ layoutVersion: WORKSPACE_LAYOUT_VERSION }), false);
    expect(workspace.panelLayout.rightTop).toEqual([
      Panel.Adjustments,
      Panel.Crop,
      Panel.Masks,
      Panel.Ai,
      Panel.Presets,
    ]);
    expect(workspace.panelLayout.leftTop).not.toContain(Panel.Presets);
  });

  it('adds Presets to the left sidebar when a saved layout lacks it', () => {
    const saved = savedWorkspace({});
    saved.panelLayout.rightTop = [Panel.Adjustments, Panel.Crop, Panel.Masks, Panel.Ai];
    const workspace = reconcileWorkspace(saved, false);
    expect(workspace.panelLayout.leftTop).toContain(Panel.Presets);
  });
});
