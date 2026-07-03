import type { FieldDiff, NormalizedSnapshot, ParityDiff } from './types';

export function assertParity(
  scenarioName: string,
  ts: NormalizedSnapshot,
  rust: NormalizedSnapshot,
): ParityDiff | null {
  const diffs: FieldDiff[] = [];
  const seen = new WeakSet<object>();
  collectDiffs(ts, rust, '$', diffs, seen);
  if (diffs.length === 0) return null;
  return { scenarioName, ts, rust, diffs };
}

function collectDiffs(
  a: unknown,
  b: unknown,
  path: string,
  diffs: FieldDiff[],
  seen: WeakSet<object>,
): void {
  if (Object.is(a, b)) return;

  // Treat null/undefined as equivalent at leaf level.
  if (a === undefined && b === null) return;
  if (a === null && b === undefined) return;

  const typeA = typeof a;
  const typeB = typeof b;

  if (a === null || b === null) {
    diffs.push({ path, tsValue: a, rustValue: b });
    return;
  }

  if (typeA !== typeB || Array.isArray(a) !== Array.isArray(b)) {
    diffs.push({ path, tsValue: a, rustValue: b });
    return;
  }

  if (typeA !== 'object') {
    diffs.push({ path, tsValue: a, rustValue: b });
    return;
  }

  // Guard against cyclic references in snapshots.
  if (seen.has(a as object) || seen.has(b as object)) {
    return;
  }
  seen.add(a as object);
  seen.add(b as object);

  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) {
      diffs.push({ path: `${path}.length`, tsValue: a.length, rustValue: b.length });
    }
    const commonLength = Math.min(a.length, b.length);
    for (let i = 0; i < commonLength; i++) {
      collectDiffs(a[i], b[i], `${path}[${i}]`, diffs, seen);
    }
    return;
  }

  const keys = new Set([...Object.keys(a as object), ...Object.keys(b as object)]);
  for (const key of keys) {
    collectDiffs(
      (a as Record<string, unknown>)[key],
      (b as Record<string, unknown>)[key],
      `${path}.${key}`,
      diffs,
      seen,
    );
  }
}
