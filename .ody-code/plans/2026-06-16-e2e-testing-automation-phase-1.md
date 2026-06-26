# E2E Testing Automation — Phase 1 Implementation Plan

**Goal:** Build Phase 1 of the E2E testing automation roadmap inside `packages/agent-core`, including the generator framework, TypeScript/Vitest generator, impact analysis, plan-mode enrichment, `RunE2ETests` builtin tool, configuration schema, JSON reports, dog-food validation, and user guide.

**Architecture:** Core abstractions (config, types, impact analyzer, generator registry) live in `packages/agent-core/src/e2e-testing/`. The `TypeScriptVitestGenerator` and `E2ETestExecutor` generate temporary tests under `.ody-code/test-generated/e2e/` and run them with `pnpm vitest run`, writing JSON reports to `.ody-code/test-reports/`. `E2EPlanEnricher` mutates the approved plan markdown inside `ExitPlanModeTool` before review, and `RunE2ETestsTool` lets the model execute E2E validation during normal-mode execution.

**Tech Stack:** TypeScript, Zod, Vitest, Kaos, pnpm, picomatch, smol-toml.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use `- [ ]` checkboxes for tracking.

---

## File Structure

| Path | Responsibility |
|---|---|
| `packages/agent-core/src/config/schema.ts` | Adds `E2EConfigSchema` and `e2e` field to `KimiConfigSchema`. |
| `packages/agent-core/src/config/toml.ts` | Adds `[e2e]` section transform and TOML round-trip serialization. |
| `packages/agent-core/src/e2e-testing/types.ts` | Domain types: `Feature`, `TestFile`, `ProjectStructure`, `AffectedTool`, `ImpactAnalysisResult`, `E2ETestGenerator`, `E2EExecutionResult`, etc. |
| `packages/agent-core/src/e2e-testing/config.ts` | `E2EConfigResolver` — merges defaults with `[e2e]` config and validates. |
| `packages/agent-core/src/e2e-testing/errors.ts` | Error helpers and codes for E2E-specific failures. |
| `packages/agent-core/src/e2e-testing/impact-map.ts` | Static `TOOL_IMPACT_MAP` from tool class name to affected file globs. |
| `packages/agent-core/src/e2e-testing/impact-analyzer.ts` | `ImpactAnalyzer.analyze(changedFiles, config)`. |
| `packages/agent-core/src/e2e-testing/registry.ts` | `E2EGeneratorRegistry` singleton + `TypeScriptVitestGenerator` registration. |
| `packages/agent-core/src/e2e-testing/generator.ts` | `TypeScriptVitestGenerator` detection and test-file generation. |
| `packages/agent-core/src/e2e-testing/executor.ts` | `E2ETestExecutor` — write, run, parse, report, markdown summary. |
| `packages/agent-core/src/e2e-testing/plan-enricher.ts` | `E2EPlanEnricher` — git-status / path extraction and plan markdown mutation. |
| `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts` | `RunE2ETestsTool` builtin tool. |
| `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.md` | Tool description surfaced to the model. |
| `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts` | Injects plan enrichment before review; constructor gains `Kaos`. |
| `packages/agent-core/src/tools/builtin/index.ts` | Re-exports `RunE2ETestsTool`. |
| `packages/agent-core/src/agent/tool/index.ts` | Registers `RunE2ETestsTool` and passes `kaos` to `ExitPlanModeTool`. |
| `packages/agent-core/src/agent/compaction/normal-task-checkpoint.ts` | Optional goal-mode E2E reminder hook. |
| `packages/agent-core/test/e2e-testing/core.test.ts` | Unit tests for config resolver, impact analyzer, registry. |
| `packages/agent-core/test/e2e-testing/generator.test.ts` | Unit tests for `TypeScriptVitestGenerator`. |
| `packages/agent-core/test/e2e-testing/executor.test.ts` | Unit tests for `E2ETestExecutor`. |
| `packages/agent-core/test/e2e-testing/integration.test.ts` | Unit tests for `RunE2ETestsTool` and `E2EPlanEnricher`. |
| `packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts` | End-to-end plan-mode flow test. |
| `.ody-code/docs/e2e-testing-guide.md` | User-facing guide for the E2E framework. |
| `.gitignore` | Excludes `.ody-code/test-generated/` and `.ody-code/test-reports/`. |

