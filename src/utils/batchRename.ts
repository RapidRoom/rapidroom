export type RenameGroupMode = 'selection' | 'auto';

export interface RenameOptions {
  groupMode: RenameGroupMode;
  groupSeconds: number;
}

export const DEFAULT_RENAME_OPTIONS: RenameOptions = { groupMode: 'auto', groupSeconds: 1 };

export interface RenamePreviewEntry {
  from: string;
  to: string;
  kind: 'image' | 'sidecar';
  photo: number;
  conflict: 'inBatch' | 'existing' | null;
}

export interface RenamePreview {
  entries: RenamePreviewEntry[];
  errors: string[];
  photoCount: number;
  conflictCount: number;
}

export interface PathChange {
  from: string;
  to: string;
}

export interface RenameOutcome {
  files: PathChange[];
  images: PathChange[];
}

export interface UndoInfo {
  renamedAt: string;
  photoCount: number;
  fileCount: number;
}

const UNIQUE_TOKENS = ['{sequence}', '{original_filename}', '{member}'];

// Several images need something that tells them apart.
export function finalizeRenameTemplate(template: string, isSingleFile: boolean): string {
  const trimmed = template.trim();
  if (!trimmed || isSingleFile || UNIQUE_TOKENS.some((token) => trimmed.includes(token))) {
    return trimmed;
  }
  return `${trimmed}_{sequence}`;
}

export function usesGroupTokens(template: string): boolean {
  return template.includes('{group}') || template.includes('{member}');
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

// Maps library paths (including `?vc=` virtual copies) through a rename.
export function pathMapper(outcome: RenameOutcome): (path: string) => string {
  const map = new Map(outcome.images.map((change) => [change.from, change.to]));
  return (path: string) => map.get(path) ?? path;
}
