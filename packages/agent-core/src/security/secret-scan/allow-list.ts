/**
 * Normalize an allow-list: trim, remove empty entries, deduplicate.
 */
export function normalizeAllowList(input: readonly string[] | undefined): readonly string[] {
  if (input === undefined) return [];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const entry of input) {
    const trimmed = entry.trim();
    if (trimmed.length === 0) continue;
    if (seen.has(trimmed)) continue;
    seen.add(trimmed);
    out.push(trimmed);
  }
  return out;
}
