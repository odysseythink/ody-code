# Delay Topic Generation & Finalize Plan/Design Filename on Exit

## Scope

### In
- Eliminate the LLM-based `TopicGenerator.generate()` call on **every** plan/design mode entry (both TUI `/plan` `/design` and LLM `EnterPlanMode` / `EnterDesignMode` tool paths). [C:USER]
- Use a temporary `fileStem` (= `planId`) on entry so `_SessionModeFilePath` is immediately valid for the permission guard. [C:INFERRED]
- Compute the final filename **only on successful exit**, derived from the file content's first Markdown H1 heading plus a `YYYY-MM-DD-` date prefix. [C:USER]
- Remove the TUI notice subtitle `"Plan/Design will be created here: …"` shown when toggling modes. [C:USER]
- Preserve the ability for a user to manually pass `topic` to `EnterPlanMode` / `EnterDesignMode` tools; when provided it is sanitized, stored as `_manualTopicSlug`, and used as the fallback stem when no H1 heading is found on exit. [C:USER]
- Plan mode that follows a design mode in the same session may reuse the **title slug** from the exited design file (date part still uses the current day). [C:USER]

### Out
- Split-plan sibling files (`<planId>-<subsystem>.md`) are **not** renamed on exit. Only the main plan file gets a final name. Rationale: split plans are rare, and renaming siblings plus updating the Parts manifest index adds significant complexity for a low-frequency case. [C:INFERRED]
- Temporary `planId.md` files are **not deleted** after finalization. They remain on disk alongside the renamed file. Rationale: Kaos has no `unlink` abstraction, and attempting remote deletion via `node:fs` would break SSHKaos. The temp files are tiny and harmless. [C:USER]
- The existing `TopicGenerator` class and its unit tests are **preserved**; they are simply no longer invoked on mode entry. They may still be used later if we decide to pre-warm a topic in another flow. [C:INFERRED]

---

## Architecture & Data Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  TUI /plan or /design        LLM EnterPlanMode / EnterDesignMode             │
│        │                              │                                      │
│        ▼                              ▼                                      │
│  session.setPlanMode(true)    tool.execute()                                 │
│        │                              │                                      │
│        ▼                              ▼                                      │
│  rpc.enterPlan({kind})        agent.planMode.enter(..., kind, fileStem?)   │
│        │                              │                                      │
│        └──────────────┬───────────────┘                                      │
│                       ▼                                                      │
│              Agent.enterPlan()  ──►  REMOVED: TopicGenerator.generate()      │
│                       │                                                      │
│                       ▼                                                      │
│              planMode.enter(id, ..., kind, fileStem = planId)                │
│                       │                                                      │
│                       ▼                                                      │
│              _fileStem  = planId                                             │
│              _SessionModeFilePath = plans/planId.md  (or designs/planId.md)         │
│                       │                                                      │
│        ┌──────────────┼───────────────┐                                      │
│        ▼              ▼               ▼                                      │
│  TUI notice     PlanModeGuard    Entry message                               │
│  (no path)      allows Write     shows file path                             │
│                                  (still shows planId path)                   │
│                                                                               │
│  ── LLM writes content to plans/planId.md via Write/Edit ──►                │
│                                                                               │
│                       │                                                      │
│        ┌──────────────┼───────────────┐                                      │
│        ▼              ▼               ▼                                      │
│  ExitPlanMode   ExitDesignMode    (user cancels / exits early)               │
│  tool.execute() tool.execute()                                               │
│        │              │                                                      │
│        ▼              ▼                                                      │
│  planMode.finalizeFileName()  ──►  reads content                             │
│                       │          ──►  extracts H1 heading (strips markdown)   │
│                       │          ──►  checks for filename collision            │
│                       │          ──►  builds YYYY-MM-DD-<slug>.md             │
│                       │          ──►  writes to final path                   │
│                       │          ──►  (temp file left behind — harmless)     │
│                       │          ──►  updates _SessionModeFilePath & _fileStem      │
│                       │          ──►  emitStatusUpdated()                     │
│                       ▼                                                      │
│              planMode.exit()  ──►  restores model, clears active state       │
│                       │                                                      │
│                       ▼                                                      │
│              resolvePlan() / resolveDesign()                                 │
│              reads NEW _SessionModeFilePath, returns to user                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Components & Interfaces

### `PlanMode` (`packages/agent-core/src/agent/plan/index.ts`)

New fields:

```ts
export class PlanMode {
  // … existing fields …
  protected _lastDesignFileStem: string | null = null;
  protected _manualTopicSlug: string | null = null;

  /**
   * Finalize the on-disk filename before exit.
   * Reads the current plan/design file, extracts the first H1 heading
   * (stripping inline markdown formatting), builds a `YYYY-MM-DD-<slug>`
   * stem, checks for collisions and appends a numeric suffix if needed,
   * writes the content to the final path, leaves the temp path intact,
   * updates internal state, and emits a status update so the permission
   * guard picks up the new path.
   * If the file is empty or has no heading, the manual topic slug or
   * planId is used as fallback.
   *
   * @returns the final file path, or the existing path if unchanged.
   */
  async finalizeFileName(): Promise<string | null>;
}
```

Behaviour changes to existing methods:

- `enter(id, createFile, emitStatus, kind, fileStem?)`
  - When `fileStem` is provided (manual topic override):
    - Store `_manualTopicSlug = slugifyTitle(fileStem)` for fallback during finalization.
  - When `fileStem` is omitted / empty and `kind === 'plan'` and `_lastDesignFileStem` is set:
    - Extract the title slug from `_lastDesignFileStem`.
    - Set `fileStem = \`${today}-${slug}\`` so the plan file shares the design title.
  - When `fileStem` is omitted / empty and no design reuse applies:
    - `fileStem = id` (the hero-slug planId). **No LLM call.**

- `exit(id?)`
  - Before clearing state, if `kind === 'design'`:
    - Save `_lastDesignFileStem = this._fileStem`.
  - Clear `_manualTopicSlug = null`.
  - Then proceed with existing exit logic.

### Pure helpers (new, inside `topic-generator.ts` or a new nearby module)

```ts
/**
 * Extract the first Markdown H1 heading (# …) from content.
 * Strips common inline markdown formatting (*, _, `, [, ], (, ), {, }).
 * Returns the trimmed heading text, or null if none found.
 */
export function extractFirstHeading(content: string): string | null;

/**
 * Strip inline markdown formatting characters from a heading.
 */
export function stripMarkdownFormatting(text: string): string;

/**
 * Slugify a title for use in a filename: lowercase, hyphenated,
 * non-alphanumeric → hyphen, collapsed, trimmed, max 50 chars.
 */
export function slugifyTitle(title: string): string;

/**
 * Format a date as YYYY-MM-DD in UTC.
 */
export function formatDatePrefix(date: Date): string;
```

### `EnterPlanModeTool` / `EnterDesignModeTool`

- Remove the `TopicGenerator.generate()` call.
- Keep the manual `topic` sanitization path (`_args.topic !== undefined`).
- When no manual topic is given, pass **no** `fileStem` to `planMode.enter`, letting it fall back to `planId`.

### `ExitPlanModeTool` / `ExitDesignModeTool`

- In `execution()`:
  1. `await this.agent.planMode.finalizeFileName()` — **before** `resolvePlan()`.
  2. `const resolved = await this.resolvePlan()` — now reads the renamed file.
  3. Proceed with existing exit + output logic.

### TUI commands (`apps/ody-code/src/tui/commands/config.ts`)

- `applyPlanMode` and `applyDesignMode`: remove the second argument to `showNotice` so only the mode label is displayed, not the file path.

---

## Algorithms

### `finalizeFileName`

```ts
async finalizeFileName(): Promise<string | null> {
  if (!this._SessionModeFilePath || !this._fileStem) return this._SessionModeFilePath;

  let content: string;
  try {
    content = await this.agent.kaos.readText(this._SessionModeFilePath);
  } catch {
    return this._SessionModeFilePath; // nothing to finalize
  }

  if (content.trim().length === 0) {
    return this._SessionModeFilePath;
  }

  const heading = extractFirstHeading(content);
  const today = formatDatePrefix(new Date());
  const slug = heading
    ? slugifyTitle(heading)
    : (this._manualTopicSlug ?? this._sessionModeId ?? 'untitled');

  let finalStem = `${today}-${slug}`;

  // Collision detection: if the target path already exists, append -1, -2, …
  finalStem = await this.findUniqueStem(finalStem);

  if (finalStem === this._fileStem) {
    return this._SessionModeFilePath;
  }

  const finalPath = this.advancedSessionModeFilePathFor(finalStem);

  // Write to final path
  try {
    await this.agent.kaos.writeText(finalPath, content);
  } catch (error) {
    this.agent.log?.warn('Failed to write finalized plan/design file', { error });
    return this._SessionModeFilePath; // keep temp on write failure
  }

  this._SessionModeFilePath = finalPath;
  this._fileStem = finalStem;
  this.agent.emitStatusUpdated();
  return finalPath;
}

