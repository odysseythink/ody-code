# Epic A — A0: Internal Reuse Scan + C8 Reuse Analysis Gate

**Document Type**: Design (for implementation planning)
**Status**: Draft (pending approval)
**Audit Level**: Deep [C:USER]
**Reference**: `.ody-code/roadmaps/design-mode: internal reuse-discovery step + Reuse Analysis gate.md` [C:USER]

---

## Scope In/Out

### In Scope
1. Add Step 0.6 — Internal reuse scan to the design-mode workflow contract [C:USER].
2. Extend the design-mode exit checklist from C1–C7 to C1–C8 by adding a **Reuse Analysis** hard gate [C:USER].
3. Enforce C8 in `ExitDesignModeTool.findMissingDesignSections()` via heading regex [C:USER].
4. Keep sparse/reentry reminders and tool docs in sync [C:USER].
5. Update existing test fixtures and add a new missing-Reuse-Analysis test [C:USER].
6. Sweep other test fixtures/assertions that may break due to the new required section [C:INFERRED].

### Out of Scope (deferred)
- Runtime micro-agent / knowledge injection (Epic A — A2).
- Semantic code search / repo-map (Backlog T3-A).
- Simplification ladder skill (Epic A — A1).
- Over-design review/audit skill (Epic A — A3).
- Technical-debt ledger (Epic A — A4).
- Any new persistent data store, flag registry, or telemetry event.

### Placement
All changes land at the locations named in the reference roadmap; no silent retargeting [C:USER].

---

## Prior Art

The reference roadmap itself is the prior-art document inside this repository; it already contains line-level guidance for the 5 files to change [C:UPSTREAM]. No external open-source system is being ported; this is a local-only enhancement to the existing design-mode workflow.

---

## Architecture

```
User enters /design
        │
        ▼
EnterDesignModeTool reads enter-design-mode.md
        │
        ▼
DesignModeInjector re-injects design-mode-contract.ts fragments
        │
        ▼
Contract now includes STEP_0_6_REUSE (scan existing code → ask per candidate → write ## Reuse Analysis)
        │
        ▼
Agent writes design file with ## Reuse Analysis section
        │
        ▼
ExitDesignModeTool.findMissingDesignSections() runs C1–C8 regex checks
        │
        ▼
Missing "Reuse Analysis section"  ──►  blocking ExecutableToolErrorResult
Present                               else handoff to plan mode
```

Components:
- `design-mode-contract.ts`: single source of truth for the workflow text; owns `STEP_0_6_REUSE`, `DESIGN_EXIT_CHECKLIST` (now C1-C8), `SPARSE_QUALITY_POINTER`, and `designModeReentryReminder`.
- `exit-design-mode.ts`: gate implementation; `findMissingDesignSections()` gains C8 regex.
- Tool docs (`enter-design-mode.md`, `exit-design-mode.md`): user-facing explanation of the new step and required section.
- `exit-design-mode.test.ts`: regression coverage.

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|------------|------------|-----------------|---------------|
| 1 | [C:INFERRED] The reference roadmap's line-level guidance is still accurate (file paths, function names, fixture locations). | Medium | Implementer may edit wrong files or tests; gate may not land. | Verify by reading the 5 files before implementation. |
| 2 | [C:INFERRED] Existing tests for `ExitDesignModeTool` are the only tests that will break due to the new required section. | Medium | Other snapshot or integration tests may fail after the change. | Sweep `packages/agent-core/test` and `apps` for `findMissingDesignSections`, `ExitDesignMode`, `design-mode`, and `C1-C7` references before implementation. |
| 3 | [C:INFERRED] Agents will follow Step 0.6 and populate `## Reuse Analysis` with either real candidates or an explicit greenfield note. | Medium | Gate passes but section is vacuous; duplication still sneaks through. | Manual smoke test on a component-like request. |
| 4 | [C:INFERRED] A single heading regex is sufficient to detect the section across English and Chinese headings. | High | False negatives block valid exits; false positives impossible by design. | Unit-test the regex against `## Reuse Analysis`, `## 复用分析`, adversarial cases. |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|------------|--------|------------|
| 1 | Existing complete-design fixtures in unrelated tests break because they lack `## Reuse Analysis`. | Medium | CI fails broadly. | Sweep and update all fixtures; run full `packages/agent-core` suite. |
| 2 | C8 regex is too strict (rejects valid heading variants) or too loose (matches unrelated headings). | Low | Blocks valid exits or misses missing sections. | Include English/Chinese variants; adversarial self-review with concrete inputs. |
| 3 | Agents treat the new step as boilerplate and write vacuous greenfield notes to bypass the gate. | Medium | Duplication still occurs. | Pair with A2 runtime micro-agent and future semantic search; manual spot checks. |
| 4 | Step 0.6 text bloats the already long design-mode contract. | Low | Token cost increase; agent attention dilution. | Keep fragment concise; place it after Step 0.5. |



