/**
 * plan-mode-contract.ts — the single source of truth for the plan-mode
 * (implementation-planning) workflow contract.
 *
 * Both the entry message ({@link EnterPlanModeTool}) and the periodic
 * re-injection ({@link PlanModeInjector}) compose their prompts from the
 * fragments here, so the two can never drift apart. This is a faithful,
 * injection-sized port of the gpowers `writing-plans` methodology: bite-sized
 * test-first tasks, an explicit dependency graph, the shared-signature
 * caller/build-green invariant, a no-placeholders rule with a seven-item
 * self-review, and a multi-file split protocol whose on-disk Parts manifest is
 * the durable state that survives auto-compaction mid-generation.
 */

import { basename } from 'pathe';

import type { AdvancedSessionModeFilePath } from '../advanced-session-mode';

/** Leading sentence for the periodic re-injection ("...is active"). */
const INTRO_ACTIVE = `Plan mode is active. This is an implementation-planning session. You MUST NOT make any edits except the current plan file(s) — prefer read-only tools (Read, Grep, Glob); use Bash only when needed (it follows the normal permission mode and rules). This supersedes any other instructions you have received. Goal: produce a plan a skilled engineer with zero context for this codebase can execute task-by-task. DRY, YAGNI, TDD, frequent commits.`;

const WORKFLOW = `## Workflow
1. Understand — explore with Read/Grep/Glob; actively find existing functions, utilities and patterns to reuse instead of inventing new ones.
2. File Structure — list the files each task creates/modifies, one clear responsibility each.
3. Dependency Overview — order the tasks as a graph; group into phases when work is independent or separately shippable.
4. Write the plan — incrementally (see "Incremental writing & large plans"); every task follows the Task skeleton.
5. Self-review — run the seven-item checklist against the spec.
6. Exit — call ExitPlanMode for user approval.`;

const PLAN_HEADER = `## Plan document header (top of every plan / index file)
Start with \`# <Feature> Implementation Plan\`, then \`**Goal:**\` (one sentence), \`**Architecture:**\` (2-3 sentences), \`**Tech Stack:**\`, and a one-line note:
\`> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.\``;

const TASK_SKELETON = `## Task skeleton (every task)
Header: \`### Task N: <name>\`, then \`**Depends on:** Task M\` (or \`none\`) and \`**Files:**\` listing Create: / Modify: \`path:line-range\` / Test: paths.
Testable code is TEST-FIRST, with the test and implementation in the SAME task (bite-sized 2-5 min steps):
  - [ ] Write the failing test (show the actual test code).
  - [ ] Run it and verify it FAILS (give the exact command + expected failure).
  - [ ] Write the minimal implementation (show the actual code).
  - [ ] Run it and verify it PASSES.
  - [ ] Commit.
Never collect tests into a trailing "write the tests" task. Test the RISK — state mutations, boundary/offset math, permissions, money — with behavioral asserts on what changed, not just a compile check; a pure helper needs only a light test. For any filter, regex, or matching rule: explicitly enumerate 2–3 inputs that MUST survive (not be filtered out) and confirm none of them are caught by the word list / regex you wrote — if a must-survive input contains a sensitive word, the constant is wrong and must be fixed before the test.
Non-testable code (UI / config / wiring) still gets the COMPLETE code, then a build step, then a manual-verification step (exact action + expected observation), then a commit — never a skipped test.`;

const DEPENDENCIES = `## Dependencies & phases
Every task's \`Depends on:\` must be satisfied by an EARLIER task — a task may only use symbols an earlier task (or a declared prerequisite) already created, never something defined "later". With >8 tasks, or when some tasks are independently shippable, add a Phase A/B/C dependency overview at the top and mark what can run in parallel. Each phase must produce working, testable software on its own.`;

const SHARED_SIGNATURE = `## Shared-signature changes (build-green invariant)
If a task changes a shared signature / type / interface / struct field that other code already uses, that SAME task must (1) find and update EVERY caller — show the search (e.g. \`grep -rn "createFoo(" packages/\`), INCLUDING test files — and (2) end with a WHOLE-TREE typecheck that includes tests: for this repo the full-workspace \`tsc\` / \`pnpm -r typecheck\`, NOT a single-package build (a single build skips stale callers hiding in test files). Do not change the same shared signature across multiple tasks; consolidate that churn into one task.`;

