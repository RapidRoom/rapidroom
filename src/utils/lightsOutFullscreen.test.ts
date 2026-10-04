import { describe, expect, it, vi } from 'vitest';
import { createLightsOutFullscreenController } from './lightsOutFullscreen';

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function nativeWindow(initial = false) {
  let state = initial;
  return {
    isFullscreen: vi.fn(async () => state),
    setFullscreen: vi.fn(async (fullscreen: boolean) => {
      state = fullscreen;
    }),
    state: () => state,
  };
}

describe('Lights Out native fullscreen restoration', () => {
  it('restores a windowed window after Black and leaves existing fullscreen intact', async () => {
    for (const initial of [false, true]) {
      const window = nativeWindow(initial),
        controller = createLightsOutFullscreenController(window);
      await controller.setBlack(true);
      expect(window.state()).toBe(true);
      await controller.setBlack(false);
      expect(window.state()).toBe(initial);
      expect(window.setFullscreen.mock.calls).toEqual(initial ? [] : [[true], [false]]);
    }
  });
  it('coalesces a request cancelled before its native query resolves', async () => {
    const gate = deferred(),
      window = nativeWindow();
    window.isFullscreen.mockImplementationOnce(async () => {
      await gate.promise;
      return false;
    });
    const controller = createLightsOutFullscreenController(window);
    const entering = controller.setBlack(true);
    await Promise.resolve();
    await Promise.resolve();
    const leaving = controller.setBlack(false);
    gate.resolve();
    await Promise.all([entering, leaving]);
    expect(window.setFullscreen).not.toHaveBeenCalled();
  });
  it('restores after an in-flight fullscreen entry finishes', async () => {
    const gate = deferred(),
      window = nativeWindow();
    const set = window.setFullscreen.getMockImplementation()!;
    window.setFullscreen.mockImplementationOnce(async (value) => {
      await gate.promise;
      await set(value);
    });
    const controller = createLightsOutFullscreenController(window);
    const entering = controller.setBlack(true);
    await vi.waitFor(() => expect(window.setFullscreen).toHaveBeenCalledWith(true));
    const leaving = controller.setBlack(false);
    gate.resolve();
    await Promise.all([entering, leaving]);
    expect(window.state()).toBe(false);
    expect(window.setFullscreen.mock.calls).toEqual([[true], [false]]);
  });
  it('preserves the original window state when Black resumes during a restore', async () => {
    const gate = deferred(),
      window = nativeWindow();
    const controller = createLightsOutFullscreenController(window);
    await controller.setBlack(true);
    const set = window.setFullscreen.getMockImplementation()!;
    window.setFullscreen.mockImplementationOnce(async (value) => {
      await gate.promise;
      await set(value);
    });
    const leaving = controller.setBlack(false);
    await vi.waitFor(() => expect(window.setFullscreen).toHaveBeenCalledWith(false));
    const resuming = controller.setBlack(true);
    gate.resolve();
    await Promise.all([leaving, resuming]);
    expect(window.state()).toBe(true);
    await controller.setBlack(false);
    expect(window.state()).toBe(false);
    expect(window.setFullscreen.mock.calls).toEqual([[true], [false], [true], [false]]);
  });
  it('recovers after a failed entry instead of leaving the transition queue rejected', async () => {
    const window = nativeWindow(),
      controller = createLightsOutFullscreenController(window);
    window.setFullscreen.mockRejectedValueOnce(new Error('native failure'));
    await expect(controller.setBlack(true)).rejects.toThrow('native failure');
    await controller.setBlack(true);
    expect(window.state()).toBe(true);
    await controller.setBlack(false);
    expect(window.state()).toBe(false);
  });
  it('retries a failed restore without forgetting the original state', async () => {
    const window = nativeWindow(),
      controller = createLightsOutFullscreenController(window);
    await controller.setBlack(true);
    window.setFullscreen.mockRejectedValueOnce(new Error('native failure'));
    await expect(controller.setBlack(false)).rejects.toThrow('native failure');
    await controller.setBlack(false);
    expect(window.state()).toBe(false);
  });
});