---

## Data Models

A0 introduces **no new runtime data structures or persistence** [C:INFERRED]. The only "data" changes are:

1. **Contract fragments** (`string`): new `STEP_0_6_REUSE` markdown fragment in `design-mode-contract.ts` [C:UPSTREAM].
2. **Checklist text** (`string`): `DESIGN_EXIT_CHECKLIST` updated from `(C1-C7)` to `(C1-C8)` [C:UPSTREAM].
3. **Regex pattern** (`RegExp`): new C8 heading detector in `findMissingDesignSections()` [C:UPSTREAM].

Signature of the existing gate function (unchanged interface):

```ts
function findMissingDesignSections(content: string): string[]
// Returns a list of human-readable missing-section names.
// Empty array means the design document satisfies C1-C8.
```

---

## Algorithms

### C8 Reuse Analysis heading detection

Location: `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts`, inside `findMissingDesignSections()` after the C7 block (~line 126) [C:USER].

Pseudocode:

```
function detectReuseAnalysisSection(trimmedDesign: string): boolean
  // Matches English and Chinese Reuse Analysis headings while avoiding
  // false positives such as "ReuseAnalysis" or "Reuse-Analysis".
  pattern := /^#{1,3}\s+(?:reuse\s+analysis|复用分析|component\s+reuse|existing\s+components?)(?:\s|$|[\u4e00-\u9fa5])/im
  return pattern.test(trimmedDesign)
```

Algorithm in `findMissingDesignSections()`:

```
function findMissingDesignSections(content: string): string[]
  missing := empty list
  trimmed := content.trim()

  // existing C1-C7 checks (unchanged)
  if length(trimmed) < 300 then push "sufficient content ..."
  if count(/^## /gm in trimmed) < 3 then push "at least 3 design sections ..."
  if not matchesScope(trimmed) then push "Scope or Scope In/Out section"
  if not matchesArchitecture(trimmed) then push "Architecture or Design section"
  if not matchesDataModels(trimmed) then push "Data Models section"
  if not matchesAlgorithms(trimmed) then push "Algorithms section"
  if not matchesErrorHandling(trimmed) then push "Error Handling section"
  if not matchesSelfReview(trimmed) then push "Self-Review section"
  if not matchesUserApproval(trimmed) then push "User Approval"

  // C8 (new)
  if not detectReuseAnalysisSection(trimmed) then
    push "Reuse Analysis section"

  return missing
```

### Contract composition

Location: `packages/agent-core/src/agent/injection/design-mode-contract.ts`, `contractBody()` [C:USER].

Pseudocode:

```
function contractBody(mockupAvailable: boolean): string
  fragments := [
    HARD_GATE,
    STEP_0_AUDIT,
    STEP_0_5_UPSTREAM,
    STEP_0_6_REUSE,        // new fragment inserted here
    STEP_1_CLARIFY,
    STEP_2_PROPOSE,
    STEP_3_PRESENT,
    STEP_4_WRITE,
    DESIGN_INCREMENTAL_AND_SPLIT,
    STEP_4_5_REVIEW_AUDIT,
    STEP_5_EXIT,
    visualCompanion(mockupAvailable),
    TURN_DISCIPLINE,
  ]
  return fragments.join("\n\n")
```

