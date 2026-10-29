import { defineConfig } from 'i18next-cli';
import { overlayExtraction } from './rapidroom/i18n-extraction.mjs';

const locales = ['en', 'de', 'pl', 'zh-CN', 'zh-TW', 'es', 'fr', 'it', 'pt', 'ja', 'ko', 'ru', 'uk', 'ca', 'nl', 'cs'];
const overlays = overlayExtraction(locales);

export default defineConfig({
  locales,
  plugins: [overlays.plugin],
  extract: {
    input: ['src/**/*.{ts,tsx}'],
    output: overlays.output,
    mergeNamespaces: true,
    defaultNS: false,
    removeUnusedKeys: false,
    sort: true,
    defaultValue: '',
  },
});