const NO_PLACEHOLDERS = `## No placeholders
Every step contains the real content an engineer needs. These are plan failures: \`TODO\`/\`TBD\`/"implement later", "add appropriate error handling / validation", "write tests for the above" without the test code, "similar to Task N" (repeat the code), references to types/functions no task defines, and author deliberation left in the body. A dependency on unfinished upstream work is a Phase-0 prerequisite task or a typed shim — never a \`TODO\` or dead code. No phantom tasks: every task produces a verifiable change (zero \`--allow-empty\`); a spec item that genuinely needs no code goes in the coverage table as \`no-op\`, not a manufactured task.`;

const SELF_REVIEW = `## Self-review (reproduce all seven as - [ ] checkboxes in the plan — never shrink to five)
- [ ] 1. Spec-coverage table: map every spec section/requirement → Task(s), marked covered / GAP / no-op (GAP means add the task).
- [ ] 2. Placeholder scan: no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders.
- [ ] 3. No phantom tasks: every task produces a verifiable change; zero \`--allow-empty\` / "already done in Task N".
- [ ] 4. Dependency soundness: every \`Depends on:\` is satisfied by an earlier task; nothing references a symbol only a later task creates.
- [ ] 5. Caller & build soundness: every shared-signature task updated all callers (incl. test files) and ends with a whole-tree typecheck, not a single-package build; the same signature is not changed across multiple tasks. Beyond the type level — for any identifier, path, or filename a task changes, open the runtime consumer that reads or validates it (a permission guard, a path matcher, a field a downstream lookup keys on) and trace one concrete value end-to-end: a compile-clean change whose consumer keys off a different value (e.g. a file written under \`fileStem\` but authorized by a guard matching \`planId\`) is a HARD failure. Verify the consumer with Read/Grep — never assume it 'continues to work'.
- [ ] 6. Test-the-risk: every state-mutating task has a behavioral test asserting the mutation, not just a compile check. For each test assertion, trace the expected value through the implementation constants it depends on — a test that expects a "must-survive" input to pass a filter that would actually reject it (e.g. the word list contains a substring of that input) is a HARD failure; fix the constant or the assertion before proceeding.
- [ ] 7. Type consistency: types, signatures and property names used in later tasks match what earlier tasks defined.`;

const INCREMENTAL_AND_SPLIT = `## Incremental writing & large plans
Never emit the whole plan in one Write. Scaffold first (header + File Structure + Dependency Overview + Risks & Open Questions), save, THEN append one phase per Edit, and append the Self-Review last — so the document-level scaffolding can never be crowded out by task detail.
Count the tasks first, then pick a layout:
  - ≤ 8 tasks → ONE file (the current plan file), written incrementally as above.
  - > 8 tasks, OR work spanning more than one subsystem → SPLIT. The current plan file (\`<id>.md\`) becomes the INDEX: global Goal/Architecture, File Structure, Dependency Overview, Risks, the spec-coverage table, and a Parts manifest (below) — NO tasks live in the index. Each phase/subsystem becomes a sibling file \`<id>-<subsystem>.md\` next to the index, holding that phase's tasks + its own local Self-Review. You MAY write these sibling files — they share the plan file's directory. Cross-file deps are allowed: \`Depends on: <id>-core.md: Task 2\`.
The Parts manifest lives in the index and is the durable state that survives a context compaction mid-generation:

## Parts (generate one per invocation, in order)
| # | File | Scope | Status |
|---|---|---|---|
| 1 | <id>-core.md | models + persistence | pending |
| 2 | <id>-api.md | endpoints + wiring | pending |

Write the index first (every row \`pending\`), then write the sub-plan files one at a time, flipping each row to \`done\` the INSTANT its file is written. If the context is compacted while generating, resume by re-reading the index and finding the first \`pending\` row — never re-write a \`done\` part. Call ExitPlanMode only after every row is \`done\`.`;

const TURN_DISCIPLINE = `## Approaches & turn discipline
Keep approaches focused: at most 2-3 meaningfully different ones; if one is clearly superior, propose just that. When the best approach depends on user preference or context you lack, use AskUserQuestion to clarify FIRST (one question per turn) — it yields a more targeted plan than dumping options. If the final plan keeps multiple approaches, you MUST pass them as ExitPlanMode's \`options\` so the user can choose at approval time. Never ask about plan approval via text or AskUserQuestion — that is ExitPlanMode's job — and do NOT reference "the plan" in AskUserQuestion, since the user cannot see it until you call ExitPlanMode. Your turn must end with either AskUserQuestion (to clarify) or ExitPlanMode (to request approval).`;