---

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|-------------|--------------------|------------------|--------------------|
| Missing `## Reuse Analysis` section | `findMissingDesignSections()` pushes `"Reuse Analysis section"`; `checkDesignCompleteness()` returns `ExecutableToolErrorResult` with the missing-item list. | ExitDesignMode is blocked; user/agent must add the section. | Design file contains a heading matching the C8 regex. |
| Section present but vacuous (e.g., empty table) | Gate cannot judge content quality; it only checks heading presence. | Low-quality section may pass gate but fail to prevent duplication. | Manual review + future A2 micro-agent / semantic search. |
| Existing test fixtures break after C8 | Test runner reports failures. | CI blocked until fixtures are updated. | Sweep and add `## Reuse Analysis` to every fixture that expects `findMissingDesignSections(...) === []`. |
| Contract text drift between entry/reentry/full/sparse variants | Single-source fragments in `design-mode-contract.ts` prevent this. | If fragments are duplicated elsewhere, text may diverge. | Centralize all Step 0.6 / C8 references in `design-mode-contract.ts`; docs point to it. |

---

## Call-Site Integration

### 1. Insert Step 0.6 fragment into contract body

**File**: `packages/agent-core/src/agent/injection/design-mode-contract.ts`
**Approx line range**: between `STEP_0_5_UPSTREAM` (line 40) and `STEP_1_CLARIFY` (line 48), and inside `contractBody()` (line 153-168) [C:USER].

```ts
const STEP_0_6_REUSE = `## Step 0.6 — Internal reuse scan ...`;

function contractBody(mockupAvailable: boolean): string {
  return [
    HARD_GATE,
    STEP_0_AUDIT,
    STEP_0_5_UPSTREAM,
    STEP_0_6_REUSE,        // insert
    STEP_1_CLARIFY,
    ...
  ].join('\n\n');
}
```

Before/after: the full and entry re-injections will now include Step 0.6 instructions.

### 2. Extend exit checklist to C8

**File**: same file
**Approx line range**: `DESIGN_EXIT_CHECKLIST` (line 112-120) [C:USER].

Change heading text `(C1-C7)` → `(C1-C8)` and append bullet:

```ts
const DESIGN_EXIT_CHECKLIST = `ExitDesignMode completeness checklist (C1-C8) — ...
  - C8. Reuse Analysis — a \`## Reuse Analysis\` section ...`;
```

Before/after: agents see C8 as a required exit criterion.

### 3. Enforce C8 in gate

**File**: `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts`
**Approx line range**: inside `findMissingDesignSections()` after C7 block (~line 127) [C:USER].

```ts
// C8: Reuse Analysis
const reusePattern = /^#{1,3}\s+(?:reuse\s+analysis|复用分析|component\s+reuse|existing\s+components?)(?:\s|$|[\u4e00-\u9fa5])/im;
if (!reusePattern.test(trimmed)) {
  missing.push('Reuse Analysis section');
}
```

Before/after: missing Reuse Analysis now blocks exit like C1-C7.

### 4. Sync sparse/reentry reminders

**File**: `packages/agent-core/src/agent/injection/design-mode-contract.ts`
**Approx line ranges**: `SPARSE_QUALITY_POINTER` (line 150), `designModeReentryReminder()` (line 229-251) [C:USER].

Update inline `C1…C7` list to `C1…C8` and add a clause about scanning for reusable components.

### 5. Tool docs

**Files**: `packages/agent-core/src/tools/builtin/planning/enter-design-mode.md` (line 17-22) and `exit-design-mode.md` (line 12-16) [C:USER].

- `enter-design-mode.md`: mention reuse scan in step 2.
- `exit-design-mode.md`: add Reuse Analysis to required-sections list.

### 6. Tests

**File**: `packages/agent-core/test/tools/exit-design-mode.test.ts` [C:USER].

- Add `## Reuse Analysis` section to every "complete" fixture (line 18-40, 84-153, 250-272, `COMPLETE_DESIGN`).
- Add new test: complete design minus `## Reuse Analysis` → expect result toContain `'Reuse Analysis section'`.

---

## Test Plan

### Unit tests (`exit-design-mode.test.ts`)