---

## Dependency Overview

```
Part 1: Core
  Task 1 ──► Task 2 ──► Task 3 ──► Task 4 ──► Task 5 ──► Task 6
  (schema)  (resolver) (types)   (impact)  (registry) (tests)

Part 2: Generator
  Task 1 ──► Task 2 ──► Task 3
  (generator) (executor) (tests)
  Depends on Part 1 Core.

Part 3: Integration
  Task 1 ──► Task 2 ──► Task 3 ──► Task 4 ──► Task 5 ──► Task 6 ──► Task 7
  (enricher) (tool)   (exit-plan wiring) (registration) (goal hook) (tests) (docs/gitignore)
  Depends on Part 1 Core and Part 2 Generator.
```

Part 1 can be implemented and tested independently. Part 2 depends on Part 1 types/config. Part 3 depends on Part 1 + Part 2. Within each part, tasks are sequential.

---

## Risks & Open Questions

| Risk | Mitigation in this plan |
|---|---|
| `ExitPlanModeTool` constructor change touches many tests. | Task 3 of Part 3 lists every test caller and updates them in the same commit; ends with whole-tree typecheck. |
| `Kaos.exec` has no `cwd` option. | Use `kaos.withCwd(projectRoot).exec(...)` everywhere subprocesses need a working directory. |
| No `agent.workspaceRoot` field exists. | Use `agent.config.cwd` as the project root fallback. |
| No `agent.injection.addNormalPartitionReminder` method exists. | Use `agent.context.appendSystemReminder(...)` from the normal-mode checkpoint hook. |
| Enrichment must not corrupt split-plan manifests. | Append the E2E task after the existing content without altering the `## Parts` table; verified by `plan-enrichment.e2e.test.ts`. |
| Generated tests must be excluded from regular `pnpm test` and git. | Add `.ody-code/test-generated/` and `.ody-code/test-reports/` to `.gitignore`; the executor writes outside `packages/agent-core/test/`. |
| Vitest JSON reporter schema may drift. | Parser treats missing fields as empty arrays/defaults. |

---

## Spec Coverage

| # | Design Requirement | Task(s) | Status |
|---|---|---|---|
| 1 | Extensible E2E generator framework in `packages/agent-core/src/e2e-testing/` | `core.md` Tasks 3, 5 | covered |
| 2 | TypeScript/Vitest generator for ody-code self-testing | `generator.md` Task 1 | covered |
| 3 | File-level impact analysis with heuristic rules for builtin tools | `core.md` Task 4 | covered |
| 4 | Plan-mode integration: append E2E task before handoff | `integration.md` Task 1, 3 | covered |
| 5 | New builtin tool `RunE2ETests` | `integration.md` Task 2, 4 | covered |
| 6 | Configuration schema in `~/.ody-code/config.toml` under `e2e` | `core.md` Task 1, 2 | covered |
| 7 | JSON reports under `.ody-code/test-reports/` + markdown summary | `generator.md` Task 2 | covered |
| 8 | Dog-food validation for `ExitPlanModeTool` and plan-mode flow | `integration.md` Task 6 | covered |
| 9 | Unit tests for registry, impact analysis, generator, executor ≥80% | `core.md` Task 6, `generator.md` Task 3, `integration.md` Task 6 | covered |
| 10 | User guide at `.ody-code/docs/e2e-testing-guide.md` | `integration.md` Task 7 | covered |
| — | Python/Pytest and Node.js/Jest generators (Phase 2) | — | no-op |
| — | Recursive/transitive dependency graph (Phase 2) | — | no-op |
| — | Test result caching (Phase 2) | — | no-op |
| — | ML risk scoring, contract testing, mutation testing (Phase 3) | — | no-op |
| — | Auto-commit of generated tests | — | no-op |
| — | Visual/TUI report UI | — | no-op |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-16-e2e-testing-automation-phase-1/core.md` | Config schema, resolver, types, impact analysis, registry, unit tests | done |
| 2 | `2026-06-16-e2e-testing-automation-phase-1/generator.md` | TypeScript/Vitest generator, executor, unit tests | done |
| 3 | `2026-06-16-e2e-testing-automation-phase-1/integration.md` | Plan enrichment, RunE2ETests tool, wiring, registration, goal hook, dog-food tests, docs | done |
