/**
 * parts-manifest.ts — mode-agnostic parsing for the Parts manifest that drives
 * the multi-file split protocol.
 *
 * Both plan mode and design mode write a split INDEX whose durable state is a
 * markdown "Parts" table (one row per sibling file, each `pending`/`done`).
 * The parsing here is purely textual and carries no plan/design-specific
 * wording, so both session modes import it. The split *directives* (the prompt
 * text steering the model to the next part) stay in each mode's contract file,
 * since their wording differs (ExitPlanMode vs ExitDesignMode, "task" vs
 * "component").
 */

import { basename } from 'pathe';

export interface ManifestPart {
  readonly file: string;
  readonly scope: string;
}

export interface PartsManifest {
  /** True when a manifest table exists and every row is `done`. */
  readonly allDone: boolean;
  /** The first `pending` row, or null when none remain. */
  readonly next: ManifestPart | null;
}

export interface ManifestCounts {
  /** Number of rows marked `done`. */
  readonly done: number;
  /** Number of rows marked `pending`. */
  readonly pending: number;
}

interface ManifestRow {
  readonly file: string;
  readonly scope: string;
  readonly status: 'pending' | 'done';
}

/**
 * Scan split-index content for Parts-manifest table rows. Resilient to surrounding
 * prose: a row counts only when its last cell is `pending`/`done` and its file cell
 * ends in `.md`, so header and separator rows are ignored. The single source of truth
 * shared by every manifest accessor below.
 */
function scanManifestRows(content: string): ManifestRow[] {
  const rows: ManifestRow[] = [];
  for (const line of content.split('\n')) {
    const cells = line.split('|').map((cell) => cell.trim());
    // Drop the empty cells produced by leading/trailing pipes.
    const trimmed = cells.filter(
      (cell, index) => !(cell === '' && (index === 0 || index === cells.length - 1)),
    );
    if (trimmed.length < 4) continue;
    const status = (trimmed.at(-1) ?? '').toLowerCase();
    if (status !== 'pending' && status !== 'done') continue;
    const file = (trimmed[1] ?? '').replaceAll('`', '').trim();
    if (!file.toLowerCase().endsWith('.md')) continue;
    rows.push({ file, scope: trimmed.at(-2) ?? '', status });
  }
  return rows;
}

/**
 * Parse a Parts manifest out of split-index content.
 * Returns null when no manifest table is present (a single-file plan/design).
 */
export function parsePartsManifest(content: string): PartsManifest | null {
  const rows = scanManifestRows(content);
  if (rows.length === 0) return null;
  const next = rows.find((row) => row.status === 'pending');
  return {
    allDone: next === undefined,
    next: next === undefined ? null : { file: basename(next.file), scope: next.scope },
  };
}

/**
 * Basenames of EVERY sibling file listed in a split-index Parts manifest (not just
 * the next pending one). Used to gather a split plan/design's sibling files for review.
 * Returns `[]` when there is no manifest table (a single-file plan/design).
 */
export function parseManifestFiles(content: string): string[] {
  return scanManifestRows(content).map((row) => basename(row.file));
}

/**
 * Counts of `done`/`pending` rows in a split-index Parts manifest, or null when
 * there is no manifest table (a single-file plan/design). Used to detect part
 * boundaries (a `done` count that grew since the previous step).
 */
export function countManifestRows(content: string): ManifestCounts | null {
  const rows = scanManifestRows(content);
  if (rows.length === 0) return null;
  let done = 0;
  let pending = 0;
  for (const row of rows) {
    if (row.status === 'done') done++;
    else pending++;
  }
  return { done, pending };
}
