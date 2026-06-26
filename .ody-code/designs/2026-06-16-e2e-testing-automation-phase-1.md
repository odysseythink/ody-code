# E2E Testing Automation — Phase 1 Design

**Audit Level**: Deep  
**Target**: `packages/agent-core/src/e2e-testing/` + plan-mode integration  
**Date**: 2026-06-16

---

## Scope

### In Scope

1. Extensible E2E generator framework in `packages/agent-core/src/e2e-testing/` [C:USER].
2. TypeScript/Vitest generator for ody-code self-testing [C:USER].
3. File-level impact analysis with heuristic rules to detect affected builtin tools [C:USER].
4. Plan-mode integration: automatically append an E2E task to the approved plan markdown before it is handed off to normal mode [C:USER].
5. New builtin tool `RunE2ETests` to generate and execute temporary E2E tests [C:USER].
6. Configuration schema in `~/.ody-code/config.toml` under an `e2e` key [C:USER].
7. JSON test reports under `.ody-code/test-reports/` plus a markdown summary returned by the tool [C:USER].
8. Dog-food validation: `ExitPlanModeTool` gets its own generated E2E test and an end-to-end plan-mode flow test [C:USER].
9. Unit tests for registry, impact analysis, generator, and executor with ≥80% coverage [C:USER].
10. User guide at `.ody-code/docs/e2e-testing-guide.md` [C:USER].

### Out of Scope (deferred)

| Item | Reason |
|---|---|
| Python/Pytest and Node.js/Jest generators | Phase 2 deliverable [C:UPSTREAM] |
| Recursive/transitive dependency graph | Phase 2 enhancement [C:UPSTREAM] |
| Test result caching | Phase 2 optimization [C:UPSTREAM] |
| ML risk scoring, contract testing, mutation testing | Phase 3 exploration [C:UPSTREAM] |
| Auto-commit of generated tests | Explicitly deferred; tests are temporary artifacts only [C:USER] |
| Visual/TUI report UI | Out of Phase 1 scope; JSON + markdown only [C:INFERRED] |

---

## Architecture

### High-Level Data Flow

```
User Request
   │
   ▼
Design Mode ──► plan markdown (SessionMode.data().content)
   │
   ▼
ExitPlanModeTool.resolveExecution()
   │
   ├──► E2EPlanEnricher.enrich(planPath, planContent)
   │       ├──► ImpactAnalyzer.analyze(changedFiles, criticalTools)
   │       │       └── ImpactAnalysisResult { affectedTools[], priority }
   │       └──► writes enriched plan markdown back to disk
   │
   ▼
plan_review display (shows added E2E task)
   │
   ▼
User approves plan
   │
   ▼
handoffToNormal() ──► normal-mode context contains enriched plan
   │
   ▼
model / goal-mode loop
   │
   ├──► implements feature tasks
   │
   └──► RunE2ETestsTool.resolveExecution()
           │
           ├──► E2EConfigResolver.resolve(agent.config, projectRoot)
           │
           ├──► E2EGeneratorRegistry.detectAndGet(projectRoot)
           │       └── TypeScriptVitestGenerator
           │
           ├──► generator.generateTestsForFeature(feature)
           │       └── TestFile[] written to .ody-code/test-generated/e2e/
           │
           ├──► E2ETestExecutor.execute(testFiles, options)
           │       ├──► Kaos.exec('pnpm', 'vitest', 'run', ...)
           │       ├──► parses JSON / tap output
           │       └──► writes .ody-code/test-reports/e2e-report-<ts>.json
           │
           └──► returns markdown summary + status
```

### Component Map

| Component | Responsibility | Location (new) |
|---|---|---|
| `E2ETestGenerator` | Abstract generator interface | see `core.md` |
| `E2EGeneratorRegistry` | Detects project stack and selects generator | see `core.md` |
| `E2EConfigResolver` | Loads and validates `e2e` config | see `core.md` |
| `ImpactAnalyzer` | Maps changed files to affected builtin tools | see `core.md` |
| `TypeScriptVitestGenerator` | Generates TS/Vitest E2E tests | see `generator.md` |
| `E2ETestExecutor` | Runs tests in parallel and parses results | see `generator.md` |
| `E2EPlanEnricher` | Rewrites plan markdown to add E2E task | see `integration.md` |
| `RunE2ETestsTool` | Builtin tool exposed to the model | see `integration.md` |

---

## Data Models (Overview)

Detailed type signatures and persistence lifecycle live in the part files. At index level the key entities are:

