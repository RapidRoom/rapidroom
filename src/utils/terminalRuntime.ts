import { Channel, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { Invokes, type TerminalTab } from '../components/ui/AppProperties';
import { useSettingsStore } from '../store/useSettingsStore';
import { useTerminalStore } from '../store/useTerminalStore';
import { localTerminalDirectory, normalizeTerminalSettings } from './terminalSettings';

type TerminalEvent = { event: 'output'; sequence: number; data: number[] } | { event: 'exit'; code: number | null };
interface Runtime {
  terminal: Terminal;
  fit: FitAddon;
  node: HTMLDivElement;
  opening: Promise<unknown>;
  input: Promise<void>;
  closed: boolean;
  attachment: string;
  observer: ResizeObserver | null;
  dimensions: string;
}

// Keep the parser and PTY subscription alive when React moves or hides a panel.
// Replaying a byte ring on every mount would duplicate the screen and escape sequences.
const runtimes = new Map<string, Runtime>();
const encoder = new TextEncoder();

function fail(id: string, error: unknown): void {
  useTerminalStore.getState().setError(id, String(error));
}

function createRuntime(tab: TerminalTab): Runtime {
  const preferences = normalizeTerminalSettings(useSettingsStore.getState().appSettings?.terminalSettings);
  const terminal = new Terminal({
    fontSize: preferences.fontSize,
    fontFamily: preferences.fontFamily,
    scrollback: 2000,
    allowProposedApi: false,
    theme: { background: '#181818', foreground: '#ededed' },
    linkHandler: {
      activate: (_event, value) => {
        try {
          const url = new URL(value);
          if (!['http:', 'https:'].includes(url.protocol)) return;
          if (window.confirm(`Open this link in your browser?\n${url.href}`))
            void open(url.href).catch((error) => fail(tab.id, error));
        } catch {
          /* Treat malformed terminal links as plain text. */
        }
      },
    },
  });
  // Never let shell output read or replace the desktop clipboard through OSC 52.
  terminal.parser.registerOscHandler(52, () => true);
  terminal.parser.registerOscHandler(7, (value) => {
    const path = localTerminalDirectory(value);
    if (path) useTerminalStore.getState().updateDirectory(tab.id, path);
    return true;
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  const node = document.createElement('div');
  node.className = 'h-full w-full min-h-0';
  const runtime: Runtime = {
    terminal,
    fit,
    node,
    opening: Promise.resolve(),
    input: Promise.resolve(),
    closed: false,
    attachment: crypto.randomUUID(),
    observer: null,
    dimensions: '',
  };
  const channel = new Channel<TerminalEvent>();
  channel.onmessage = (event) => {
    if (runtime.closed) return;
    if (event.event === 'exit') {
      terminal.options.disableStdin = true;
      terminal.write(`\r\n[Shell exited${event.code === null ? '' : `: ${event.code}`}]\r\n`);
      return;
    }
    terminal.write(new Uint8Array(event.data), () => {
      if (!runtime.closed)
        void invoke(Invokes.PtyAck, { id: tab.id, attachment: runtime.attachment, sequence: event.sequence }).catch(
          (error) => fail(tab.id, error),
        );
    });
  };
  runtime.opening = invoke(Invokes.PtyOpen, {
    request: { id: tab.id, path: tab.path, cols: 80, rows: 24, attachment: runtime.attachment },
    onEvent: channel,
  }).catch((error) => {
    terminal.options.disableStdin = true;
    fail(tab.id, error);
    throw error;
  });
  // Handle creation failures even if a user closes the panel before entering input.
  void runtime.opening.catch(() => undefined);
  terminal.onData((data) => {
    void writeTerminal(tab.id, data).catch((error) => fail(tab.id, error));
  });
  runtimes.set(tab.id, runtime);
  return runtime;
}

export function mountTerminal(tab: TerminalTab, host: HTMLElement): () => void {
  const runtime = runtimes.get(tab.id) ?? createRuntime(tab);
  const preferences = normalizeTerminalSettings(useSettingsStore.getState().appSettings?.terminalSettings);
  runtime.terminal.options.fontSize = preferences.fontSize;
  runtime.terminal.options.fontFamily = preferences.fontFamily;
  host.append(runtime.node);
  if (!runtime.terminal.element) runtime.terminal.open(runtime.node);
  const fit = () => {
    if (runtime.closed || !host.isConnected || host.clientWidth < 20 || host.clientHeight < 20) return;
    runtime.fit.fit();
    const dimensions = `${runtime.terminal.cols}:${runtime.terminal.rows}`;
    if (dimensions === runtime.dimensions) return;
    runtime.dimensions = dimensions;
    void runtime.opening
      .then(() => {
        if (!runtime.closed)
          return invoke(Invokes.PtyResize, { id: tab.id, cols: runtime.terminal.cols, rows: runtime.terminal.rows });
      })
      .catch((error) => fail(tab.id, error));
  };
  runtime.observer?.disconnect();
  runtime.observer = new ResizeObserver(fit);
  runtime.observer.observe(host);
  fit();
  runtime.terminal.focus();
  return () => {
    runtime.observer?.disconnect();
    runtime.observer = null;
    runtime.node.remove();
  };
}

export function writeTerminal(id: string, text: string): Promise<void> {
  const tab = useTerminalStore.getState().tabs.find((candidate) => candidate.id === id);
  if (!tab) return Promise.reject(new Error('Terminal tab is closed'));
  const runtime = runtimes.get(id) ?? createRuntime(tab);
  const data = encoder.encode(text);
  const write = runtime.input.then(async () => {
    await runtime.opening;
    if (runtime.closed) throw new Error('Terminal tab is closed');
    for (let offset = 0; offset < data.length; offset += 65536) {
      await invoke(Invokes.PtyWrite, { id, data: Array.from(data.subarray(offset, offset + 65536)) });
    }
  });
  runtime.input = write.catch(() => undefined);
  return write;
}

export async function closeTerminal(id: string): Promise<void> {
  const runtime = runtimes.get(id);
  if (runtime) {
    runtime.closed = true;
    runtime.observer?.disconnect();
    // The backend also rejects delayed opens for closed IDs.
    await runtime.opening.catch(() => undefined);
  }
  try {
    await invoke(Invokes.PtyClose, { id });
  } finally {
    runtime?.terminal.dispose();
    runtime?.node.remove();
    runtimes.delete(id);
    useTerminalStore.getState().removeTab(id);
  }
}
