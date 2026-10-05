// @vitest-environment jsdom
import { createElement } from 'react';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { describe, expect, it } from 'vitest';
import { useExportSettings } from './useExportSettings';
import { ExportPreset, SharpenAmount, SharpenTarget } from '../components/ui/ExportImportProperties';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

async function mountSettings() {
  const result: { current: ReturnType<typeof useExportSettings> | null } = { current: null };
  function Harness() {
    result.current = useExportSettings();
    return null;
  }
  const root = createRoot(document.createElement('div'));
  await act(async () => {
    root.render(createElement(Harness));
  });
  return result as { current: ReturnType<typeof useExportSettings> };
}

describe('useExportSettings output sharpening', () => {
  it('is off by default', async () => {
    const settings = await mountSettings();
    expect(settings.current.outputSharpening).toBeNull();
    expect(settings.current.currentSettingsObject.outputSharpening).toBeNull();
  });

  it('round-trips through a preset, and older presets turn it off', async () => {
    const settings = await mountSettings();
    const sharpening = { target: SharpenTarget.Print, amount: SharpenAmount.High };
    await act(async () => {
      settings.current.setOutputSharpening(sharpening);
    });
    const saved = { ...settings.current.currentSettingsObject, id: 'p', name: 'P' } as ExportPreset;
    expect(saved.outputSharpening).toEqual(sharpening);

    const { outputSharpening: _omitted, ...older } = saved;
    await act(async () => {
      settings.current.handleApplyPreset(older as ExportPreset);
    });
    expect(settings.current.outputSharpening).toBeNull();

    await act(async () => {
      settings.current.handleApplyPreset(saved);
    });
    expect(settings.current.outputSharpening).toEqual(sharpening);
  });
});
