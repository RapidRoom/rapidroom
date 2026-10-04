interface FullscreenWindow {
  isFullscreen(): Promise<boolean>;
  setFullscreen(fullscreen: boolean): Promise<void>;
}

// Serialize native transitions. A rapid Black -> Normal -> Black sequence must
// finish any restore before recording the next session's original window state.
export function createLightsOutFullscreenController(window: FullscreenWindow) {
  let requestedBlack = false;
  let originalFullscreen: boolean | null = null;
  let pending: Promise<void> = Promise.resolve();

  return {
    setBlack(black: boolean): Promise<void> {
      requestedBlack = black;
      pending = pending
        .catch(() => {})
        .then(async () => {
          if (requestedBlack) {
            if (originalFullscreen !== null) return;
            const original = await window.isFullscreen();
            if (!requestedBlack) return;
            originalFullscreen = original;
            if (!original) {
              try {
                await window.setFullscreen(true);
              } catch (error) {
                originalFullscreen = null;
                throw error;
              }
            }
          } else {
            if (originalFullscreen === null) return;
            if (!originalFullscreen && (await window.isFullscreen())) {
              await window.setFullscreen(false);
            }
            originalFullscreen = null;
          }
        });
      return pending;
    },
  };
}