async findUniqueStem(baseStem: string): Promise<string> {
  let stem = baseStem;
  let suffix = 1;
  while (true) {
    const candidatePath = this.advancedSessionModeFilePathFor(stem);
    try {
      await this.agent.kaos.stat(candidatePath);
      // File exists — try next suffix
      stem = `${baseStem}-${suffix}`;
      suffix++;
    } catch {
      // File does not exist — safe to use
      return stem;
    }
  }
}
```

### `extractFirstHeading`

```ts
export function extractFirstHeading(content: string): string | null {
  const match = content.match(/^#\s+(.+)$/m);
  const raw = match?.[1]?.trim();
  if (!raw || raw.length === 0) return null;
  const cleaned = stripMarkdownFormatting(raw);
  return cleaned.length > 0 ? cleaned : null;
}
```

### `stripMarkdownFormatting`

```ts
export function stripMarkdownFormatting(text: string): string {
  // Remove common inline markdown syntax characters
  return text.replace(/[*_`{}\[\]()#]+/g, '').trim();
}
```

### `slugifyTitle`

```ts
export function slugifyTitle(title: string): string {
  let s = title
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-+|-+$/g, '')
    .replace(/-+/g, '-');
  if (s.length > 50) {
    s = s.slice(0, 50).replace(/-+$/, '');
  }
  return s;
}
```

### `formatDatePrefix`

```ts
export function formatDatePrefix(date: Date): string {
  return date.toISOString().slice(0, 10); // YYYY-MM-DD
}
```

### Reuse design title in plan mode (`enter`)

```ts
// Inside PlanMode.enter, before calling advancedSessionModeFilePathFor:
let effectiveStem = fileStem;
if (!effectiveStem) {
  if (kind === 'plan' && this._lastDesignFileStem) {
    const designSlug = extractSlugFromDatedStem(this._lastDesignFileStem);
    effectiveStem = `${formatDatePrefix(new Date())}-${designSlug}`;
  } else {
    effectiveStem = id;
  }
}
if (fileStem) {
  this._manualTopicSlug = slugifyTitle(fileStem);
}
this._fileStem = effectiveStem ?? id;
```

```ts
function extractSlugFromDatedStem(stem: string): string {
  // Stems look like "2024-06-05-implement-glm-provider"
  // Find first dash after the date (YYYY-MM-DD-...)
  const m = stem.match(/^\d{4}-\d{2}-\d{2}-(.+)$/);
  return m ? m[1] : stem;
}
```

---

## Call-Site Integration

### 1. `packages/agent-core/src/agent/index.ts` (~lines 362–370)

**Current:**
```ts
enterPlan: async (payload) => {
  let fileStem = payload.fileStem;
  if (fileStem === undefined || fileStem.length === 0) {
    const generator = new TopicGenerator(this);
    const topic = await generator.generate();
    const fallback = payload.kind === 'design' ? 'design' : 'plan';
    fileStem = `${topic ?? fallback}-${formatUtcTimestamp(new Date())}`;
  }
  await this.planMode.enter(undefined, undefined, undefined, payload.kind ?? 'plan', fileStem);
},
```

**Replace with:**
```ts
enterPlan: async (payload) => {
  // fileStem is now determined lazily inside PlanMode.enter (planId fallback).
  // Only pass through an explicit override.
  await this.planMode.enter(
    undefined,
    undefined,
    undefined,
    payload.kind ?? 'plan',
    payload.fileStem,
  );
},
```

### 2. `packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts` (~lines 51–61)

**Current:**
```ts
let topic: string | null = null;
if (_args.topic !== undefined) {
  topic = cleanupTopic(_args.topic);
} else {
  const generator = new TopicGenerator(this.agent);
  topic = await generator.generate();
}
if (topic === null) {
  topic = 'plan';
}
const fileStem = `${topic}-${formatUtcTimestamp(new Date())}`;
```

**Replace with:**
```ts
let fileStem: string | undefined;
if (_args.topic !== undefined) {
  const cleaned = cleanupTopic(_args.topic);
  if (cleaned !== null) {
    fileStem = cleaned; // PlanMode.enter stores it as _manualTopicSlug
  }
}
// When no manual topic is provided, leave fileStem undefined so
// PlanMode.enter falls back to planId (no LLM call).
```

*(Same pattern for `enter-design-mode.ts`, with fallback `'design'` if manual topic is null.)*

### 3. `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts` (~lines 120–144)

**Current order:**
```ts
const resolvedPlan = await this.resolvePlan();
// … telemetry …
const failed = this.exitPlanMode();
```

**New order:**
```ts
await this.agent.planMode.finalizeFileName();
const resolvedPlan = await this.resolvePlan();
// … telemetry …
const failed = this.exitPlanMode();
```

*(Same for `exit-design-mode.ts`.)*

### 4. `packages/agent-core/src/agent/plan/index.ts` — `enter()` (~lines 43–109)

Insert before `this._fileStem = fileStem ?? id;`:
```ts
let effectiveStem = fileStem;
if (!effectiveStem) {
  if (kind === 'plan' && this._lastDesignFileStem) {
    const designSlug = extractSlugFromDatedStem(this._lastDesignFileStem);
    effectiveStem = `${formatDatePrefix(new Date())}-${designSlug}`;
  }
}
if (fileStem) {
  this._manualTopicSlug = slugifyTitle(fileStem);
}
this._fileStem = effectiveStem ?? id;
```

And in `exit()` (~lines 157–174), insert before clearing fields:
```ts
if (this._kind === 'design' && this._fileStem) {
  this._lastDesignFileStem = this._fileStem;
}
this._manualTopicSlug = null;
```

### 5. `apps/ody-code/src/tui/commands/config.ts` (~lines 52–58 and 99–105)

**Current:**
```ts
host.showNotice(
  'Plan mode: ON',
  plan?.path !== undefined ? `Plan will be created here: ${plan.path}` : undefined,
);
```

**Replace with:**
```ts
host.showNotice('Plan mode: ON');
```

*(Same for Design mode.)*

---

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `finalizeFileName` read fails (missing/temp file) | Return existing `_SessionModeFilePath`; exit continues normally | File keeps temporary `planId` name | N/A — user still gets their content |
| `finalizeFileName` write to final path fails | Log warning, return temp path | File keeps temporary name | N/A — content is safe on disk |
| `finalizeFileName` collision detected | Append `-1`, `-2`, … to stem until unique | Filename gets a numeric suffix | N/A — content is preserved, filename is still semantic |
| Empty file on exit (no content written) | `finalizeFileName` returns early with temp path | File is not created (or empty temp remains) | N/A — expected if user exited without writing |
| No H1 heading in file | Fallback to `_manualTopicSlug` or `planId`: `YYYY-MM-DD-<slug>` | Filename uses manual topic or planId fallback | N/A — functional, just less semantic |

---

## Test Plan

### Unit tests to add/modify

1. **`packages/agent-core/test/agent/plan/topic-generator.test.ts`** — add tests for new pure helpers:
   - `extractFirstHeading('# Hello World')` → `'Hello World'`
   - `extractFirstHeading('## No H1\n# Yes')` → `'Yes'`
   - `extractFirstHeading('no heading')` → `null`
   - `extractFirstHeading('# **Bold** Title')` → `'Bold Title'`
   - `extractFirstHeading('# *Italic* _Plan_')` → `'Italic Plan'`
   - `slugifyTitle('Implement GLM Provider!')` → `'implement-glm-provider'`
   - `slugifyTitle('中文标题')` → `'中文标题'`
   - `slugifyTitle('a'.repeat(100))` → `'a'.repeat(50)`

2. **`packages/agent-core/test/agent/plan.test.ts`** — `finalizeFileName` tests (new describe block):
   - Given active plan mode with file content `# My Plan\n\ncontent`, calling `finalizeFileName` writes a new file with stem `YYYY-MM-DD-my-plan`.
   - Given a pre-existing file `plans/YYYY-MM-DD-my-plan.md`, `finalizeFileName` writes to `plans/YYYY-MM-DD-my-plan-1.md` instead.
   - Given empty file, `finalizeFileName` returns the temp path unchanged.
   - Given file with no H1 and no manual topic, `finalizeFileName` uses `YYYY-MM-DD-<planId>`.
   - Given file with no H1 but manual topic was provided, `finalizeFileName` uses `YYYY-MM-DD-<manual-slug>`.
   - Verify `_SessionModeFilePath` and `_fileStem` are updated after finalize.
   - Verify `isWritableAdvancedSessionModePath(finalPath)` returns `true` after finalize.

3. **`packages/agent-core/test/tools/enter-plan-mode.test.ts`** — update existing tests:
   - Remove mock expectation / assertion that `TopicGenerator.generate()` is called.
   - Assert that when no `topic` arg is given, `planMode.enter` is called with `fileStem` undefined.
   - Assert that when `topic` arg is given, sanitized topic is passed as `fileStem`.

4. **`packages/agent-core/test/tools/enter-design-mode.test.ts`** — same updates as above.

5. **`packages/agent-core/test/tools/exit-plan-mode.test.ts`** — add:
   - Assert that `finalizeFileName` is called before `resolvePlan`.
   - Assert exit message contains the final (renamed) path when a heading exists.
   - Assert exit message contains the collision-suffixed path when a collision occurs.

### Done criteria

```bash
# Typecheck entire tree
pnpm -r typecheck

# Run affected test suites
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/agent/plan.test.ts
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/tools/enter-plan-mode.test.ts
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/tools/enter-design-mode.test.ts
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/tools/exit-plan-mode.test.ts
pnpm --filter @odysseythink/agent-core test -- --run packages/agent-core/test/agent/plan/topic-generator.test.ts
```

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | `_SessionModeFilePath` rename happens after `resolvePlan` in a future refactor, causing the exit message to show the old temp path | Medium | Medium — user confusion | Code review + explicit ordering comment in `exit-plan-mode.ts` and `exit-design-mode.ts` |
| 2 | `findUniqueStem` loops indefinitely if every suffix up to a very large number is taken | Very Low | Low — infinite loop on exit | Cap suffix at a reasonable max (e.g. 1000) and fall back to `planId` if exceeded; or use a timestamp micro-suffix |
| 3 | LLM writes a heading with PII / secrets (e.g. `# Fix login for user@corp.com`) and it ends up in the filename | Low | Medium — sensitive data in filename | `slugifyTitle` does not strip email addresses; if this becomes a problem, reuse `cleanupTopic`'s sensitive-word filter on the extracted heading before slugifying |
| 4 | Design → Plan title reuse regex `^\d{4}-\d{2}-\d{2}-(.+)$` fails on stems that do not have a date prefix (e.g. manual topic override without date) | Medium | Low — falls back to using the whole stem | `extractSlugFromDatedStem` returns the full stem when no date prefix matches, which is harmless |
| 5 | Split-plan siblings remain with temp names while the index is renamed, breaking cross-file `Depends on:` references that use filenames | Low (split plans are rare) | Medium — broken plan structure | **Deferred to Out of scope**; if needed later, extend `finalizeFileName` to glob `<planId>-*.md` siblings and rename them too |
| 6 | Temp `planId.md` files accumulate in `plans/` or `designs/` directory over many sessions | Medium | Low — disk clutter | Periodically clean old temp files (e.g. files whose stem matches a hero-slug pattern and have a zero-byte or very small size). Deferred to a background maintenance task. |

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | Leaving temporary `planId.md` files on disk is acceptable because they are small and harmless. No user-visible cleanup is required in this change. | High | Low — minor disk clutter | Acceptable per user confirmation |
| 2 | Removing the TUI notice subtitle does not break any automated test that asserts on the exact notice text. | Medium | Low — test failure only | Run `apps/ody-code` tests after change |
| 3 | `TopicGenerator.generate()` is **only** invoked on mode entry; removing it from the two entry paths does not break other features. | High | Medium | Grep for all `TopicGenerator` / `generate()` call sites — verified: only `agent/index.ts` and the two enter tools |
| 4 | The regex `/^#\s+(.+)$/m` correctly captures the first H1 in all plan/design documents produced by the LLM, including those with front-matter or leading whitespace. | Medium | Low — falls back to `planId` slug | Test with real plan/design outputs; regex has `m` flag so `^` matches after newlines |
| 5 | `stripMarkdownFormatting` removes enough inline syntax to produce clean slugs without over-stripping legitimate characters (e.g. it does not remove hyphens or alphanumeric chars). | High | Low — slightly off slug | Verified with `node -e` on `# **Bold** Title`, `# *Italic* _Plan_`, `# [Link](x) Title` — all produce expected clean slugs |
| 6 | Users will not be confused by seeing `planId`-based paths in the LLM injection messages (`Plan file: plans/brave-fox-1234.md`) while in plan/design mode. | Medium | Low — cosmetic | The contract already says "Plan file: …"; the path is functional, not user-facing in TUI |
| 7 | Plan mode entry reusing `_lastDesignFileStem` only makes sense when the user explicitly transitions design → plan in the same session; we do not persist `_lastDesignFileStem` across session restarts. | High | Low — on restart, plan gets a fresh `planId` name, which is acceptable | `_lastDesignFileStem` is an in-memory field on `PlanMode`; session resume calls `restoreEnter` which does not touch it |
| 8 | `kaos.stat()` throws when the path does not exist, enabling collision detection via `try/catch`. | Medium | Low — collision check silently passes, risking overwrite | Verify `LocalKaos.stat` and `SSHKaos.stat` behaviour on missing files; both are expected to throw ENOENT-style errors |
