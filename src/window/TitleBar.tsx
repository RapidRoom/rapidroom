import { useCallback, useState, useEffect, useMemo } from 'react';
import { platform } from '@tauri-apps/plugin-os';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { useSettingsStore } from '../store/useSettingsStore';
import { visibleControls, WindowControlState } from './windowControls';
import { Minus, Square, X } from 'lucide-react';

const RestoreDownIcon = ({ size = 14, className = '' }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
  >
    <rect x="3" y="6" width="8" height="8" rx="1.5" />
    <path d="M6 6V4.5A1.5 1.5 0 0 1 7.5 3h5A1.5 1.5 0 0 1 14 4.5v5A1.5 1.5 0 0 1 12.5 11H11" />
  </svg>
);

export default function TitleBar() {
  const [osPlatform, setOsPlatform] = useState('');
  const [controlState, setControlState] = useState<WindowControlState | null>(null);
  const preference = useSettingsStore((s) => s.appSettings?.windowControls ?? 'auto');
  const isMaximized = !!(controlState?.maximized || controlState?.fullscreen);
  const controls = visibleControls(controlState, preference);
  const appWindow = useMemo(() => getCurrentWindow(), []);

  useEffect(() => {
    const getPlatform = async () => {
      try {
        const p = platform();
        setOsPlatform(p);
      } catch (error) {
        console.error('Failed to get platform:', error);
        setOsPlatform('windows');
      }
    };
    getPlatform();
  }, []);

  useEffect(() => {
    let disposed = false;
    let generation = 0;
    const unlisteners: (() => void)[] = [];
    const accept = (state: WindowControlState) => {
      if (!disposed) setControlState(state);
    };
    const refresh = async () => {
      const run = ++generation;
      try {
        const state = await invoke<WindowControlState>('get_window_control_state');
        if (run === generation) accept(state);
      } catch (error) {
        if (!disposed) console.error('Failed to read window state:', error);
      }
    };
    const add = (unlisten: () => void) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };
    const subscribe = async () => {
      try {
        add(
          await appWindow.listen<WindowControlState>('window-control-state', ({ payload }) => {
            ++generation;
            accept(payload);
          }),
        );
        if (disposed) return;
        add(await appWindow.onResized(() => void refresh()));
        if (!disposed) await refresh();
      } catch (error) {
        if (!disposed) console.error('Failed to subscribe to window state:', error);
      }
    };
    void subscribe();
    return () => {
      disposed = true;
      ++generation;
      unlisteners.forEach((u) => u());
    };
  }, [appWindow]);

  const handleMinimize = async () => {
    try {
      await appWindow.minimize();
    } catch (error) {
      console.error('Failed to minimize window:', error);
    }
  };
  const handleClose = async () => {
    try {
      await appWindow.close();
    } catch (error) {
      console.error('Failed to close window:', error);
    }
  };

  const handleMaximize = useCallback(async () => {
    try {
      const state = await invoke<WindowControlState>('get_window_control_state');
      if (osPlatform === 'macos') await appWindow.setFullscreen(!state.fullscreen);
      else if (state.fullscreen) await appWindow.setFullscreen(false);
      else if (state.maximized) await appWindow.unmaximize();
      else await appWindow.maximize();
    } catch (error) {
      console.error('Failed to toggle maximize:', error);
    }
  }, [osPlatform, appWindow]);

  const isMac = osPlatform === 'macos';
  const isLinux = osPlatform === 'linux';
  const isWindows = osPlatform === 'windows';
  const isMobile = osPlatform === 'android';
  if (!osPlatform || isMobile) {
    return null;
  }
  const outerDragProps = isLinux ? {} : { 'data-tauri-drag-region': 'true' };

  return (
    <div className="relative pt-2 px-2 w-full z-50 bg-transparent" {...outerDragProps}>
      <div
        className="h-10 bg-bg-secondary flex justify-between items-center select-none rounded-lg overflow-hidden"
        {...outerDragProps}
      >
        <div className="flex items-center h-full">
          {isMac && (
            <div className="flex items-center h-full px-4 space-x-2 z-10">
              <button
                aria-label="Close window"
                className="w-3 h-3 rounded-full bg-red-500 hover:bg-red-600 transition-colors duration-150"
                onClick={handleClose}
              />
              <button
                aria-label="Minimize window"
                className="w-3 h-3 rounded-full bg-yellow-500 hover:bg-yellow-600 transition-colors duration-150"
                onClick={handleMinimize}
              />
              <button
                aria-label="Maximize window"
                className="w-3 h-3 rounded-full bg-green-500 hover:bg-green-600 transition-colors duration-150"
                onClick={handleMaximize}
              />
            </div>
          )}
          <div data-tauri-drag-region className={`flex items-center h-full ${isMac ? '' : 'px-4'}`}>
            <p className="text-sm font-semibold text-text-secondary pointer-events-none">RapidRoom</p>
          </div>
        </div>
        <div data-tauri-drag-region className="flex-1 h-full" />
        <div className="flex items-center h-full z-10">
          {isLinux && (
            <div className="flex items-center gap-2 pr-2 h-full">
              {controls.minimize && (
                <button
                  aria-label="Minimize window"
                  className="w-7 h-7 rounded-full inline-flex justify-center items-center hover:bg-white/10 transition-colors duration-150"
                  onClick={handleMinimize}
                >
                  <Minus size={16} className="text-text-secondary" />
                </button>
              )}
              {controls.maximize && (
                <button
                  aria-label={isMaximized ? 'Restore window' : 'Maximize window'}
                  className="w-7 h-7 rounded-full inline-flex justify-center items-center hover:bg-white/10 transition-colors duration-150"
                  onClick={handleMaximize}
                >
                  {isMaximized ? (
                    <RestoreDownIcon size={13} className="text-text-secondary" />
                  ) : (
                    <Square size={13} className="text-text-secondary" />
                  )}
                </button>
              )}
              <button
                aria-label="Close window"
                className="w-7 h-7 rounded-full inline-flex justify-center items-center hover:bg-red-500 hover:text-white transition-colors duration-150"
                onClick={handleClose}
              >
                <X size={16} className="text-text-secondary hover:text-white" />
              </button>
            </div>
          )}

          {isWindows && <div data-tauri-drag-region className="w-36 shrink-0 h-full pointer-events-none" />}
        </div>
      </div>

      {isWindows && (
        <div className="absolute top-0 right-0 flex h-12 z-20">
          <button aria-label="Minimize window" className="relative w-12 group outline-none" onClick={handleMinimize}>
            <div className="absolute bottom-0 left-0 w-12 h-10 flex justify-center items-center group-hover:bg-white/10 group-active:bg-white/20 transition-colors duration-150">
              <Minus size={16} className="text-text-secondary" />
            </div>
          </button>

          <button aria-label="Maximize window" className="relative w-12 group outline-none" onClick={handleMaximize}>
            <div className="absolute bottom-0 left-0 w-12 h-10 flex justify-center items-center group-hover:bg-white/10 group-active:bg-white/20 transition-colors duration-150">
              {isMaximized ? (
                <RestoreDownIcon size={12} className="text-text-secondary" />
              ) : (
                <Square size={12} className="text-text-secondary" />
              )}
            </div>
          </button>

          <button aria-label="Close window" className="relative w-14 group outline-none" onClick={handleClose}>
            <div className="absolute bottom-0 left-0 w-12 h-10 flex justify-center items-center group-hover:bg-red-500 group-active:bg-red-600 transition-colors duration-150 rounded-r-lg">
              <X size={16} className="text-text-secondary group-hover:text-white transition-colors duration-150" />
            </div>
          </button>
        </div>
      )}
    </div>
  );
}
