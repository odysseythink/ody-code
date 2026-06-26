import type { FieldDiff, NormalizedSnapshot, ParityDiff } from './types';

export function assertParity(
  scenarioName: string,
  ts: NormalizedSnapshot,
  rust: NormalizedSnapshot,
): ParityDiff | null {
  const diffs: FieldDiff[] = [];
  collectDiffs(ts, rust, '$', diffs);
  if (diffs.length === 0) return null;
  return { scenarioName, ts, rust, diffs };
}

function collectDiffs(a: unknown, b: unknown, path: string, diffs: FieldDiff[]): void {
  if (Object.is(a, b)) return;

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

  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) {
      diffs.push({ path, tsValue: a.length, rustValue: b.length });
      return;
    }
    for (let i = 0; i < a.length; i++) {
      collectDiffs(a[i], b[i], `${path}[${i}]`, diffs);
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
    );
  }
}
