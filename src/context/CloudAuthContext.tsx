import { createContext, ReactNode, useContext, useMemo, useState } from 'react';
import { ClerkProvider, useAuth, useUser } from '@clerk/react';
import { useSettingsStore } from '../store/useSettingsStore';

const CLERK_PUBLISHABLE_KEY = 'pk_test_YnJpZWYtc2Vhc25haWwtMTIuY2xlcmsuYWNjb3VudHMuZGV2JA'; // local dev key

interface CloudAuth {
  isSignedIn: boolean;
  isPro: boolean;
  getToken: () => Promise<string | null>;
}

const CloudAuthContext = createContext<CloudAuth>({
  isSignedIn: false,
  isPro: false,
  getToken: async () => null,
});

export const useCloudAuth = () => useContext(CloudAuthContext);

function ClerkAuthBridge({ children }: { children: ReactNode }) {
  const { isSignedIn, getToken } = useAuth();
  const { user } = useUser();
  const isPro = user?.publicMetadata?.plan === 'pro';
  const value = useMemo(() => ({ isSignedIn: !!isSignedIn, isPro, getToken }), [isSignedIn, isPro, getToken]);

  return <CloudAuthContext.Provider value={value}>{children}</CloudAuthContext.Provider>;
}

export function CloudAuthProvider({ children }: { children: ReactNode }) {
  const isCloud = useSettingsStore((s) => s.appSettings?.aiProvider === 'cloud');
  const [enabled, setEnabled] = useState(isCloud);
  if (isCloud && !enabled) setEnabled(true);

  if (!enabled) return <>{children}</>;

  return (
    <ClerkProvider
      publishableKey={CLERK_PUBLISHABLE_KEY}
      routerPush={() => {}}
      routerReplace={() => {}}
      telemetry={false}
    >
      <ClerkAuthBridge>{children}</ClerkAuthBridge>
    </ClerkProvider>
  );
}
