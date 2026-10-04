import { describe, expect, it } from 'vitest';
import tauriConf from '../../src-tauri/tauri.conf.json';
import { RAPIDRAW_BASE_VERSION, RAPIDROOM_VERSION } from './rapidroom';

describe('RapidRoom version', () => {
  it('is the app version in tauri.conf.json', () => {
    expect(tauriConf.version).toBe(RAPIDROOM_VERSION);
  });

  it('is plain MAJOR.MINOR.PATCH, which the update check compares', () => {
    expect(RAPIDROOM_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
    expect(RAPIDRAW_BASE_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
