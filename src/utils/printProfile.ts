export interface PrintProfileOptions {
  profilePath: string;
  intent: 'relative' | 'perceptual';
  blackPointCompensation: boolean;
}

export const DEFAULT_PRINT_PROFILE: PrintProfileOptions = {
  profilePath: '',
  intent: 'relative',
  blackPointCompensation: true,
};
