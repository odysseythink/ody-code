# Phase A: Completeness Gate

**Goal:** 扩展 `findMissingDesignSections` 覆盖设计文档 Part 3 的 7 项完整度检查（C1-C7），并在 System Reminder 中加入完整度清单指导 AI 在调用 ExitDesignMode 前自查。

**Architecture:** 纯函数 `findMissingDesignSections(content: string): string[]` 已存在（`exit-design-mode.ts:75`），只需扩展其正则匹配表。`ExitDesignModeTool.checkDesignCompleteness()` 已调用它，无需改动接线。System Reminder 在 `design-mode-contract.ts` 的 `STEP_5_EXIT` 段加入完整度检查清单。

---

## Task A1: Expand completeness checker to 7 criteria

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts:75-99` — expand `findMissingDesignSections`
- Test: `packages/agent-core/test/tools/exit-design-mode.test.ts:17-112` — add C3-C7 test cases

### Steps

- [ ] Write failing tests for C3-C7 (add to existing `describe('findMissingDesignSections', ...)` block):

```ts
// C3: missing Data Models section
it('detects missing Data Models section', () => {
  const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Algorithms
Algorithm details with enough content to exceed three hundred characters minimum.

## Error Handling
Error handling content.`;
  const result = findMissingDesignSections(design);
  expect(result).toContain('Data Models section');
});

// C4: missing Algorithms section
it('detects missing Algorithms section', () => {
  const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Error Handling
Error handling strategies and fallback paths.`;
  const result = findMissingDesignSections(design);
  expect(result).toContain('Algorithms section');
});

// C5: missing Error Handling section
it('detects missing Error Handling section', () => {
  const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode and control flow details with sufficient content.`;
  const result = findMissingDesignSections(design);
  expect(result).toContain('Error Handling section');
});

// C6: missing Self-Review section
it('detects missing Self-Review section', () => {
  const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail.`;
  const result = findMissingDesignSections(design);
  expect(result).toContain('Self-Review section');
});

