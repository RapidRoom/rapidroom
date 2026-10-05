import { describe, expect, it } from 'vitest';
import { localTerminalDirectory, normalizeTerminalSettings } from './terminalSettings';

describe('terminal metadata', () => {
  it('restores only bounded valid unique tabs and safe font defaults', () => {
    const tab = { id: '2ba4c8c9-1d18-44f6-9707-53ad19b5a3a0', title: 'Photos', path: '/photos' };
    const result = normalizeTerminalSettings({
      tabs: [tab, tab, { ...tab, id: 'invalid' }],
      activeTab: 'missing',
      fontSize: Infinity,
    });
    expect(result.tabs).toEqual([tab]);
    expect(result.activeTab).toBe(tab.id);
    expect(result.fontSize).toBe(14);
    expect(normalizeTerminalSettings({ fontSize: 100 }).fontSize).toBe(32);
    expect(normalizeTerminalSettings({ tabs: [] }).activeTab).toBeNull();
    expect(Object.keys(result)).not.toContain('transcript');
  });

  it('accepts only local file-directory notifications and decodes folder names', () => {
    expect(localTerminalDirectory('file://localhost/photos/My%20RAWs')).toBe('/photos/My RAWs');
    for (const value of [
      'file://example.com/photos',
      'https://localhost/photos',
      'file:///photos/%00',
      'file:///photos/%ZZ',
      'malformed',
    ]) {
      expect(localTerminalDirectory(value)).toBeNull();
    }
  });
});
