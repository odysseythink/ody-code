/** Derive a package root from monorepo-style changed file paths. */
export function derivePackageRoot(changedFiles: string[]): string | undefined {
  for (const file of changedFiles) {
    const normalized = file.replace(/\\/g, '/');
    const parts = normalized.split('/');
    if ((parts[0] === 'packages' || parts[0] === 'apps') && parts.length >= 2) {
      return `${parts[0]}/${parts[1]}`;
    }
  }
  return undefined;
}
