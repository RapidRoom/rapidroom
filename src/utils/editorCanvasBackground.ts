export type EditorCanvasBackground = 'theme' | 'black' | 'dark-grey' | 'mid-grey' | 'white';

export const resolveEditorCanvasBackground = (
  background: string | undefined,
  legacyNeutralGrey: boolean | undefined,
): EditorCanvasBackground => {
  switch (background) {
    case 'black':
    case 'dark-grey':
    case 'mid-grey':
    case 'white':
    case 'theme':
      return background;
    default:
      return legacyNeutralGrey ? 'mid-grey' : 'theme';
  }
};

export const canvasBackgroundRgb = (
  background: EditorCanvasBackground,
  themeBackground: [number, number, number, number],
): [number, number, number, number] => {
  switch (background) {
    case 'black':
      return [0, 0, 0, 1];
    case 'dark-grey':
      return [48 / 255, 48 / 255, 48 / 255, 1];
    case 'mid-grey':
      return [128 / 255, 128 / 255, 128 / 255, 1];
    case 'white':
      return [1, 1, 1, 1];
    default:
      return themeBackground;
  }
};