// C7: missing User Approval marker
it('detects missing User Approval', () => {
  const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths.

## Self-Review
Security: checked X. Test: checked Y.`;
  const result = findMissingDesignSections(design);
  expect(result).toContain('User Approval');
});

// All 7 pass
it('returns empty for a design with all 7 criteria met', () => {
  const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor.

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
  expect(findMissingDesignSections(design)).toEqual([]);
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/tools/exit-design-mode.test.ts 2>&1 | tail -20
```

Expected: 6 new test failures with "Data Models section", "Algorithms section", "Error Handling section", "Self-Review section", "User Approval" NOT found in result.

- [ ] Write minimal implementation — extend `findMissingDesignSections` at `exit-design-mode.ts:75-99`:

```ts
export function findMissingDesignSections(content: string): string[] {
  const missing: string[] = [];
  const trimmed = content.trim();

  if (trimmed.length < 300) {
    missing.push('sufficient content (design appears incomplete or empty)');
  }

  const headingCount = (trimmed.match(/^## /gm) ?? []).length;
  if (headingCount < 3) {
    missing.push(`at least 3 design sections (found ${headingCount})`);
  }

  const scopePattern = /^#{1,3}\s+(scope|in\/out|范围|scope\s+in)/im;
  if (!scopePattern.test(trimmed)) {
    missing.push('Scope or Scope In/Out section');
  }

  const archPattern = /^#{1,3}\s+(architecture|design|approach|overview|架构|设计方案)/im;
  if (!archPattern.test(trimmed)) {
    missing.push('Architecture or Design section');
  }

  // C3: Data Models
  const dataModelsPattern = /^#{1,3}\s+(data\s*models?|数据模型|models?|data\s+&?\s*state)/im;
  if (!dataModelsPattern.test(trimmed)) {
    missing.push('Data Models section');
  }

  // C4: Algorithms
  const algorithmsPattern = /^#{1,3}\s+(algorithms?|算法|pseudocode|implementation\s+notes?)/im;
  if (!algorithmsPattern.test(trimmed)) {
    missing.push('Algorithms section');
  }

  // C5: Error Handling
  const errorHandlingPattern = /^#{1,3}\s+(error\s*handling|错误处理|errors?|degradation|failure\s+scenarios?)/im;
  if (!errorHandlingPattern.test(trimmed)) {
    missing.push('Error Handling section');
  }

  // C6: Self-Review
  const selfReviewPattern = /^#{1,3}\s+(self[- ]?review|自检|review|audit)/im;
  if (!selfReviewPattern.test(trimmed)) {
    missing.push('Self-Review section');
  }

  // C7: User Final Approval
  const userApprovalPattern = /^#{1,3}\s+(user\s+(final\s+)?approval|用户批准|批准状态|approved?)/im;
  if (!userApprovalPattern.test(trimmed)) {
    missing.push('User Approval');
  }

  return missing;
}
```

**Regex rationale — must-survive inputs verified against patterns:**

| Pattern | Must-survive input | Would pattern match it? |
|---------|-------------------|------------------------|
| `data\s*models?` | `## Data Model` | ✓ matches `data\s*model` |
| `data\s*models?` | `## Data & State` | ✓ matches `data\s+&?\s*state` |
| `algorithms?` | `## Algorithm` | ✓ matches `algorithms?` |
| `pseudocode` | `## Implementation Notes` | ✓ matches `implementation\s+notes?` |
| `error\s*handling` | `## Errors` | ✓ matches `errors?` |
| `error\s*handling` | `## Failure Scenarios` | ✓ matches `failure\s+scenarios?` |
| `self[- ]?review` | `## Self Review` | ✓ matches `self[- ]?review` |
| `review` | `## Review` | ✓ matches `review` |
| `user\s+(final\s+)?approval` | `## User Approval` | ✓ matches |
| `用户批准` | `## 用户批准` | ✓ matches |
| `approved?` | `## Approved` | ✓ matches |

**Anti-filter check:** None of the must-survive inputs contain substrings that would be caught by earlier/later patterns. Each pattern is independent and additive. The `review` pattern in C6 is intentionally broad (catches "Review" alone) but that's acceptable — a design document with a review section should pass, and the missing-list is only triggered if NO heading matches.

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/tools/exit-design-mode.test.ts 2>&1 | tail -10
```

Expected: all tests pass, including the 6 new C3-C7 tests.

- [ ] Verify existing ExitDesignModeTool integration tests still pass — the `COMPLETE_DESIGN` constant at line 114 in the test file already has all required sections (Scope, Architecture, Data Models, Error Handling, Assumptions). Add Algorithms, Self-Review, and User Approval to `COMPLETE_DESIGN` so the integration tests pass:

Edit `COMPLETE_DESIGN` at line 114:

```ts
const COMPLETE_DESIGN = `
## Scope In/Out

### Scope In
- Feature A [C:USER]

### Scope Out
- Feature B [C:DEFERRED]

## Architecture

The system uses X to accomplish Y. Call site: \`src/foo.ts:42\`.

## Data Models

\`\`\`ts
interface Foo { id: string; }
\`\`\`

## Algorithms

1. Parse input
2. Validate against schema
3. Transform and return

## Error Handling

| Error | Strategy |
|-------|----------|
| ENOENT | return null |

## Self-Review

Security: verified no secrets in paths. Test: all assertions traceable.

## User Final Approval

Approved by user [C:USER].
`.trim();
```

- [ ] Run full test suite for this file:

```bash
cd packages/agent-core && pnpm vitest run test/tools/exit-design-mode.test.ts
```

Expected: all tests pass (16+ tests total: 8 existing + 6 new C3-C7 + 2 existing integration).

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/planning/exit-design-mode.ts \
        packages/agent-core/test/tools/exit-design-mode.test.ts
git commit -m "feat: expand design completeness check to 7 criteria (C1-C7)"
```

---

## Task A2: Add completeness checklist to System Reminder

**Depends on:** Task A1

**Files:**
- Modify: `packages/agent-core/src/agent/injection/design-mode-contract.ts:112` — update `STEP_5_EXIT`
- Modify: `packages/agent-core/src/agent/injection/design-mode-contract.ts:138` — update `SPARSE_QUALITY_POINTER`
- Test: `packages/agent-core/test/agent/injection/design-mode.test.ts:67-95` — verify checklist presence in full reminder
- Test: `packages/agent-core/test/agent/injection/design-mode.test.ts:230-243` — verify checklist in sparse reminder

### Steps

- [ ] Write failing test — add to `describe('DesignModeInjector content', ...)` block in `design-mode.test.ts` at line 95:

```ts
it('includes the 7-item completeness checklist in the full reminder', async () => {
  const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
  const injector = new DesignModeInjector(agent);

  await injector.inject();
  const text = lastReminder(agent);

  for (const marker of [
    '7-item completeness checklist',
    'Scope In/Out',
    'Architecture',
    'Data Models',
    'Algorithms',
    'Error Handling',
    'Self-Review',
    'User Final Approval',
    'do not accept a partial design',
  ]) {
    expect(text).toContain(marker);
  }
});

it('includes the completeness checklist pointer in the sparse reminder', async () => {
  const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
  const injector = new DesignModeInjector(agent);

  await injector.inject();
  const messages = history(agent);
  messages.push({ role: 'assistant' }, { role: 'assistant' });
  await injector.inject();

  const text = lastReminder(agent);
  expect(text).toContain('7-item completeness checklist');
});
```

- [ ] Run and verify FAILS:

```bash
cd packages/agent-core && pnpm vitest run test/agent/injection/design-mode.test.ts 2>&1 | tail -20
```

Expected: 2 failures — "7-item completeness checklist" not found.

- [ ] Write implementation — update `STEP_5_EXIT` in `design-mode-contract.ts:112`:

Replace line 112:
```ts
const STEP_5_EXIT = `## Step 5 — Exit for approval
Call ExitDesignMode. If the design offers a real choice between approaches, pass them as the \`options\` parameter so the user can select one at approval time. After approval, design mode turns OFF and your ONLY next move is to recommend the user run /plan — do NOT begin implementing.`;
```

With:
```ts
const STEP_5_EXIT = `## Step 5 — Exit for approval

### Design Completeness Gate (BLOCKING — check before calling ExitDesignMode)
Before you call ExitDesignMode, the design file MUST pass this 7-item completeness checklist. ExitDesignMode will REJECT an incomplete design and list what is missing. Do NOT accept a partial design — if the user says "that's enough" before all 7 items are covered, push back.

  1. **Scope / Scope In/Out** — what is covered and what is explicitly deferred, with reasons.
  2. **Architecture / Design** — data-flow arrows, component relationships, caller→callee paths.
  3. **Data Models** — every exported interface/type/function with full type signatures + one-line contract.
  4. **Algorithms** — language-agnostic pseudocode for each non-trivial algorithm (not prose, not production code).
  5. **Error Handling** — error class → immediate handling → degradation path → recovery condition table.
  6. **Self-Review** — four-lens findings (Security / Test / Ops / Integration) written to the file.
  7. **User Final Approval** — the user has explicitly approved the design (not just "looks good").

Call ExitDesignMode. If the design offers a real choice between approaches, pass them as the \`options\` parameter so the user can select one at approval time. After approval, design mode turns OFF and your ONLY next move is to recommend the user run /plan — do NOT begin implementing.`;
```

Update `SPARSE_QUALITY_POINTER` at line 138 to add the checklist mention:

Replace line 138:
```ts
const SPARSE_QUALITY_POINTER = `Reminder: the design file must follow the fidelity rubric (Scope In/Out, data-flow arrows, typed interfaces, per-algorithm language-agnostic pseudocode (not production code), call-sites with file path + line range, an error/degradation table, test assertions, and a risk register), and you MUST run the self-review + post-write audit gate (scaled to the recorded audit level) before ExitDesignMode — that gate lists each [C:INFERRED] assumption verbatim for per-item sign-off and blocks ExitDesignMode until done, and a user-named target (a specific binary/path) must not be silently retargeted.`;
```

With:
```ts
const SPARSE_QUALITY_POINTER = `Reminder: the design file must pass the 7-item completeness checklist (Scope, Architecture, Data Models, Algorithms, Error Handling, Self-Review, User Final Approval) — ExitDesignMode REJECTS incomplete designs — follow the fidelity rubric (data-flow arrows, typed interfaces, per-algorithm language-agnostic pseudocode (not production code), call-sites with file path + line range, an error/degradation table, test assertions, and a risk register), and you MUST run the self-review + post-write audit gate (scaled to the recorded audit level) before ExitDesignMode — that gate lists each [C:INFERRED] assumption verbatim for per-item sign-off and blocks ExitDesignMode until done, and a user-named target (a specific binary/path) must not be silently retargeted.`;
```

- [ ] Also verify entry-message test — the entry message and full reminder share the contract. The test at line 109 (`'keeps the entry message and the full reminder in sync'`) checks for `'Step 4.5'`. We need to also check for `'completeness checklist'` in that test:

Edit the test at line 118-127 — add `'completeness checklist'` and `'User Final Approval'` to the `markers` array:

```ts
for (const marker of [
  'Step 0 — Audit strategy gate',
  'Step 0.5 — Upstream inventory',
  'Call-site integration',
  'Step 4.5',
  '[C:UPSTREAM]',
  'completeness checklist',
  'User Final Approval',
]) {
```

- [ ] Run and verify PASSES:

```bash
cd packages/agent-core && pnpm vitest run test/agent/injection/design-mode.test.ts
```

Expected: all tests pass, including the 2 new checklist tests and the updated sync test.

- [ ] Run whole-tree typecheck to ensure no broken callers:

```bash
pnpm -r typecheck 2>&1 | tail -5
```

Expected: no type errors.

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/injection/design-mode-contract.ts \
        packages/agent-core/test/agent/injection/design-mode.test.ts
git commit -m "feat: add 7-item completeness checklist to design mode system reminders"
```

---

## Self-Review (completeness.md)

- [x] 1. Spec-coverage table: C1-C7 all covered by Task A1 (checker) + Task A2 (reminder). Part 6 T1-T8 tests covered by A1 test cases.
- [x] 2. Placeholder scan: no TODO/TBD — all patterns, test code, and commit messages are concrete.
- [x] 3. No phantom tasks: A1 produces real implementation changes; A2 produces real reminder text changes.
- [x] 4. Dependency soundness: A2 → A1 (reminder references checks A1 defines). No forward refs.
- [x] 5. Caller & build soundness: `findMissingDesignSections` signature is unchanged — only body expanded. `STEP_5_EXIT` and `SPARSE_QUALITY_POINTER` are internal constants in `design-mode-contract.ts`, consumed by `designModeFullReminder`/`designModeSparseReminder` — both compose from fragments automatically, no signature changes. A2 ends with `pnpm -r typecheck`.
- [x] 6. Test-the-risk: Regex patterns verified against must-survive inputs in the rationale table above. Each pattern tested with a dedicated "detects missing X" test AND a comprehensive "all 7 pass" test. `COMPLETE_DESIGN` updated to include all 7 sections so integration tests exercise the full gate. No false positives from overlapping patterns — each pattern matches distinct heading names.
- [x] 7. Type consistency: No new types introduced. `findMissingDesignSections` return type `string[]` unchanged. Reminder text is pure string constants.