| # | Test | Exact assertion |
|---|------|-----------------|
| 1 | `findMissingDesignSections` returns empty for a complete design with Reuse Analysis | `expect(findMissingDesignSections(design)).toEqual([])` where design includes `## Reuse Analysis`. |
| 2 | Detects missing Reuse Analysis section | `expect(result).toContain('Reuse Analysis section')` for a design that is otherwise complete. |
| 3 | `ExitDesignModeTool` exits successfully when Reuse Analysis is present | `expect(result.isError).toBe(false)` and `expect(result.output).toContain('Design saved to')` using `COMPLETE_DESIGN` updated with Reuse Analysis. |
| 4 | `ExitDesignModeTool` rejects when Reuse Analysis is absent | `expect(result.isError).toBe(true)` and `expect(result.output).toContain('Reuse Analysis section')`. |
| 5 | Regex accepts English and Chinese heading variants | `findMissingDesignSections('## Reuse Analysis\n...')` and `findMissingDesignSections('## 复用分析\n...')` both return `[]`. |

### Sweep tests

| # | Check | Command |
|---|-------|---------|
| 6 | No remaining `C1-C7` / `C1–C7` references in tests | `grep -rn "C1[-–]C7" packages/agent-core/test` returns empty. |
| 7 | Find all fixtures that may need updating | `grep -rln "findMissingDesignSections\|ExitDesignMode\|design-mode" packages/agent-core/test apps`. |

### Done criteria

```bash
# Type check
npx tsc -p packages/agent-core/tsconfig.json --noEmit

# Targeted tests
npx vitest run packages/agent-core/test/tools/exit-design-mode

# Full agent-core suite
npx vitest run packages/agent-core
```

All of the above must pass.



---

## Self-Review

### Expensive decisions scrutinised

**Decision 1 — C8 heading regex.** This is the only non-trivial matching rule in A0. A false negative lets an agent exit without Reuse Analysis; a false positive blocks a valid exit.

Concrete cases verified with `node -e` (the final regex used in the design):

| Input | Expected | Actual | Verdict |
|-------|----------|--------|---------|
| `## Reuse Analysis\n...` | pass | pass | must-survive |
| `## 复用分析\n...` | pass | pass | must-survive |
| `### Reuse\n...` | fail | fail | must-reject (too vague) |
| `## ReuseAnalysis\n...` | fail | fail | adversarial typo |
| `## Reuse-Analysis\n...` | fail | fail | adversarial punctuation |
| `## Reuse Analysisasd\n...` | fail | fail | adversarial suffix |
| `#### Reuse Analysis\n...` | fail | fail | wrong heading level |
| `## Component reuse\n...` | pass | pass | acceptable variant |

The original reference regex allowed `### Reuse`, `## ReuseAnalysis`, and `## Reuse-Analysis` to pass because it included a bare `reuse` branch and `\s*` between "reuse" and "analysis". I tightened it to require a space for "reuse analysis", dropped the bare `reuse`/`复用` branches, and added a trailing delimiter check. This fixes the false positives without losing the required variants [C:INFERRED].

### Four-lens sweep

- **Security**: The regex only inspects the design file content; no external calls, no secret paths, no PII. No changes to permission model. Nothing found.
- **Test**: Every behaviour has a must-pass and must-reject case in the Test Plan. The regex was verified with adversarial inputs. Existing complete-design fixtures must be updated; a sweep command is included. Nothing found beyond the regex refinement above.
- **Ops**: No new persistent state, network calls, or background tasks. The regex is O(n) on design-file length and runs only on `ExitDesignMode`. No identifier collision risk. Nothing found.
- **Integration**: All insertion points were read and confirmed to exist:
  - `packages/agent-core/src/agent/injection/design-mode-contract.ts` — owns fragments, checklist, reminders.
  - `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts` — owns `findMissingDesignSections()`.
  - `packages/agent-core/src/tools/builtin/planning/enter-design-mode.md` and `exit-design-mode.md` — exist.
  - `packages/agent-core/test/tools/exit-design-mode.test.ts` — exists.
  No silent retargeting; all paths match the reference roadmap.
- **Scope**: This is a single coherent change to the design-mode workflow. It does not grow into multiple independent subsystems; no split needed.

---

## User Final Approval

- Audit level: Deep [C:USER]
- Key design decisions confirmed: C8 hard gate; unified section requirement with scoped semantic applicability; tightened regex; fixture updates [C:USER].
- All 7 [C:INFERRED] assumptions accepted by user [C:USER].
- Status: Approved for implementation planning. ExitDesignMode called.

