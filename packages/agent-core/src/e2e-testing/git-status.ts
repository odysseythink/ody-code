/**
 * Parse `git status --short` output into file paths.
 *
 * Each short-status line has the form `XY path` (or `XY orig -> rename`).
 * We skip the first two status columns and return the (possibly renamed) path.
 */
export function parseGitStatusShort(output: string): string[] {
  const files: string[] = [];
  for (const raw of output.split('\n')) {
    const trimmed = raw.trim();
    if (trimmed.length < 4) continue;
    let path = raw.substring(3).trim();
    if (path.includes(' -> ')) {
      path = path.split(' -> ').pop()!.trim();
    }
    if (path.length > 0) {
      files.push(path);
    }
  }
  return files;
}
