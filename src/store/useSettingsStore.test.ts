import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke, mockCommand } from '../test/tauriMock';
import { useSettingsStore } from './useSettingsStore';
import { AppSettings, Invokes } from '../components/ui/AppProperties';

const initialState = useSettingsStore.getState();

afterEach(() => {
  useSettingsStore.setState(initialState, true);
  vi.restoreAllMocks();
});

describe('useSettingsStore.handleSettingsChange', () => {
  it('saves settings through the backend without the transient search criteria', async () => {
    const saved: unknown[] = [];
    mockCommand(Invokes.SaveSettings, (args) => {
      saved.push(args?.settings);
    });
    const settings = {
      lastRootPath: '/photos',
      theme: 'light',
      searchCriteria: { tags: ['beach'], text: '', mode: 'OR' },
    } as unknown as AppSettings;

    await useSettingsStore.getState().handleSettingsChange(settings);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(saved).toEqual([{ lastRootPath: '/photos', theme: 'light' }]);
    expect(useSettingsStore.getState().appSettings).toBe(settings);
    expect(useSettingsStore.getState().theme).toBe('light');
  });

  it('keeps the new settings in the UI and does not throw when saving fails', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    mockCommand(Invokes.SaveSettings, () => {
      throw new Error('disk full');
    });
    const settings = { lastRootPath: '/photos' } as unknown as AppSettings;

    await expect(useSettingsStore.getState().handleSettingsChange(settings)).resolves.toBeUndefined();

    expect(invoke).toHaveBeenCalledWith(Invokes.SaveSettings, { settings: { lastRootPath: '/photos' } });
    expect(useSettingsStore.getState().appSettings).toBe(settings);
    expect(consoleError).toHaveBeenCalledWith('Failed to save settings:', expect.any(Error));
  });
});
