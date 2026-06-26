# Phase A3 Verification & Fixup Implementation Plan

**Goal:** Implement a reproducible Node 24.15.0 verification suite for Phase A3, close the `apps/vis/web` typecheck gap, add a non-interactive TUI smoke mode, and wire it into CI.

**Architecture:** A single `scripts/verify-phase-a3.mjs` orchestrator runs an ordered, fail-fast `StepRegistry` and produces `.ody-code/reports/phase-a3-report.json`; the CLI `--smoke-test` flag bypasses the interactive TUI and exercises the Rust host over stdio/socket/TCP; source-level type fixes restore `pnpm -r typecheck`; CI replaces scattered steps with one `pnpm run verify:phase-a3` invocation across the platform matrix.

**Tech Stack:** Node.js 24.15.0+, pnpm 10.33.0, TypeScript 6.0.2, Rust stable, GitHub Actions, Vitest.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `scripts/verify-phase-a3.mjs` | Main orchestrator: config parsing, StepRegistry, report/ADR update |
| `scripts/verify-phase-a3.test.mjs` | Unit tests for redaction, summary, ADR update, Node version check |
| `apps/ody-code/src/cli/options.ts` | Add `smokeTest` to `CLIOptions`; validate `--smoke-test` requires `--host=rust` |
| `apps/ody-code/src/cli/commands.ts` | Register `--smoke-test` flag; parse into `CLIOptions` |
| `apps/ody-code/src/cli/run-shell.ts` | Branch to `runSmokeTest()` when `opts.smokeTest` |
| `apps/ody-code/src/tui/types.ts` | Add `smokeTest` to `OdyTUIOptions` / `OdyTUIStartupInput` |
| `apps/ody-code/src/tui/ody-tui.ts` | Lazy terminal setup in smoke mode; add `runSmokeTest()`; guard `start()` |
| `package.json` | Add `verify:phase-a3` and `verify:phase-a3:local` scripts |
| `packages/agent-core-shared/src/wasm-loader.ts` | Cast `WebAssembly.instantiate` result to expected type |
| `packages/agent-core/src/agent/permission/index.ts` | Prefix unused `signal` parameter with `_` |
| `packages/agent-core/src/agent/permission/matches-rule.ts` | Remove/type-only unused `ParsedPattern` import |
| `packages/agent-core/src/rpc/client.ts` | Prefix unused `fn` parameter with `_` |
| `packages/agent-core/src/rpc/transports/websocket.ts` | Remove unused `decodeJson` import |
| `packages/agent-core/src/session/checkpoint/backup-store.ts` | Remove unused `dirname` import |
| `packages/agent-core/src/session/checkpoint/recovery.ts` | Remove unused `CheckpointVersion` import |
| `packages/agent-core/src/session/hooks/types.ts` | Remove/type-only unused `HOOK_EVENT_TYPES` import |
| `packages/agent-core/src/utils/wasm-glob.ts` | Remove unused `GLOB_ERROR` constant |
| `packages/e2e-testing/src/generators/python-pytest.ts` | Remove unused `extname` import |
| `packages/e2e-testing/src/recursive-impact-analyzer.ts` | Remove unused `existsSync` import |
| `packages/e2e-testing/src/result-cache.ts` | Prefix unused `kaos` property with `_` |
| `packages/mcp-host/src/built-in/sea-builtins.ts` | Remove unused `readFileSync` import |
| `packages/mcp-host/src/oauth/service.ts` | Remove/type-only unused `OAuthClientProvider` import |
| `packages/mcp-host/src/trace-recorder.ts` | Remove unused `dirname` import |
| `.github/workflows/rust-host.yml` | Replace steps with `pnpm run verify:phase-a3`; upload report + binary |

---

## Dependency Overview

### Phase A — Verification Script
Independent of product changes; produces the reusable verification harness.

- **Task A1**: Node check + `VerificationConfig` parsing
- **Task A2**: `redact()` with must-survive / must-reject tests
- **Task A3**: `runStep()` + timeout + process-tree cleanup
- **Task A4**: `StepRegistry` + report aggregation
- **Task A5**: ADR updater
- **Task A6**: Package scripts + dry-run end-to-end test

### Phase B — TUI Smoke Mode
Independent of Phase A; can be built in parallel.

- **Task B1**: `CLIOptions` + commander flag registration
- **Task B2**: `options.ts` validation
- **Task B3**: `OdyTUI` smoke mode + `runSmokeTest()`
- **Task B4**: `run-shell.ts` smoke branch
- **Task B5**: Smoke tests for stdio/socket/tcp

### Phase C — vis/web Typecheck Fixes
Independent of A and B; can be built in parallel. See `2026-06-26-backend-architecture-evolution-phase3-f/vis-typecheck.md` for the full task list.

- **Task C1**: `wasm-loader.ts` type fix
- **Task C2–C7**: Unused-symbol fixes across `agent-core`, `e2e-testing`, and `mcp-host`
- **Task C8**: Verify `pnpm -r typecheck` passes