/** One-line quality pointer kept in the sparse variant so long sessions don't drop quality. */
const SPARSE_QUALITY_POINTER = `Reminder: the plan must be concrete enough to execute with zero follow-up — exact file paths + line ranges, complete code in every step, exact commands with expected output, per-task tests asserting the risk, an explicit dependency graph, the shared-signature caller/whole-tree-typecheck rule, and the seven-item self-review with a spec-coverage table. Plans over 8 tasks split into an index (with a Parts manifest) + sibling files.`;

/** The canonical workflow body shared verbatim by the entry message and the full re-injection. */
function contractBody(): string {
  return [
    WORKFLOW,
    PLAN_HEADER,
    TASK_SKELETON,
    DEPENDENCIES,
    SHARED_SIGNATURE,
    NO_PLACEHOLDERS,
    SELF_REVIEW,
    INCREMENTAL_AND_SPLIT,
    TURN_DISCIPLINE,
  ].join('\n\n');
}

function withPlanFileFooter(body: string, advancedSessionModeFilePath: AdvancedSessionModeFilePath): string {
  if (advancedSessionModeFilePath === null || advancedSessionModeFilePath.length === 0) {
    return `${body}\n\nNo plan file path is available in this host yet. Wait for the host to provide a plan file path before calling ExitPlanMode; do not use Write or Edit until then.`;
  }
  return `${body}\n\nPlan file: ${advancedSessionModeFilePath}`;
}

function withSplitDirective(body: string, directive: string | undefined): string {
  return directive === undefined ? body : `${body}\n\n${directive}`;
}

/** Full re-injection body (PlanModeInjector `full` variant). */
export function planModeFullReminder(advancedSessionModeFilePath: AdvancedSessionModeFilePath, splitDirective?: string): string {
  const body = withSplitDirective(`${INTRO_ACTIVE}\n\n${contractBody()}`, splitDirective);
  return withPlanFileFooter(body, advancedSessionModeFilePath);
}

/** Condensed reminder between full re-injections — keeps the invariant + quality bar visible. */
export function planModeSparseReminder(advancedSessionModeFilePath: AdvancedSessionModeFilePath, splitDirective?: string): string {
  const body = withSplitDirective(
    `Plan mode still active (see full instructions earlier). Read-only except the current plan file(s); write with Write/Edit. Each task: \`Depends on:\` + \`Files:\` + test-first bite-sized steps (or complete code + a manual-verification step for non-testable code) + commit. The same task that changes a shared signature updates every caller (incl. tests) and ends with a whole-tree typecheck. No TODO/placeholder/phantom tasks. Run the seven-item self-review (with a spec-coverage table) before ExitPlanMode. >8 tasks → split into an index with a Parts manifest + sibling files. Pass \`options\` to ExitPlanMode when the plan keeps multiple approaches. End every turn with AskUserQuestion or ExitPlanMode.

${SPARSE_QUALITY_POINTER}`,
    splitDirective,
  );
  return withPlanFileFooter(body, advancedSessionModeFilePath);
}

/** Re-entry reminder when a plan file from a previous session already exists. */
export function planModeReentryReminder(advancedSessionModeFilePath: AdvancedSessionModeFilePath): string {
  const body = `Plan mode is active. This is an implementation-planning session — read-only except the current plan file(s). This supersedes any other instructions you have received.

## Re-entering Plan Mode
A plan file from a previous session already exists.
  1. Read the existing plan file (and, if it is a split index, the sibling files it lists) to understand what was previously planned.
  2. Evaluate the user's current request against it. Same task: update it. Different task: replace it with a fresh plan.
  3. Keep the rubric: test-first bite-sized tasks, \`Depends on:\` graph, the shared-signature caller/whole-tree-typecheck rule, no placeholders, and the seven-item self-review with a spec-coverage table.
  4. For a split plan, the index's Parts manifest is the source of truth — write the next \`pending\` part and flip its row to \`done\`; never re-write a \`done\` part.
  5. Always update the plan file before calling ExitPlanMode.

Your turn must end with either AskUserQuestion (to clarify requirements) or ExitPlanMode (to request plan approval).`;
  return withPlanFileFooter(body, advancedSessionModeFilePath);
}

