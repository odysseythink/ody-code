# Epic A — A0: Internal Reuse Scan + C8 Reuse Analysis Gate Implementation Plan

**Goal:** Add an internal reuse-scan step (Step 0.6) and a hard Reuse Analysis exit gate (C8) to design mode, so every approved design explicitly records existing-code reuse candidates before handoff to plan mode.

**Architecture:** Extend `packages/agent-core/src/agent/injection/design-mode-contract.ts` with a new `STEP_0_6_REUSE` fragment and bump the exit checklist to C1-C8; enforce C8 in `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts` by detecting a `## Reuse Analysis` heading; keep entry/exit tool docs and the two affected test files in sync. No new runtime data structures or persistent state are introduced.

**Tech Stack:** TypeScript, Vitest, pnpm workspace (`packages/agent-core`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| File | Responsibility |
|------|----------------|
| `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts` | Add C8 regex to `findMissingDesignSections()` after the C7 block. |
| `packages/agent-core/test/tools/exit-design-mode.test.ts` | Add a missing-Reuse-Analysis test and update every "complete" fixture to include `## Reuse Analysis`. |
| `packages/agent-core/src/agent/injection/design-mode-contract.ts` | Add `STEP_0_6_REUSE`, insert it into `contractBody()`, bump `DESIGN_EXIT_CHECKLIST` to C1-C8, update `SPARSE_QUALITY_POINTER`, and refresh the reentry reminder. |
| `packages/agent-core/src/tools/builtin/planning/enter-design-mode.md` | Mention the internal reuse scan in the "What Happens in Design Mode" list. |
| `packages/agent-core/src/tools/builtin/planning/exit-design-mode.md` | Add Reuse Analysis to the required-sections list. |
| `packages/agent-core/test/agent/injection/design-mode.test.ts` | Update the checklist test to C1-C8 and add Step 0.6 marker assertions. |
| `.changeset/feat-reuse-analysis-gate.md` | Record the minor bump for `agent-core` and the CLI bundle. |

## Dependency Overview

```
Task 1 ─┬─► Task 2 ─┬─► Task 3
        │           │
        │           └─► Task 4
        │
        └───────────────► Task 5 ──► Task 6
```

- **Task 1** (C8 gate + `exit-design-mode.test.ts`) has no prerequisites.
- **Task 2** (contract text) can run independently of Task 1 but precedes the tests/docs that assert its text.
- **Task 3** (tool docs) depends on Task 2 because the docs describe the contract text.
- **Task 4** (`design-mode.test.ts`) depends on Task 2 because it asserts the new contract markers.
- **Task 5** (whole-tree verification) depends on Tasks 1, 3, and 4.
- **Task 6** (changeset) depends on Task 5 because the changeset is the final bookkeeping step after tests pass.

## Risks & Open Questions

| # | Risk | Mitigation |
|---|------|------------|
| 1 | Existing complete-design fixtures in `exit-design-mode.test.ts` break as soon as C8 is enforced. | Update all fixtures in the same task that adds the gate. |
| 2 | C8 regex is too strict (blocks valid heading variants) or too loose (matches unrelated headings). | Unit-test English/Chinese variants and adversarial near-misses in Task 1. |
| 3 | Agents bypass the gate with a vacuous `## Reuse Analysis` section. | The gate only checks presence; content quality is left to the adversarial self-review step and future Epic A work. |

### Task 1: Enforce C8 Reuse Analysis in the exit gate

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts:122-128`
- Test: `packages/agent-core/test/tools/exit-design-mode.test.ts:18-40, 83-105, 107-129, 131-153, 249-272, 281-319`

- [ ] Write the failing tests. Insert the following three tests after the existing C7 test (around line 247):

```ts
  // C8: missing Reuse Analysis section
  it('detects missing Reuse Analysis section', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Reuse Analysis section');
  });

  it('rejects adversarial ReuseAnalysis heading without space', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## ReuseAnalysis
This heading is missing the required space and must not satisfy C8.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Reuse Analysis section');
  });

  it('accepts English and Chinese Reuse Analysis headings', () => {
    const english = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## Reuse Analysis
Existing validation helper can be reused.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;

    const chinese = `## 范围
内容内容内容。Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## 架构
设计架构部分。Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation.

## 数据模型
更多内容以满足最小长度要求。Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

## 算法
算法伪代码内容，满足最小长度要求。

## 错误处理
错误处理策略和降级路径，包含足够细节。

## 复用分析
无现成组件可复用。

## 自检
Security: checked X. Test: checked Y. Ops: verified Z.

## 用户批准
已批准。`;

    expect(findMissingDesignSections(english)).toEqual([]);
    expect(findMissingDesignSections(chinese)).toEqual([]);
  });