- `E2EConfig`: loaded from TOML `[e2e]` section; fields include `enabled`, `strategy`, `criticalTools`, `failurePolicy`, `maxConcurrency`, `testTimeout`.
- `ImpactAnalysisResult`: `{ affectedTools: AffectedTool[] }` where `AffectedTool = { toolId, priority: 'critical' | 'important' | 'nice-to-have' }`.
- `Feature`: `{ toolId, changedFiles, projectRoot, description }`.
- `TestFile`: `{ path, content }`.
- `E2EExecutionResult`: `{ passed, failed, skipped, durationMs, reportPath, summary }`.

---

## Algorithms (Overview)

Detailed pseudocode for each algorithm lives in the part files. The high-level control flow is:

1. **Config resolution** (`core.md`): merge TOML `[e2e]` section with defaults and validate.
2. **Impact analysis** (`core.md`): map changed files to affected tools via `TOOL_IMPACT_MAP`, assign priorities based on `criticalTools`.
3. **Generator detection** (`core.md`): inspect `package.json` and `vitest.config.*` to select `TypeScriptVitestGenerator`.
4. **Test generation** (`generator.md`): render tool-specific or generic TS/Vitest templates from a `Feature`.
5. **Test execution** (`generator.md`): write temporary files, chunk by `maxConcurrency`, run `pnpm vitest run --reporter=json`, parse output, write report.
6. **Plan enrichment** (`integration.md`): detect changed files, run impact analysis, append a markdown task to the approved plan.
7. **RunE2ETests execution** (`integration.md`): resolve config, detect generator, generate tests, execute, apply `failurePolicy`.

---

## Error Handling (Overview)

Cross-component error handling is summarized below; per-component tables are in the part files.

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `E2EConfigValidationError` | Tool returns error result; fallback to defaults where safe | E2E runs with defaults | User fixes `config.toml` |
| `E2ENoMatchingGeneratorError` | Tool warns and skips generation | No tests generated for this project | Add generator in Phase 2 |
| `ImpactAnalysisFailedError` | Log warning; return empty result | No E2E task injected | Fix analyzer or mapping |
| `VitestProcessError` / `E2ETestTimeoutError` | Parse partial output if possible; mark failed | Apply `failurePolicy` (`block` / `warn` / `ignore`) | Fix source/generated tests or increase timeout |
| `ReportWriteError` | Log warning; still return markdown summary | No JSON report on disk | Fix directory permissions |
| `RunE2ETestsPermissionDenied` | Tool returns error; user can approve and retry | Plan execution pauses | Adjust permission mode or yolo |

---

## Prior Art

Roadmap is the primary reference; no upstream system is being ported. Implementation borrows patterns from:

- Existing `packages/agent-core/src/flags/registry.ts` for feature gating [C:INFERRED].
- Existing `ToolManager.initializeBuiltinTools()` for tool registration [C:INFERRED].
- `vitest` JSON reporter format for result parsing [C:INFERRED].

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | [C:INFERRED] `ExitPlanModeTool` is the right place to mutate the plan file before review; `resolveExecution()` runs before the plan is displayed. | High | E2E task would not appear in approval UI. | Read `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts` lines 95-124. |
| 2 | [C:INFERRED] Plan markdown can be safely appended with a new `### Task N: Generate E2E tests` section without breaking split-plan manifests. | Medium | Could corrupt split plans or duplicate task numbers. | Verify `parsePartsManifest()` and `isWritableSessionModePath()` behavior with sample plans. |
| 3 | [C:INFERRED] Tool directory names under `packages/agent-core/src/tools/builtin/` map to tool class names (e.g. `planning/exit-plan-mode.ts` ↔ `ExitPlanModeTool`). | Medium | Impact analysis would mis-identify affected tools. | Inspect tool file layout and class names. |
| 4 | [C:INFERRED] Generated tests can be written to `.ody-code/test-generated/e2e/` and executed by `pnpm vitest run <file>` from the package directory. | Medium | Tests fail to run or pollute source tree. | Run `pnpm vitest run` against a temporary test file. |
| 5 | [C:INFERRED] Goal-mode auto-trigger for `RunE2ETests` is acceptable as an enhancement gated by the existing `goal-command` flag; default behavior is model-driven. | Medium | If goal-command remains experimental, E2E automation is less reliable than expected. | Check `FLAG_DEFINITIONS` and `TurnFlow.driveGoal()` integration feasibility. |
| 6 | [C:INFERRED] `maxConcurrency` default of 4 and `testTimeout` default of 30s are acceptable defaults. | Medium | Performance or flakiness issues. | Benchmark during dog-food. |
| 7 | [C:INFERRED] The project uses `pnpm` as the package manager, so `pnpm vitest run` is the correct invocation. | High | Wrong command would fail to execute tests. | Verified: `package.json` declares `packageManager: "pnpm@10.33.0"`. |
| 8 | [C:INFERRED] No additional approvals beyond BashTool-like rules are needed for `RunE2ETests`; the user has already approved the plan containing the E2E task. | Medium | Users see unexpected approval prompts. | Review permission policy behavior during implementation. |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | Markdown plan enrichment corrupts existing task numbering or split-plan manifests | Medium | Plan unreadable or parse errors | Append tasks at end; use manifest-aware helper; unit test with split plans |
| 2 | Impact analysis heuristics produce false positives (too many E2E tasks) | Medium | Noise, slower plan execution | Start with strict tool-directory mapping; make mapping table explicit and testable |
| 3 | Impact analysis false negatives miss affected tools | Medium | Regression protection gaps | Mark all changes under `packages/agent-core/src/tools/builtin/**` as at least `important` |
| 4 | Generated temporary tests fail due to import paths or environment | Medium | False CI failures / user confusion | Dog-food on ExitPlanModeTool; validate generated file with `tsc --noEmit` before running |
| 5 | Goal-mode auto-trigger never fires because `goal-command` flag is off | Medium | Perceived automation gap | Document that model-driven path is the default; goal-mode is an enhancement |
| 6 | RunE2ETests subprocess approval is too noisy | Low | User friction | Use BashTool approval rule with command glob matching `pnpm vitest run*` |
| 7 | Concurrent test runs exhaust resources | Low | Slowdown or OOM | Default `maxConcurrency: 4`; respect `AbortSignal` for cancellation |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `e2e-testing-automation-phase-1/core.md` | Data models, config schema, generator registry, impact analysis | done |
| 2 | `e2e-testing-automation-phase-1/generator.md` | TypeScript/Vitest generator and test executor | done |
| 3 | `e2e-testing-automation-phase-1/integration.md` | Plan enrichment, RunE2ETests tool, registration, dog-food tests | done |

---

## Self-Review

### Security
- Checked the task-number regex and path-extraction regex with `node -e`; no false positives on `Tasks`/`Step`/`####Task`, and no missed valid `### Task N:` headings.
- Verified that generated tests live only in `.ody-code/test-generated/e2e/` and JSON reports in `.ody-code/test-reports/`; both directories are outside source control.
- Confirmed `RunE2ETestsTool` reuses BashTool-style approval rules, so subprocess execution is not silently auto-approved outside yolo mode.
- No PII or secrets are embedded in report filenames (timestamp-only) or generated test templates.

### Test
- Every behavior in the parts has at least one must-pass assertion and a must-reject or boundary case.
- Added adversarial cases: `strategy === 'critical-only'` with no critical tools must not inject a task; `RunE2ETestsTool.execution` with invalid `projectRoot` must reject.
- Verified that no assertion contradicts its constants (e.g. default `enabled: true`, default `criticalTools: ['ExitPlanModeTool']`).

### Ops
- `maxConcurrency: 4` caps parallel vitest invocations; report filenames include millisecond timestamps to avoid collisions.
- Temporary generated tests are cleaned up by the execution lifecycle (written to a temp dir that can be wiped between runs).
- `AbortSignal` is forwarded to the subprocess where supported.

### Integration
- Verified `ExitPlanModeTool` exists at `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts` with `resolveExecution()` at lines 95-124.
- Verified `ToolManager.initializeBuiltinTools()` at `packages/agent-core/src/agent/tool/index.ts` lines 407-464 for tool registration.
- Verified `Agent.config` exposes `cwd` and `data()` for config access.
- Verified `flags.enabled('goal-command')` and `TurnFlow.driveGoal()` exist.
- Verified `Kaos.exec` is available and is the required abstraction for subprocess calls.
- No silent retargeting: the design builds exactly where the roadmap specified (`packages/agent-core/src/e2e-testing/`).

### Scope
- The design remains one coherent subsystem: Phase 1 of the E2E testing automation roadmap.
- Multi-language generators, recursive impact analysis, caching, ML, contract testing, and mutation testing are explicitly deferred to Phases 2/3.

---

## User Final Approval

- Audit level: Deep [C:USER].
- All 8 [C:INFERRED] assumptions were reviewed; user elected to **accept and defer** them for verification during implementation [C:USER].
- Design file and all parts are complete; no corrections required.
- Approved for ExitDesignMode.
