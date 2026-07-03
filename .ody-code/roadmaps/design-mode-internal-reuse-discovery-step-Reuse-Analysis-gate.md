# feat(design-mode): internal reuse-discovery step + Reuse Analysis gate (C8)

## Context

The user built three page modules with AI in design→plan→execute flow. Several components
across the three pages were highly similar and should have been **reused or extended** from
existing ones — but the AI silently built three near-duplicate copies.

Root cause: the design-mode contract's `Step 0.5` only covers (A) porting an *external* upstream
system and (B) a *web* prior-art search — and it **explicitly tells the agent to skip internal
changes** ("Skip this sub-step for purely internal changes"). There is **no step that makes the
agent scan our OWN codebase** for existing components/modules with high functional or visual
overlap before proposing new ones. So duplication is never even surfaced.

Fix: add an **internal reuse-scan step** to the design-mode workflow. Before proposing approaches,
the agent must search this codebase for overlapping frontend/backend components/modules, and for
each high-overlap candidate ask the user — per candidate, via AskUserQuestion — to choose
**Reuse-as-is / Extend / Build-new** (recorded as `[C:USER]`). The findings + decisions are
written to a mandatory `## Reuse Analysis` section, enforced as a new completeness criterion
**C8** that blocks `ExitDesignMode` (same hard-gate mechanism as C1–C7). Genuinely greenfield
work satisfies C8 with an explicit "no overlap found" note plus the search terms tried.

Decisions confirmed with the user: **hard gate** (required section, blocks exit) and
**per-candidate user choice** (AskUserQuestion during clarification).

This builds on the existing completeness-gate machinery (`findMissingDesignSections`) — the same
single source of truth that already enforces C1–C7.

---

## Design

### 1. New contract fragment — `STEP_0_6_REUSE`
`packages/agent-core/src/agent/injection/design-mode-contract.ts`

Add a fragment after `STEP_0_5_UPSTREAM` and insert it into `contractBody()` between
`STEP_0_5_UPSTREAM` and `STEP_1_CLARIFY`:

```ts
const STEP_0_6_REUSE = `## Step 0.6 — Internal reuse scan (before proposing approaches)
For ANY task that adds or changes UI components, modules, services, endpoints, or data shapes, BEFORE you propose approaches you MUST scan THIS codebase for existing components/modules whose function or appearance overlaps the request — do NOT design a new component until you have ruled out reuse. Search by feature keywords, component/prop names, similar routes, and sibling files with Grep/Glob/Read (use Agent(subagent_type="explore") for a non-trivial scan). For EACH candidate with meaningful overlap, surface a Reuse-as-is / Extend / Build-new choice to the user via AskUserQuestion — one candidate per turn, during Step 1 — and record the pick as [C:USER]; never silently duplicate an existing component. Write the findings to a \`## Reuse Analysis\` section (table: Candidate | Path | Overlap | Decision [reuse/extend/new] | Why). If a genuine scan finds nothing to reuse, the section MUST still exist and state "No overlapping components found — greenfield" together with the search terms you tried. This is internal-only and complements Step 0.5(B)'s external prior-art search.`;
```

### 2. Extend the completeness checklist to C8
Same file, `DESIGN_EXIT_CHECKLIST`:
- Change the heading `(C1-C7)` → `(C1-C8)`.
- Append:
  ```
    - C8. Reuse Analysis — a `## Reuse Analysis` section listing overlapping existing components and the reuse / extend / build-new decision for each (or "greenfield — no overlap" with the search terms tried).
  ```

### 3. Enforce C8 in the gate
`packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts`, inside
`findMissingDesignSections()` (after the C7 block, ~line 126):

```ts
// C8: Reuse Analysis
const reusePattern = /^#{1,3}\s+(reuse\s*analysis|reuse|复用分析|复用|component\s+reuse|existing\s+components?)/im;
if (!reusePattern.test(trimmed)) {
  missing.push('Reuse Analysis section');
}
```

No other change needed in the tool — `checkDesignCompleteness()` already turns any missing
item into the blocking `ExecutableToolErrorResult`.

### 4. Keep the sparse / reentry reminders in sync
Same contract file:
- `SPARSE_QUALITY_POINTER`: append `C8. Reuse Analysis` to its inline `C1…C7` list, and add a
  clause "scan the codebase for reusable/overlapping components before proposing new ones".
- `designModeReentryReminder`: add a short step reminding to redo/keep the `## Reuse Analysis`
  section when the request adds components.

