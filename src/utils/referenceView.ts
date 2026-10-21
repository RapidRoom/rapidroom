export interface ReferenceViewImage {
  readonly path: string;
  readonly label: string;
}

export interface ReferenceViewState {
  readonly isChooserOpen: boolean;
  readonly mode: 'off' | 'side-by-side' | 'wipe';
  readonly reference: ReferenceViewImage | null;
  readonly wipePosition: number;
  readonly wipeSwapped: boolean;
  readonly wipeReference: boolean;
}

export type ReferenceViewCommand =
  | { type: 'enter' }
  | { type: 'exit' }
  | { type: 'toggle' }
  | { type: 'toggle-wipe' }
  | { type: 'toggle-original-wipe' }
  | { type: 'set-wipe-position'; position: number }
  | { type: 'swap-wipe-sides' }
  | { type: 'open-chooser' }
  | { type: 'close-chooser' }
  | { image: ReferenceViewImage; type: 'set-reference' }
  | { type: 'clear-reference' };

export const DEFAULT_REFERENCE_VIEW_STATE: ReferenceViewState = {
  isChooserOpen: false,
  mode: 'off',
  reference: null,
  wipePosition: 50,
  wipeSwapped: false,
  wipeReference: false,
};

export const reduceReferenceView = (state: ReferenceViewState, command: ReferenceViewCommand): ReferenceViewState => {
  switch (command.type) {
    case 'enter':
      return { ...state, isChooserOpen: state.reference === null, mode: 'side-by-side' };
    case 'exit':
      return { ...state, isChooserOpen: false, mode: 'off' };
    case 'toggle':
      return reduceReferenceView(state, { type: state.mode === 'off' ? 'enter' : 'exit' });
    case 'toggle-wipe':
      return state.mode === 'wipe'
        ? { ...state, mode: 'off', isChooserOpen: false }
        : { ...state, mode: 'wipe', isChooserOpen: false, wipeReference: state.reference !== null };
    case 'toggle-original-wipe':
      return state.mode === 'wipe' && !state.wipeReference
        ? { ...state, mode: 'off', isChooserOpen: false }
        : { ...state, mode: 'wipe', isChooserOpen: false, wipeReference: false };
    case 'set-wipe-position':
      return { ...state, wipePosition: Math.max(0, Math.min(100, command.position)) };
    case 'swap-wipe-sides':
      return { ...state, wipeSwapped: !state.wipeSwapped };
    case 'open-chooser':
      return { ...state, isChooserOpen: true, mode: 'side-by-side' };
    case 'close-chooser':
      return { ...state, isChooserOpen: false };
    case 'set-reference':
      return { ...state, isChooserOpen: false, mode: 'side-by-side', reference: command.image };
    case 'clear-reference':
      return { ...state, isChooserOpen: true, mode: 'side-by-side', reference: null };
  }
};

export const isReferenceViewActive = (state: ReferenceViewState): boolean => state.mode !== 'off';

export const isReferenceCandidate = (state: ReferenceViewState, candidatePath: string, activePath: string | null) =>
  candidatePath !== activePath && candidatePath !== state.reference?.path;

export const getReferenceLabel = (path: string): string => {
  const [sourcePath, vcId] = path.split('?vc=');
  const fileName = sourcePath.split(/[\\/]/).pop() || sourcePath;
  return vcId ? `${fileName} (VC)` : fileName;
};

// The left half is the comparison source (Original, or the pinned version's own
// name) unless the sides are swapped, so each label follows its image.
export const getWipeLabels = (swapped: boolean, referenceLabel: string | null = null) => {
  const comparison = referenceLabel || 'Original';
  return swapped ? { left: 'Your edit', right: comparison } : { left: comparison, right: 'Your edit' };
};
