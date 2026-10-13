export interface WindowControlState {
  canMinimize: boolean;
  canMaximize: boolean;
  maximized: boolean;
  fullscreen: boolean;
}

export function visibleControls(state: WindowControlState | null, preference: string) {
  if (preference === 'hide') return { minimize: false, maximize: false };
  if (preference === 'always') return { minimize: true, maximize: true };
  return { minimize: state?.canMinimize ?? false, maximize: state?.canMaximize ?? false };
}
