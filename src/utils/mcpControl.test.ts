import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../test/tauriMock';
import { Invokes, type AppSettings } from '../components/ui/AppProperties';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';
import { ensureMcpControl } from './mcpControl';

const settings = useSettingsStore.getState();
const ui = useUIStore.getState();
const strings = { title: 'Enable control', message: 'Local only; fresh key', confirm: 'Enable and start' };
afterEach(() => {
  useSettingsStore.setState(settings, true);
  useUIStore.setState(ui, true);
  vi.restoreAllMocks();
});
const disabled = () => mockCommand(Invokes.McpControlStatus, () => ({ available: true, enabled: false, port: 0 }));

describe('assistant launch consent', () => {
  it('does not prompt when unavailable or already enabled', async () => {
    mockCommand(Invokes.McpControlStatus, () => ({ available: false, enabled: false, port: 0 }));
    expect(await ensureMcpControl(strings)).toBe(true);
    mockCommand(Invokes.McpControlStatus, () => ({ available: true, enabled: true, port: 7790 }));
    expect(await ensureMcpControl(strings)).toBe(true);
    expect(useUIStore.getState().confirmModalState.isOpen).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith(Invokes.SetMcpEnabled, expect.anything());
  });
  it('cancel leaves control disabled and does not permit a launch', async () => {
    disabled();
    const result = ensureMcpControl(strings);
    await vi.waitFor(() => expect(useUIStore.getState().confirmModalState.isOpen).toBe(true));
    useUIStore.getState().setUI({ confirmModalState: { isOpen: false } });
    expect(await result).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith(Invokes.SetMcpEnabled, expect.anything());
  });
  it('shares one confirmation/start and preserves other preferences', async () => {
    disabled();
    useSettingsStore.getState().setAppSettings({ theme: 'light', mcpEnabled: false } as AppSettings);
    mockCommand(Invokes.SetMcpEnabled, () => ({ available: true, enabled: true, port: 7790 }));
    const a = ensureMcpControl(strings),
      b = ensureMcpControl(strings);
    await vi.waitFor(() => expect(useUIStore.getState().confirmModalState.isOpen).toBe(true));
    expect(useUIStore.getState().confirmModalState.message).toBe(strings.message);
    useUIStore.getState().confirmModalState.onConfirm?.();
    useUIStore.getState().setUI({ confirmModalState: { isOpen: false } });
    expect(await Promise.all([a, b])).toEqual([true, true]);
    expect(invoke.mock.calls.filter(([command]) => command === Invokes.SetMcpEnabled)).toHaveLength(1);
    expect(useSettingsStore.getState().appSettings).toEqual({ theme: 'light', mcpEnabled: true });
  });
  it('a failed backend start does not enable preferences or permit launch', async () => {
    disabled();
    useSettingsStore.getState().setAppSettings({ theme: 'light', mcpEnabled: false } as AppSettings);
    mockCommand(Invokes.SetMcpEnabled, () => {
      throw new Error('Port occupied');
    });
    const result = ensureMcpControl(strings);
    await vi.waitFor(() => expect(useUIStore.getState().confirmModalState.isOpen).toBe(true));
    useUIStore.getState().confirmModalState.onConfirm?.();
    useUIStore.getState().setUI({ confirmModalState: { isOpen: false } });
    await expect(result).rejects.toThrow('Port occupied');
    expect(useSettingsStore.getState().appSettings?.mcpEnabled).toBe(false);
  });
});