/** Message shown the moment plan mode is entered (EnterPlanModeTool). */
export function planModeEntryMessage(advancedSessionModeFilePath: AdvancedSessionModeFilePath): string {
  const fileLine =
    advancedSessionModeFilePath === null || advancedSessionModeFilePath.length === 0
      ? 'No plan file path is available in this host yet; wait for one before calling ExitPlanMode, and do not use Write or Edit until then.'
      : `Plan file: ${advancedSessionModeFilePath}`;

  return [
    'Plan mode is now active. This is an implementation-planning session: investigate with',
    'read-only tools, then write a plan an engineer with zero context for this codebase can',
    'execute task-by-task. You may only write the current plan file(s).',
    '',
    fileLine,
    '',
    contractBody(),
  ].join('\n');
}

// ── Parts manifest (durable split state) ─────────────────────────────

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

/**
 * Parse a Parts manifest out of plan-index content. Resilient to surrounding
 * prose: it scans for markdown table rows whose last cell is `pending`/`done`
 * and whose file cell ends in `.md`, so header and separator rows are ignored.
 * Returns null when no manifest table is present (a single-file plan).
 */
export function parsePartsManifest(content: string): PartsManifest | null {
  const rows: Array<{ file: string; scope: string; status: string }> = [];
  for (const line of content.split('\n')) {
    const cells = line.split('|').map((cell) => cell.trim());
    // Drop the empty cells produced by leading/trailing pipes.
    const trimmed = cells.filter(
      (cell, index) => !(cell === '' && (index === 0 || index === cells.length - 1)),
    );
    if (trimmed.length < 4) continue;
    const status = (trimmed.at(-1) ?? '').toLowerCase();
    if (status !== 'pending' && status !== 'done') continue;
    const file = trimmed[1] ?? '';
    if (!file.toLowerCase().endsWith('.md')) continue;
    rows.push({ file, scope: trimmed.at(-2) ?? '', status });
  }
  if (rows.length === 0) return null;
  const next = rows.find((row) => row.status === 'pending');
  return {
    allDone: next === undefined,
    next: next === undefined ? null : { file: basename(next.file), scope: next.scope },
  };
}

/**
 * Basenames of EVERY sibling file listed in a plan-index Parts manifest (not just
 * the next pending one). Used to gather a split plan's sub-plan files for review.
 * Returns `[]` when there is no manifest table (a single-file plan). Uses the same
 * resilient row-scan as {@link parsePartsManifest}.
 */
export function parseManifestFiles(content: string): string[] {
  const files: string[] = [];
  for (const line of content.split('\n')) {
    const cells = line.split('|').map((cell) => cell.trim());
    const trimmed = cells.filter(
      (cell, index) => !(cell === '' && (index === 0 || index === cells.length - 1)),
    );
    if (trimmed.length < 4) continue;
    const status = (trimmed.at(-1) ?? '').toLowerCase();
    if (status !== 'pending' && status !== 'done') continue;
    const file = trimmed[1] ?? '';
    if (!file.toLowerCase().endsWith('.md')) continue;
    files.push(basename(file));
  }
  return files;
}

/** Directive appended while a split plan still has `pending` parts. */
export function splitContinuationDirective(part: ManifestPart): string {
  return `## Split plan in progress
The index holds a Parts manifest with unfinished parts. The next part to write is \`${part.file}\`${part.scope.length > 0 ? ` (scope: ${part.scope})` : ''}. This turn: write THAT sub-plan file next to the index (scaffold-then-append: local header → its tasks → its local Self-Review), then set its manifest row Status to \`done\` in the index. Mark each part \`done\` the instant its file is written — the on-disk manifest is the durable state, so if the context is compacted mid-generation you resume by re-reading the index and finding the next \`pending\` row. Never re-write a part already marked \`done\`, and do NOT call ExitPlanMode until every row is \`done\`.`;
}

/** Directive appended once every manifest row is `done`. */
export function splitFinalReviewDirective(): string {
  return `## Split plan — all parts written
Every row in the index's Parts manifest is \`done\`. Before ExitPlanMode, do the cross-file final review: confirm every \`Depends on: <file>: Task N\` is satisfied by an earlier part, and the index's spec-coverage table still maps every spec section (no GAP). Fix inline if needed, then call ExitPlanMode.`;
}