### 5. Tool-doc updates
- `enter-design-mode.md` "What Happens in Design Mode" step 2: add "...and scan for existing
  components/modules that overlap the request; for each high-overlap match, ask the user whether
  to reuse, extend, or build new."
- `exit-design-mode.md` "Required sections" list: add
  `- **Reuse Analysis** — a \`## Reuse Analysis\` section recording overlapping existing components and the reuse/extend/build-new decision (or "greenfield — no overlap").`

---

## Files to Change

| File | Change |
|---|---|
| `packages/agent-core/src/agent/injection/design-mode-contract.ts` | Add `STEP_0_6_REUSE`; insert into `contractBody()`; update `DESIGN_EXIT_CHECKLIST` to C8; update `SPARSE_QUALITY_POINTER` + reentry reminder |
| `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts` | Add C8 regex check to `findMissingDesignSections()` |
| `packages/agent-core/src/tools/builtin/planning/enter-design-mode.md` | Mention reuse scan in step 2 |
| `packages/agent-core/src/tools/builtin/planning/exit-design-mode.md` | Add Reuse Analysis to required-sections list |
| `packages/agent-core/test/tools/exit-design-mode.test.ts` | Add `## Reuse Analysis` to every "complete" fixture; new "detects missing Reuse Analysis section" test |

### Test fixture updates (critical — C8 makes existing complete-design fixtures fail)
Every fixture that currently expects `findMissingDesignSections(...) === []` or drives a
successful exit must gain a `## Reuse Analysis` section. In
`test/tools/exit-design-mode.test.ts` that is: the `COMPLETE_DESIGN` constant (~line 281) and the
inline complete designs in the `findMissingDesignSections` describe ("returns empty…",
"accepts Design", "accepts Approach", "accepts Chinese 架构", "all 7 criteria met").
Add, e.g.:

```
## Reuse Analysis

| Candidate | Path | Overlap | Decision | Why |
|-----------|------|---------|----------|-----|
| FooCard | src/ui/foo-card.tsx:1 | layout | extend | add a variant prop [C:USER] |
```

Add one new unit test mirroring the C3–C7 pattern:
```
it('detects missing Reuse Analysis section', () => { /* complete design minus ## Reuse Analysis → expect result toContain 'Reuse Analysis section' */ });
```

### Sweep for other affected fixtures / assertions
Before running, grep the whole repo for fixtures and prompt-snapshot assertions that will move:
```bash
grep -rn "C1-C7\|C1–C7\|## Self-Review\|User Final Approval" packages/agent-core/test
grep -rln "findMissingDesignSections\|ExitDesignMode\|design-mode" packages/agent-core/test apps
```
Update any other "complete design" fixture (e.g. in `exit-design-mode-options.test.ts`,
`planning/exit-design-mode-telemetry.test.ts`) and any test that asserts the contract text /
`C1-C7` count. If a snapshot captures the design-mode injection prompt, update it.

---

## Verification

```bash
export PATH="$HOME/.nvm/versions/node/v24.16.0/bin:$PATH"
cd /Users/ranwei/workspace/ody-code

# Type check
npx tsc -p packages/agent-core/tsconfig.json --noEmit 2>&1 | grep -v "npm warn\|npm notice"

# Targeted tests
npx vitest run packages/agent-core/test/tools/exit-design-mode 2>&1 | tail -30

# Full agent-core suite (catches contract / injection snapshot drift)
npx vitest run packages/agent-core 2>&1 | tail -30
```

Manual smoke test:
1. `/design` a feature that adds a UI component similar to an existing one.
2. Confirm the agent runs an internal reuse scan and asks, per candidate, Reuse / Extend / Build-new.
3. Write a design WITHOUT a `## Reuse Analysis` section → `ExitDesignMode` must be rejected with
   "Reuse Analysis section" in the missing list.
4. Add the section (real candidates or an explicit "greenfield — no overlap" note) → exit succeeds.
