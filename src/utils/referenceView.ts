export interface ReferenceViewImage {
  readonly path: string;
  readonly label: string;
}

export interface ReferenceViewState {
  readonly isChooserOpen: boolean;
  readonly mode: 'off' | 'side-by-side';
  readonly reference: ReferenceViewImage | null;
}

export type ReferenceViewCommand =
  | { type: 'enter' }
  | { type: 'exit' }
  | { type: 'toggle' }
  | { type: 'open-chooser' }
  | { type: 'close-chooser' }
  | { image: ReferenceViewImage; type: 'set-reference' }
  | { type: 'clear-reference' };

export const DEFAULT_REFERENCE_VIEW_STATE: ReferenceViewState = {
  isChooserOpen: false,
  mode: 'off',
  reference: null,
};

export const reduceReferenceView = (state: ReferenceViewState, command: ReferenceViewCommand): ReferenceViewState => {
  switch (command.type) {
    case 'enter':
      return { ...state, isChooserOpen: state.reference === null, mode: 'side-by-side' };
    case 'exit':
      return { ...state, isChooserOpen: false, mode: 'off' };
    case 'toggle':
      return reduceReferenceView(state, { type: state.mode === 'off' ? 'enter' : 'exit' });
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
