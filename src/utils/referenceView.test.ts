import { describe, expect, it } from 'vitest';
import {
  DEFAULT_REFERENCE_VIEW_STATE,
  getReferenceLabel,
  isReferenceCandidate,
  isReferenceViewActive,
  reduceReferenceView,
} from './referenceView';

const reference = { label: 'reference.raw', path: '/tmp/reference.raw' };

describe('reference view state', () => {
  it('opens the chooser when entering without a reference', () => {
    const state = reduceReferenceView(DEFAULT_REFERENCE_VIEW_STATE, { type: 'enter' });
    expect(state.mode).toBe('side-by-side');
    expect(state.isChooserOpen).toBe(true);
    expect(isReferenceViewActive(state)).toBe(true);
  });

  it('keeps the pinned reference across exit and re-enter', () => {
    const chosen = reduceReferenceView(DEFAULT_REFERENCE_VIEW_STATE, { image: reference, type: 'set-reference' });
    expect(chosen.isChooserOpen).toBe(false);
    const exited = reduceReferenceView(chosen, { type: 'toggle' });
    expect(exited.mode).toBe('off');
    expect(exited.reference).toEqual(reference);
    const reentered = reduceReferenceView(exited, { type: 'toggle' });
    expect(reentered.mode).toBe('side-by-side');
    expect(reentered.isChooserOpen).toBe(false);
    expect(reentered.reference).toEqual(reference);
  });

  it('clears the reference and asks for a new one', () => {
    const chosen = reduceReferenceView(DEFAULT_REFERENCE_VIEW_STATE, { image: reference, type: 'set-reference' });
    const cleared = reduceReferenceView(chosen, { type: 'clear-reference' });
    expect(cleared.reference).toBeNull();
    expect(cleared.isChooserOpen).toBe(true);
    expect(cleared.mode).toBe('side-by-side');
  });

  it('cancelling a replace keeps the current reference', () => {
    const chosen = reduceReferenceView(DEFAULT_REFERENCE_VIEW_STATE, { image: reference, type: 'set-reference' });
    const choosing = reduceReferenceView(chosen, { type: 'open-chooser' });
    expect(choosing.isChooserOpen).toBe(true);
    const cancelled = reduceReferenceView(choosing, { type: 'close-chooser' });
    expect(cancelled.isChooserOpen).toBe(false);
    expect(cancelled.reference).toEqual(reference);
  });

  it('rejects the active image and the current reference as candidates', () => {
    const state = { ...DEFAULT_REFERENCE_VIEW_STATE, reference };
    expect(isReferenceCandidate(state, '/tmp/active.raw', '/tmp/active.raw')).toBe(false);
    expect(isReferenceCandidate(state, reference.path, '/tmp/active.raw')).toBe(false);
    expect(isReferenceCandidate(state, '/tmp/next.raw', '/tmp/active.raw')).toBe(true);
    expect(isReferenceCandidate(state, '/tmp/active.raw?vc=abc', '/tmp/active.raw')).toBe(true);
  });

  it('labels files and virtual copies', () => {
    expect(getReferenceLabel('/photos/IMG_0001.CR3')).toBe('IMG_0001.CR3');
    expect(getReferenceLabel('C:\\photos\\IMG_0001.CR3')).toBe('IMG_0001.CR3');
    expect(getReferenceLabel('/photos/IMG_0001.CR3?vc=a1b2')).toBe('IMG_0001.CR3 (VC)');
  });
});
