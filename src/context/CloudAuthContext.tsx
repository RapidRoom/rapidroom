import { createContext, ReactNode, useContext } from 'react';

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

export function CloudAuthProvider({ children }: { children: ReactNode }) {
  return <>{children}</>;
}
