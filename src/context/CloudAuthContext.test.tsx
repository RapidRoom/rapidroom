// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mockCommand } from '../test/tauriMock';
import { CloudAuthProvider, useCloudAuth } from './CloudAuthContext';
import { useSettingsStore } from '../store/useSettingsStore';
import { useCloudStore } from '../store/useCloudStore';
import { setMcpControl } from '../utils/mcpControl';
import { AppSettings, Invokes } from '../components/ui/AppProperties';

const mocks = vi.hoisted(() => ({ init: vi.fn(), provider: vi.fn(), token: vi.fn() }));
vi.mock('tauri-plugin-clerk', () => ({ initClerk: mocks.init }));
vi.mock('@clerk/react', () => ({
  ClerkProvider: (props: { children: unknown; telemetry: boolean }) => {
    mocks.provider(props);
    return props.children;
  },
}));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'linux' }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const settingsInitial = useSettingsStore.getState();
const cloudInitial = useCloudStore.getState();
let root: ReturnType<typeof createRoot> | undefined;
let auth!: ReturnType<typeof useCloudAuth>;
const settings = (aiProvider?: string) => ({ aiProvider }) as AppSettings;
function Child() {
  auth = useCloudAuth();
  return null;
}
async function mount() {
  root = createRoot(document.createElement('div'));
  await act(async () => root!.render(createElement(CloudAuthProvider, null, createElement(Child))));
}
beforeEach(() => {
  mocks.init.mockReset();
  mocks.provider.mockClear();
  mocks.token.mockReset();
  mocks.token.mockResolvedValue('token');
  mocks.init.mockResolvedValue({
    publishableKey: 'test-key',
    user: { publicMetadata: { plan: 'pro' } },
    addListener: vi.fn(),
    session: { getToken: mocks.token },
  });
});
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = undefined;
  useSettingsStore.setState(settingsInitial, true);
  useCloudStore.setState(cloudInitial, true);
  vi.restoreAllMocks();
});

describe('persisted cloud opt-in', () => {
  it.each([undefined, 'cpu', 'ai-free', 'ai-connector'])(
    'keeps signed-out defaults and never initializes or mounts Clerk for %s',
    async (provider) => {
      useSettingsStore.getState().setAppSettings(settings(provider));
      await mount();
      await useCloudStore.getState().initAuth();
      await useCloudStore.getState().fetchUsage();
      expect(mocks.init).not.toHaveBeenCalled();
      expect(mocks.provider).not.toHaveBeenCalled();
      expect(auth.isSignedIn).toBe(false);
      expect(await auth.getToken()).toBeNull();
    },
  );

  it('does not load Clerk before preferences arrive', async () => {
    await mount();
    expect(mocks.init).not.toHaveBeenCalled();
    expect(mocks.provider).not.toHaveBeenCalled();
  });

  it('initializes once for saved cloud, disables telemetry, and suppresses auth in AI-free mode', async () => {
    useSettingsStore.getState().setAppSettings(settings('cloud'));
    await mount();
    expect(mocks.init).toHaveBeenCalledTimes(1);
    expect(mocks.init).toHaveBeenCalledWith(expect.objectContaining({ telemetry: false }));
    expect(mocks.provider).toHaveBeenCalledWith(expect.objectContaining({ telemetry: false }));
    expect(auth.isSignedIn).toBe(true);
    expect(await auth.getToken()).toBe('token');
    mocks.provider.mockClear();
    await act(async () => useSettingsStore.getState().setAppSettings(settings('ai-free')));
    expect(mocks.provider).not.toHaveBeenCalled();
    expect(auth.isSignedIn).toBe(false);
    expect(await useCloudStore.getState().getToken()).toBeNull();
    await act(async () => useSettingsStore.getState().setAppSettings(settings('cloud')));
    expect(mocks.init).toHaveBeenCalledTimes(1);
    expect(auth.isSignedIn).toBe(true);
  });

  it('waits for a successful preference save before initializing cloud', async () => {
    useSettingsStore.getState().setAppSettings(settings('cpu'));
    let finish!: () => void;
    mockCommand(
      Invokes.SaveSettings,
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    await mount();
    let saving!: Promise<void>;
    await act(async () => {
      saving = useSettingsStore.getState().handleSettingsChange(settings('cloud'));
    });
    expect(mocks.init).not.toHaveBeenCalled();
    await act(async () => {
      finish();
      await saving;
    });
    expect(mocks.init).toHaveBeenCalledTimes(1);
  });

  it('never initializes cloud when saving the opt-in fails', async () => {
    useSettingsStore.getState().setAppSettings(settings('cpu'));
    mockCommand(Invokes.SaveSettings, () => {
      throw new Error('disk full');
    });
    vi.spyOn(console, 'error').mockImplementation(() => {});
    await mount();
    await act(async () => useSettingsStore.getState().handleSettingsChange(settings('cloud')));
    expect(mocks.init).not.toHaveBeenCalled();
    expect(mocks.provider).not.toHaveBeenCalled();
    expect(auth.isSignedIn).toBe(false);
  });
  it('does not treat an unrelated MCP setting save as a persisted cloud opt-in', async () => {
    useSettingsStore.getState().setAppSettings(settings('cpu'));
    mockCommand(Invokes.SaveSettings, () => Promise.reject(new Error('disk full')));
    mockCommand(Invokes.SetMcpEnabled, () => ({ available: true, enabled: true, port: 7790 }));
    vi.spyOn(console, 'error').mockImplementation(() => {});
    await mount();
    await act(async () => {
      await useSettingsStore.getState().handleSettingsChange(settings('cloud'));
      await setMcpControl(true);
    });
    expect(useSettingsStore.getState().persistedAiProvider).toBe('cpu');
    expect(mocks.init).not.toHaveBeenCalled();
    expect(mocks.provider).not.toHaveBeenCalled();
  });
  it('discards a pending token after cloud is disabled', async () => {
    useSettingsStore.getState().setAppSettings(settings('cloud'));
    let finish!: (token: string) => void;
    mocks.token.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          finish = resolve;
        }),
    );
    await mount();
    const token = useCloudStore.getState().getToken();
    await act(async () => useSettingsStore.getState().setAppSettings(settings('ai-free')));
    finish('late-token');
    expect(await token).toBeNull();
  });
});
