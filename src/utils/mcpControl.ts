import { invoke } from '@tauri-apps/api/core';
import { Invokes } from '../components/ui/AppProperties';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';

export interface McpControlStatus {
  available: boolean;
  enabled: boolean;
  port: number;
}
export const readMcpControl = () => invoke<McpControlStatus>(Invokes.McpControlStatus);

export async function setMcpControl(enabled: boolean): Promise<void> {
  const result = await invoke<McpControlStatus>(Invokes.SetMcpEnabled, { enabled });
  if (result.enabled !== enabled) throw new Error('AI control did not reach the requested state');
  const settings = useSettingsStore.getState();
  if (settings.appSettings) settings.setAppSettings({ ...settings.appSettings, mcpEnabled: enabled });
}

let pendingEnable: Promise<boolean> | null = null;
export async function ensureMcpControl(strings: { title: string; message: string; confirm: string }): Promise<boolean> {
  const status = await readMcpControl();
  if (!status.available || status.enabled) return true;
  if (!pendingEnable) {
    pendingEnable = (async () => {
      if (useUIStore.getState().confirmModalState.isOpen)
        throw new Error('Close the current dialog before starting an assistant');
      const accepted = await new Promise<boolean>((resolve) => {
        const unsubscribe = useUIStore.subscribe((state, previous) => {
          if (previous.confirmModalState.isOpen && !state.confirmModalState.isOpen) {
            unsubscribe();
            resolve(false);
          }
        });
        useUIStore.getState().setUI({
          confirmModalState: {
            isOpen: true,
            title: strings.title,
            message: strings.message,
            confirmText: strings.confirm,
            onConfirm: () => {
              unsubscribe();
              resolve(true);
            },
          },
        });
      });
      if (!accepted) return false;
      await setMcpControl(true);
      return true;
    })().finally(() => {
      pendingEnable = null;
    });
  }
  return pendingEnable;
}
