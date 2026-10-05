import type { TerminalSettings, TerminalTab } from '../components/ui/AppProperties';

export const TERMINAL_TAB_LIMIT = 16;
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function normalizeTerminalSettings(saved: TerminalSettings | undefined): Required<TerminalSettings> {
  const seen = new Set<string>();
  const tabs: TerminalTab[] = [];
  for (const tab of Array.isArray(saved?.tabs) ? saved.tabs : []) {
    if (tabs.length >= TERMINAL_TAB_LIMIT) break;
    if (!tab || !uuid.test(tab.id) || seen.has(tab.id) || typeof tab.path !== 'string' || !tab.path) continue;
    seen.add(tab.id);
    tabs.push({
      id: tab.id,
      path: tab.path,
      title: typeof tab.title === 'string' ? tab.title.slice(0, 80) : 'Terminal',
    });
  }
  return {
    shell: typeof saved?.shell === 'string' ? saved.shell : '',
    fontSize: Number.isFinite(saved?.fontSize) ? Math.min(32, Math.max(10, saved!.fontSize!)) : 14,
    fontFamily:
      typeof saved?.fontFamily === 'string' && saved.fontFamily.trim() ? saved.fontFamily.slice(0, 200) : 'monospace',
    startIn: saved?.startIn === 'external' ? 'external' : 'built-in',
    tabs,
    activeTab: tabs.some((tab) => tab.id === saved?.activeTab) ? saved!.activeTab! : (tabs[0]?.id ?? null),
  };
}

export function localTerminalDirectory(value: string): string | null {
  try {
    const url = new URL(value);
    if (url.protocol !== 'file:' || !['', 'localhost', '127.0.0.1', '[::1]'].includes(url.hostname)) return null;
    const path = decodeURIComponent(url.pathname);
    return path.startsWith('/') && !path.includes('\0') ? path : null;
  } catch {
    return null;
  }
}
