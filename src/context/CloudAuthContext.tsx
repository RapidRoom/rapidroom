import { createContext, ReactNode, useContext, useEffect, useMemo } from 'react';
import { ClerkProvider } from '@clerk/react';
import { useSettingsStore } from '../store/useSettingsStore';
import { useCloudStore } from '../store/useCloudStore';

interface CloudAuth {
  isSignedIn: boolean;
  isPro: boolean;
  getToken: () => Promise<string | null>;
}

const signedOut: CloudAuth = { isSignedIn: false, isPro: false, getToken: async () => null };
const CloudAuthContext = createContext<CloudAuth>(signedOut);
export const useCloudAuth = () => useContext(CloudAuthContext);

export function CloudAuthProvider({ children }: { children: ReactNode }) {
  const provider = useSettingsStore((s) => s.appSettings?.aiProvider);
  const persistedProvider = useSettingsStore((s) => s.persistedAiProvider);
  const enabled = provider === 'cloud' && persistedProvider === 'cloud';
  const clerk = useCloudStore((s) => s.clerk);
  const user = useCloudStore((s) => s.user);
  const initAuth = useCloudStore((s) => s.initAuth);
  const getToken = useCloudStore((s) => s.getToken);

  useEffect(() => {
    if (enabled) void initAuth();
    else useCloudStore.setState({ user: null, cloudUsage: null });
  }, [enabled, initAuth]);

  const auth = useMemo(
    () => (enabled ? { isSignedIn: !!user, isPro: user?.publicMetadata?.plan === 'pro', getToken } : signedOut),
    [enabled, user, getToken],
  );
  const content = <CloudAuthContext.Provider value={auth}>{children}</CloudAuthContext.Provider>;
  if (!enabled || !clerk) return content;
  return (
    <ClerkProvider
      Clerk={clerk}
      publishableKey={clerk.publishableKey}
      telemetry={false}
      routerPush={() => {}}
      routerReplace={() => {}}
    >
      {content}
    </ClerkProvider>
  );
}
