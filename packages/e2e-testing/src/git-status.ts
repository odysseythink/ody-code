import type { Kaos } from '@odysseythink/kaos';

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

async function gitOutput(kaos: Kaos, args: string[]): Promise<string> {
  const proc = await kaos.exec('git', ...args);
  const chunks: Buffer[] = [];
  proc.stdout.on('data', (chunk: Buffer) => chunks.push(chunk));
  await proc.wait();
  return Buffer.concat(chunks).toString('utf-8');
}

/**
 * Resolve a baseline ref to diff `HEAD` against — the merge-base with the
 * default branch. Remote-tracking refs are tried first so that work committed
 * locally (but not yet pushed) is still counted as "changed". Returns null when
 * no baseline can be resolved (e.g. a fresh repo with no remote).
 */
async function resolveBaseRef(kaos: Kaos): Promise<string | null> {
  for (const ref of ['origin/HEAD', 'origin/main', 'origin/master', 'main', 'master']) {
    try {
      const base = (await gitOutput(kaos, ['merge-base', 'HEAD', ref])).trim();
      // A base equal to HEAD means there is nothing committed beyond the
      // baseline — keep looking for a ref that actually predates HEAD.
      if (base && base !== (await gitOutput(kaos, ['rev-parse', 'HEAD'])).trim()) {
        return base;
      }
    } catch {
      // ref does not exist here; try the next candidate
    }
  }
  return null;
}

/**
 * Detect the files changed by the current body of work: uncommitted changes
 * (`git status`) unioned with everything committed since the merge-base with the
 * default branch (`git diff <base>..HEAD`).
 *
 * The union matters because many workflows commit per task — by the time E2E
 * runs, the relevant changes are already committed and would be invisible to
 * `git status` alone. Best-effort: any git failure degrades to a partial (or
 * empty) result rather than throwing.
 */
export async function detectChangedFiles(kaos: Kaos, projectRoot: string): Promise<string[]> {
  const k = kaos.withCwd(projectRoot);
  const files = new Set<string>();

  try {
    for (const f of parseGitStatusShort(await gitOutput(k, ['status', '--short', '--no-renames']))) {
      files.add(f);
    }
  } catch {
    // not a git repo / git unavailable — fall through
  }

  try {
    const base = await resolveBaseRef(k);
    if (base !== null) {
      const out = await gitOutput(k, ['diff', '--name-only', '--no-renames', `${base}..HEAD`]);
      for (const line of out.split('\n')) {
        const path = line.trim();
        if (path.length > 0) files.add(path);
      }
    }
  } catch {
    // no resolvable baseline — uncommitted changes only
  }

  return [...files];
}