### Phase D — CI Integration
Depends on A, B, and C being complete and green because the verification script exercises the smoke and typecheck steps. See `2026-06-26-backend-architecture-evolution-phase3-f/ci-integration.md` for the full task list.

- **Task D1**: Verify root `package.json` Phase A3 scripts
- **Task D2**: Rewrite `.github/workflows/rust-host.yml`
- **Task D3**: Validate workflow YAML and local script run
- **Task D4**: Run CI smoke test on PR branch

### Parallel Groups
- **Group 1**: Phase A, Phase B, Phase C — no shared symbols, can run concurrently.
- **Group 2**: Phase D — serializes after Group 1.

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| R1 | `ProcessTerminal`/`TUI` constructor opens `/dev/tty` even when smoke mode is requested | Move terminal creation and layout building out of the smoke branch; only instantiate `ProcessTerminal` when `smokeTest === false`. |
| R2 | `WebAssembly.instantiate` cast may be rejected by the actual TypeScript version or another package's config | If the cast fails, use a runtime guard: `const result = await WebAssembly.instantiate(bytes, {}); const instance = 'instance' in result ? result.instance : result;` |
| R3 | Removing an "unused" symbol breaks another package's build | Run `pnpm -r typecheck` after each removal; prefer `_` prefix over removal for public/callback signatures. |
| R4 | CI matrix runner does not have Node 24.15.0 exactly | Use `.nvmrc` and `actions/setup-node@v4` with `node-version-file`; the verification script also checks Node version. |
| R5 | SEA build times out on linux-x64 | Override `sea-build` timeout via `ODY_CODE_STEP_TIMEOUTS` env in the verification script. |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-26-backend-architecture-evolution-phase3-f/verification-script.md` | Verification script, report, redaction, ADR update | done |
| 2 | `2026-06-26-backend-architecture-evolution-phase3-f/tui-smoke.md` | `--smoke-test` flag, non-interactive TUI smoke | done |
| 3 | `2026-06-26-backend-architecture-evolution-phase3-f/vis-typecheck.md` | `apps/vis/web` typecheck remediation | done |
| 4 | `2026-06-26-backend-architecture-evolution-phase3-f/ci-integration.md` | `.github/workflows/rust-host.yml` rewrite | done |

---

## Spec-Coverage Table

| Design Requirement | Task(s) | Status |
|---|---|---|
| `scripts/verify-phase-a3.mjs` reusable verification script | A1–A6 | covered |
| `.ody-code/reports/phase-a3-report.json` machine-readable report | A4 | covered |
| Update `docs/designs/rust-host-reversal-adr.md` Prototype Results tables | A5 | covered |
| `--smoke-test` flag + non-interactive TUI smoke | B1–B5 | covered |
| stdio/socket/tcp transport smoke coverage | B3–B5 | covered |
| `apps/vis/web` typecheck fix (full `pnpm -r typecheck`) | C1–C8 | covered |
| `.github/workflows/rust-host.yml` rewrite | D1–D4 | covered |
| Node 24.15.0 minimum version enforcement | A1 | covered |
| Secret redaction before report write | A2 | covered |
| Fail-fast step registry with partial reports | A3–A4 | covered |

---

## Self-Review

- [ ] 1. **Spec-coverage table:** every design requirement from the approved design is mapped to one or more tasks; no GAP rows remain.
- [ ] 2. **Placeholder scan:** no TODO/TBD/"implement later" remains in the index or any part file; every task shows complete code/commands.
- [ ] 3. **No phantom tasks:** every task in every part file produces a verifiable change or explicit observation; no `--allow-empty` commits or "already done in Task N" shortcuts.
- [ ] 4. **Dependency soundness:** every `Depends on:` in every part file points to an earlier task in the same part or to a completed prerequisite part (Part D depends on Parts A–C); no forward references exist.
- [ ] 5. **Caller & build soundness:** shared-signature changes are consolidated into single tasks (e.g. `smokeTest` added to `CLIOptions`/`OdyTUIOptions` in Task B1/B3, with all callers updated and whole-tree typecheck run in Task B5; verification script env vars consumed by CI in Task D2). No signature is changed across multiple tasks.
- [ ] 6. **Test-the-risk:** state-mutating or permission-sensitive code has behavioral tests (redaction must-survive/must-reject in Task A2, StepRegistry partial reports in Task A4, smoke transport assertions in Task B5). Non-testable CI/type fixes have manual verification steps with exact commands and expected observations.
- [ ] 7. **Type consistency:** property names (`smokeTest`, `host`, `session`), environment variable names (`ODY_HOST_BINARY_PATH`, `ODY_CODE_REPORT_DIR`), and report paths (`.ody-code/reports/phase-a3-report.json`) match between the script, CLI, TUI, and CI tasks.
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/apps/ody-code/scripts/native (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/scripts (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src/cli (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src/native (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code (priority: important)
- /Users/ranwei/workspace/ody-code/packages/node-sdk/src (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

