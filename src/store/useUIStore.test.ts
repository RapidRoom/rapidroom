import { afterEach, describe, expect, it } from 'vitest';
import { useUIStore } from './useUIStore';

const initialState = useUIStore.getState();

afterEach(() => {
  useUIStore.setState(initialState, true);
});

describe('useUIStore.cycleLightsOut', () => {
  it('cycles Normal, Dim, Black and back to Normal', () => {
    const { cycleLightsOut } = useUIStore.getState();
    expect(useUIStore.getState().lightsOutMode).toBe('off');
    cycleLightsOut();
    expect(useUIStore.getState().lightsOutMode).toBe('dim');
    cycleLightsOut();
    expect(useUIStore.getState().lightsOutMode).toBe('black');
    cycleLightsOut();
    expect(useUIStore.getState().lightsOutMode).toBe('off');
  });

  it('cycles backward', () => {
    const { cycleLightsOut } = useUIStore.getState();
    cycleLightsOut(-1);
    expect(useUIStore.getState().lightsOutMode).toBe('black');
    cycleLightsOut(-1);
    expect(useUIStore.getState().lightsOutMode).toBe('dim');
  });
});
