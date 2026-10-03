// Card mode opens a memory card read-only. The backend refuses every write under the card root;
// the frontend uses this to keep edits in memory and to show that the folder is read-only.
export function isPathInCardRoot(path: string | null | undefined, cardRoot: string | null | undefined): boolean {
  if (!path || !cardRoot) {
    return false;
  }
  const root = cardRoot.replace(/[\\/]+$/, '');
  const basePath = path.split('?vc=')[0];
  return basePath === root || basePath.startsWith(`${root}/`) || basePath.startsWith(`${root}\\`);
}
