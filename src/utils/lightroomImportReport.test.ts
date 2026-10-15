import { describe, expect, it } from 'vitest';
import { createInstance } from 'i18next';
import en from '../i18n/locales/en.json';
import { describeLightroomSettings, type LightroomSettingReport } from './lightroomImportReport';

describe('Lightroom setting report', () => {
  it('shows counts, individual fields, honest reasons and bounded omissions', async () => {
    const i18n = createInstance();
    await i18n.init({ lng: 'en', resources: { en: { translation: en } }, interpolation: { escapeValue: false } });
    const report: LightroomSettingReport = {
      mapped: 1,
      approximated: 1,
      ignored: 1,
      omitted: 3,
      settings: [
        { field: 'HasCrop', outcome: 'mapped', reason: 'disabled' },
        { field: 'Sharpness', outcome: 'approximated', reason: 'sharpnessScale' },
        { field: 'SharpenRadius', outcome: 'ignored', reason: 'sharpenCompanion' },
      ],
    };
    const text = describeLightroomSettings(report, i18n.t);
    expect(text).toContain('1 mapped, 1 approximated, 1 ignored');
    expect(text).toContain('Sharpness — approximated: Amount is scaled from 0-150 to 0-100');
    expect(text).toContain('SharpenRadius — ignored: Lightroom sharpening radius/detail');
    expect(text).toContain('3 additional settings omitted');
    expect(text.split('\n')).toHaveLength(5);
  });
});