```

- [ ] Run the new tests and verify they FAIL:

```bash
npx vitest run packages/agent-core/test/tools/exit-design-mode.test.ts
```

Expected failure: the first two tests report `expected [] to contain 'Reuse Analysis section'`. The third test passes because `findMissingDesignSections` currently returns `[]` for any design that satisfies C1-C7.

- [ ] Write the minimal implementation. Insert the C8 block immediately after the C7 block in `findMissingDesignSections` (`packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts:126`):

```ts
  // C8: Reuse Analysis
  const reuseAnalysisPattern = /^#{1,3}\s+(?:reuse\s+analysis|复用分析|component\s+reuse|existing\s+components?)(?:\s|$|[\u4e00-\u9fa5])/im;
  if (!reuseAnalysisPattern.test(trimmed)) {
    missing.push('Reuse Analysis section');
  }
```

- [ ] Update every "complete" fixture in the same test file so the existing tests pass again. Add a `## Reuse Analysis` section immediately before each `## Self-Review` section:

  - First complete fixture (line ~18): add
    ```
    ## Reuse Analysis
    No reusable components identified; greenfield design.
    ```
  - `"accepts "Design" as alternative"` fixture (line ~84): add the same block.
  - `"accepts "Approach" as alternative"` fixture (line ~107): add the same block.
  - Chinese architecture fixture (line ~131): add
    ```
    ## 复用分析
    无现成组件可复用。
    ```
  - `"All 7 pass"` fixture (line ~249): add the English Reuse Analysis block and change the comment to `// All 8 pass`.
  - `COMPLETE_DESIGN` constant (line ~281): add
    ```markdown
    ## Reuse Analysis

    - Existing `findMissingDesignSections` in `exit-design-mode.ts` can be extended to detect the section. [C:UPSTREAM]
    ```

- [ ] Run the test file again and verify everything PASSES:

```bash
npx vitest run packages/agent-core/test/tools/exit-design-mode.test.ts
```

Expected: all tests green.

- [ ] Commit: `git add packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts packages/agent-core/test/tools/exit-design-mode.test.ts && git commit -m "feat(agent-core): enforce C8 Reuse Analysis gate in ExitDesignMode"`.

