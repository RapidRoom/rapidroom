// @vitest-environment jsdom
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import DenoiseModal from './DenoiseModal';
import { useSettingsStore } from '../../store/useSettingsStore';
import { AppSettings, Invokes } from '../ui/AppProperties';
import { mockCommand } from '../../test/tauriMock';

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const initial = useSettingsStore.getState();
let root: ReturnType<typeof createRoot> | undefined;
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = undefined;
  useSettingsStore.setState(initial, true);
});

describe('AI-free denoise controls', () => {
  it.each([false, true])('keeps non-AI denoise when Fewer methods is clicked (RAW=%s)', async (isRaw) => {
    useSettingsStore.getState().setAppSettings({ aiProvider: 'ai-free' } as AppSettings);
    mockCommand(Invokes.IsRaw9Available, () => false);
    const onDenoise = vi.fn();
    const host = document.createElement('div');
    root = createRoot(host);
    await act(async () =>
      root!.render(
        createElement(DenoiseModal, {
          isOpen: true,
          onClose: vi.fn(),
          onDenoise,
          onBatchDenoise: vi.fn(),
          onSave: vi.fn(),
          onOpenFile: vi.fn(),
          error: null,
          previewBase64: null,
          originalBase64: null,
          isProcessing: false,
          progressMessage: null,
          aiModelDownloadStatus: null,
          isRaw,
          targetPaths: ['/photo.raw'],
        }),
      ),
    );
    const buttons = [...host.querySelectorAll('button')];
    const fewer = buttons.find((b) => b.textContent === 'modals.denoise.fewerMethods');
    expect(fewer).toBeDefined();
    expect(fewer!.disabled).toBe(true);
    await act(async () => fewer!.click());
    const start = buttons.find((b) => b.textContent === 'modals.denoise.btnStart');
    expect(start).toBeDefined();
    await act(async () => start!.click());
    expect(onDenoise).toHaveBeenCalledOnce();
    expect(onDenoise.mock.calls[0][1]).toBe('bm3d');
  });
});
