import { readFileSync, readdirSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import i18n from 'i18next';
import './index';
import { deepMerge } from './rapidroom.mjs';

const localeDir = new URL('./locales/', import.meta.url);
const languages = readdirSync(localeDir).filter((name) => name.endsWith('.json'));

describe('the catalogs loaded by the app', () => {
  it('loads every upstream language with its complete RapidRoom overlay', () => {
    expect(languages).toHaveLength(16);
    expect(Object.keys(i18n.options.resources ?? {}).sort()).toEqual(languages.map((name) => name.slice(0, -5)).sort());
    for (const filename of languages) {
      const upstream = JSON.parse(readFileSync(new URL(filename, localeDir), 'utf8'));
      const overlay = JSON.parse(readFileSync(new URL(`rapidroom/${filename}`, localeDir), 'utf8'));
      const before = structuredClone(upstream);
      expect(i18n.getResourceBundle(filename.slice(0, -5), 'translation')).toEqual(deepMerge(upstream, overlay));
      expect(upstream).toEqual(before);
    }
  });

  it('keeps the RapidRoom Viewer override and inherited sibling strings', () => {
    const upstream = JSON.parse(readFileSync(new URL('en.json', localeDir), 'utf8'));
    expect(i18n.t('editor.viewer.windowTitle', { lng: 'en' })).toBe('RapidRoom Viewer');
    expect(i18n.t('editor.viewer.openFailed', { lng: 'en' })).toBe(upstream.editor.viewer.openFailed);
  });
});