### Task 2: Add Step 0.6 and C1-C8 checklist to the design-mode contract

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/injection/design-mode-contract.ts:40-48, 112-120, 150, 153-168, 241-244`

This task changes prompt text, not a shared TypeScript signature, so it does not require a caller sweep beyond the tests updated in Task 4.

- [ ] Add the new `STEP_0_6_REUSE` fragment immediately after `STEP_0_5_UPSTREAM` (around line 46):

```ts
const STEP_0_6_REUSE = `## Step 0.6 — Internal reuse scan (before proposing new code)
Before you design new components, functions, or data structures, scan the existing codebase for code that already solves the same problem or a substantially similar one. Use Read, Grep, Glob, or \`Agent(subagent_type="explore")\` for non-trivial searches. For each candidate:
  1. Record the file path and the function/type/module that could be reused.
  2. Decide whether it can be used as-is, adapted, or should be replaced.
  3. If no reusable candidate exists, explicitly note "greenfield — no reusable component found".
Write the findings to a \`## Reuse Analysis\` section in the design file. This section is a hard exit gate (C8).`;
```

- [ ] Insert `STEP_0_6_REUSE` into `contractBody` after `STEP_0_5_UPSTREAM`:

```ts
function contractBody(mockupAvailable: boolean): string {
  return [
    HARD_GATE,
    STEP_0_AUDIT,
    STEP_0_5_UPSTREAM,
    STEP_0_6_REUSE,
    STEP_1_CLARIFY,
    STEP_2_PROPOSE,
    STEP_3_PRESENT,
    STEP_4_WRITE,
    DESIGN_INCREMENTAL_AND_SPLIT,
    STEP_4_5_REVIEW_AUDIT,
    STEP_5_EXIT,
    visualCompanion(mockupAvailable),
    TURN_DISCIPLINE,
  ].join('\n\n');
}
```

- [ ] Replace `DESIGN_EXIT_CHECKLIST` (around line 112) with the C1-C8 version:

```ts
const DESIGN_EXIT_CHECKLIST = `ExitDesignMode completeness checklist (C1-C8) — the design file MUST contain all of the following before you call ExitDesignMode:
  - C1. Scope In/Out — what is in scope and explicitly deferred.
  - C2. Architecture / Design — components, data flow, typed interfaces.
  - C3. Data Models — new data structures, persistence, lifecycle.
  - C4. Algorithms — language-agnostic pseudocode for each non-trivial piece of logic.
  - C5. Error Handling — failure scenarios, fallback, retry, degradation path.
  - C6. Self-Review — the four-lens findings written to a \`## Self-Review\` section.
  - C7. User Final Approval — a \`## User Final Approval\` section recording the approval state.
  - C8. Reuse Analysis — a \`## Reuse Analysis\` section listing existing-code reuse candidates or explicitly stating greenfield.
If any item is missing, return to the corresponding Step and add it before calling ExitDesignMode.`;
```

- [ ] Replace `SPARSE_QUALITY_POINTER` (around line 150) with the version that mentions Step 0.6 and C1-C8:

```ts
const SPARSE_QUALITY_POINTER = `Reminder: the design file must follow the fidelity rubric (Scope In/Out, data-flow arrows, typed interfaces, per-algorithm language-agnostic pseudocode (not production code), call-sites with file path + line range, an error/degradation table, test assertions, and a risk register), and you MUST run the self-review + post-write audit gate (scaled to the recorded audit level) before ExitDesignMode — that gate lists each [C:INFERRED] assumption verbatim for per-item sign-off and blocks ExitDesignMode until done, and a user-named target (a specific binary/path) must not be silently retargeted. Before proposing new code, run the Step 0.6 internal reuse scan and record candidates in a \`## Reuse Analysis\` section. Before ExitDesignMode, verify the C1-C8 completeness checklist is satisfied: C1. Scope In/Out, C2. Architecture, C3. Data Models, C4. Algorithms, C5. Error Handling, C6. Self-Review, C7. User Final Approval, and C8. Reuse Analysis.`;
```

- [ ] Update the re-entry reminder step 5 (around line 241) to mention the reuse scan:

Replace
```
  5. Clarify any newly-required decisions one question per turn (seven-dimension checklist); verify any data source / hook point the design relies on actually exists in code; if the request names a concrete target, design THERE — do not silently retarget.
```
with
```
  5. Run an internal reuse scan for existing components before proposing new ones and record the findings in a \`## Reuse Analysis\` section; clarify any newly-required decisions one question per turn (seven-dimension checklist); verify any data source / hook point the design relies on actually exists in code; if the request names a concrete target, design THERE — do not silently retarget.
```

- [ ] Build / manual verification:

```bash
npx tsc -p packages/agent-core/tsconfig.json --noEmit
```

Expected: no type errors.

```bash
grep -n "Step 0.6 — Internal reuse scan" packages/agent-core/src/agent/injection/design-mode-contract.ts
grep -n "C1-C8" packages/agent-core/src/agent/injection/design-mode-contract.ts
grep -n "Reuse Analysis" packages/agent-core/src/agent/injection/design-mode-contract.ts
```

Expected: each grep returns at least one hit.

- [ ] Commit: `git add packages/agent-core/src/agent/injection/design-mode-contract.ts && git commit -m "feat(agent-core): add Step 0.6 reuse scan and C1-C8 exit checklist to design mode contract"`.

### Task 3: Sync enter/exit design-mode tool docs

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/planning/enter-design-mode.md:17-22`
- Modify: `packages/agent-core/src/tools/builtin/planning/exit-design-mode.md:12-17`

These are non-testable user-facing docs; they get complete code, a typecheck build step, and a manual grep verification.

- [ ] Update step 2 in `enter-design-mode.md`:

Replace
```markdown
2. Investigate the codebase with read-only tools (Read, Grep, Glob). Use `Agent(subagent_type="explore")` for non-trivial investigation. Use Bash only when needed.
```
with
```markdown
2. Investigate the codebase with read-only tools (Read, Grep, Glob). As part of that investigation, run an internal reuse scan: look for existing functions, types, or modules that already solve the problem. Use `Agent(subagent_type="explore")` for non-trivial investigation. Use Bash only when needed.
```

- [ ] Add Reuse Analysis to the required-sections list in `exit-design-mode.md`:

Replace
```markdown
## Required sections (must be present in the design file before calling)
- **Scope** — a `## Scope`, `### Scope In/Out`, or equivalent heading with in/out lists
- **Architecture / Design** — an `## Architecture`, `## Design`, `## Approach`, or equivalent
- At least **3 total `##` sections** and **300 characters** of substantive content
```
with
```markdown
## Required sections (must be present in the design file before calling)
- **Scope** — a `## Scope`, `### Scope In/Out`, or equivalent heading with in/out lists
- **Architecture / Design** — an `## Architecture`, `## Design`, `## Approach`, or equivalent
- **Reuse Analysis** — a `## Reuse Analysis` section listing existing-code reuse candidates (or an explicit greenfield note)
- At least **3 total `##` sections** and **300 characters** of substantive content
```

- [ ] Build / manual verification:

```bash
npx tsc -p packages/agent-core/tsconfig.json --noEmit
```

Expected: no type errors (the `.md` files are imported as raw strings; their content does not affect types).

```bash
grep -n "internal reuse scan" packages/agent-core/src/tools/builtin/planning/enter-design-mode.md
grep -n "Reuse Analysis" packages/agent-core/src/tools/builtin/planning/exit-design-mode.md
```

Expected: each grep returns at least one hit.

- [ ] Commit: `git add packages/agent-core/src/tools/builtin/planning/enter-design-mode.md packages/agent-core/src/tools/builtin/planning/exit-design-mode.md && git commit -m "docs(agent-core): document Step 0.6 reuse scan and C8 Reuse Analysis gate"`.

### Task 4: Update design-mode injection tests for Step 0.6 and C1-C8

**Depends on:** Task 2

**Files:**
- Test: `packages/agent-core/test/agent/injection/design-mode.test.ts:109-157`

- [ ] Update the shared-contract test marker list (around line 118) to include the new step:

Replace
```ts
    for (const marker of [
      'Step 0 — Audit strategy gate',
      'Step 0.5 — Upstream inventory',
      'Call-site integration',
      'Step 4.5',
      '[C:UPSTREAM]',
    ]) {
```
with
```ts
    for (const marker of [
      'Step 0 — Audit strategy gate',
      'Step 0.5 — Upstream inventory',
      'Step 0.6 — Internal reuse scan',
      'Call-site integration',
      'Step 4.5',
      '[C:UPSTREAM]',
    ]) {
```

- [ ] Update the checklist test (around line 131). Change the test name and marker list:

Replace
```ts
  it('carries the C1-C7 exit checklist in the entry message, full reminder, and sparse reminder', async () => {
```
with
```ts
  it('carries the C1-C8 exit checklist in the entry message, full reminder, and sparse reminder', async () => {
```

Replace
```ts
      for (const marker of [
        'C1. Scope In/Out',
        'C2. Architecture',
        'C3. Data Models',
        'C4. Algorithms',
        'C5. Error Handling',
        'C6. Self-Review',
        'C7. User Final Approval',
      ]) {
```
with
```ts
      for (const marker of [
        'C1. Scope In/Out',
        'C2. Architecture',
        'C3. Data Models',
        'C4. Algorithms',
        'C5. Error Handling',
        'C6. Self-Review',
        'C7. User Final Approval',
        'C8. Reuse Analysis',
      ]) {
```

- [ ] Run the test file and verify it PASSES:

```bash
npx vitest run packages/agent-core/test/agent/injection/design-mode.test.ts
```

Expected: all tests green, including the updated C1-C8 checklist test.

- [ ] Commit: `git add packages/agent-core/test/agent/injection/design-mode.test.ts && git commit -m "test(agent-core): assert Step 0.6 and C1-C8 checklist in design-mode reminders"`.

### Task 5: Whole-tree typecheck and full `agent-core` test run

**Depends on:** Task 1, Task 3, Task 4

**Files:**
- Verify: all files changed in Tasks 1-4

No source-code signatures were changed (`findMissingDesignSections` keeps the same interface), so there is no caller sweep. This task confirms the whole workspace still typechecks and the full `agent-core` suite passes.

- [ ] Run the workspace typecheck:

```bash
pnpm -r run typecheck
```

Expected: no `error TS` output.

- [ ] Run the full `agent-core` test suite:

```bash
npx vitest run packages/agent-core
```

Expected: all tests pass.

- [ ] Commit (if any fixes were needed): `git add -A && git commit -m "chore(agent-core): whole-tree verification for C8 reuse analysis gate"`. If no fixes were needed, this task produces a verified state and no empty commit is required.

### Task 6: Add changeset

**Depends on:** Task 5

**Files:**
- Create: `.changeset/feat-reuse-analysis-gate.md`

The change touches `@odysseythink/agent-core` source and enters the `@odysseythink/kimi-code` CLI bundle, so both packages are listed with a `minor` bump. The change is backwards-compatible for CLI users (design-mode workflow enhancement) and does not rename/remove commands or arguments.

- [ ] Create `.changeset/feat-reuse-analysis-gate.md`:

```markdown
---
"@odysseythink/agent-core": minor
"@odysseythink/kimi-code": minor
---

Add a Reuse Analysis hard gate to design-mode exit and an internal reuse-scan step to the design workflow.
```

- [ ] Verify the changeset is recognized:

```bash
git status
```

Expected: `.changeset/feat-reuse-analysis-gate.md` appears as an untracked or staged file.

```bash
cat .changeset/feat-reuse-analysis-gate.md
```

Expected: the file contains both package entries, `minor` bumps, and the English changelog sentence.

- [ ] Commit: `git add .changeset/feat-reuse-analysis-gate.md && git commit -m "chore: add changeset for C8 Reuse Analysis gate"`.

## Spec-Coverage Table

| Approved Design Requirement | Task(s) | Status |
|---|---|---|
| Add Step 0.6 — Internal reuse scan to the workflow contract | Task 2 | covered |
| Extend exit checklist from C1-C7 to C1-C8 (Reuse Analysis hard gate) | Task 1 (enforcement), Task 2 (checklist text) | covered |
| Enforce C8 in `ExitDesignModeTool.findMissingDesignSections()` via heading regex | Task 1 | covered |
| Keep sparse/reentry reminders and tool docs in sync | Task 2 (sparse/reentry), Task 3 (tool docs) | covered |
| Update `exit-design-mode.test.ts` fixtures and add a missing-Reuse-Analysis test | Task 1 | covered |
| Sweep other test fixtures/assertions that may break due to the new required section | Task 4 (`design-mode.test.ts`), Task 5 (full suite) | covered |
| Runtime micro-agent / knowledge injection (Epic A — A2) | — | no-op |
| Semantic code search / repo-map (Backlog T3-A) | — | no-op |
| Simplification ladder skill (Epic A — A1) | — | no-op |
| Over-design review/audit skill (Epic A — A3) | — | no-op |
| Technical-debt ledger (Epic A — A4) | — | no-op |
| New persistent data store, flag registry, or telemetry event | — | no-op |

## Self-Review

- [ ] 1. Spec-coverage table: every approved design requirement maps to a task or is explicitly marked `no-op`; no GAPs remain.
- [ ] 2. Placeholder scan: the plan contains no `TODO`, `TBD`, "implement later", or dead-code placeholders; every step has concrete code, commands, and expected output.
- [ ] 3. No phantom tasks: each of the six tasks produces a verifiable change; there are no `--allow-empty` or "already done in Task N" commits.
- [ ] 4. Dependency soundness: every `Depends on:` references an earlier task; no task uses a symbol or text fragment that a later task defines.
- [ ] 5. Caller & build soundness: no shared TypeScript signature is changed (`findMissingDesignSections` keeps the same interface), so no caller sweep is required. The final task runs a whole-workspace typecheck (`pnpm -r run typecheck`) and the full `agent-core` test suite.
- [ ] 6. Test-the-risk: Task 1 tests the C8 regex with a missing-section case, an adversarial `ReuseAnalysis` near-miss, and must-survive English/Chinese headings; the assertions trace directly to the regex constants defined in the same task.
- [ ] 7. Type consistency: all types, signatures, and property names used in later tasks match the earlier tasks (no signature changes; checklist/fragment names are introduced in Task 2 and consumed in Tasks 3 and 4).

