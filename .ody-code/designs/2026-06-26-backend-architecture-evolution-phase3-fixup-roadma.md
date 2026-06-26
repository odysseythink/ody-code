# Phase A3 — Re-verify on Node 24.15.0: Detailed Design

> **Source roadmap**: `.ody-code/roadmaps/backend-architecture-evolution-phase3-fixup-roadmap.md`  
> **Scope**: Close the remaining Phase 3 prototype blockers by making the Node 24.15.0 verification reproducible, fixing `apps/vis/web` typecheck, adding a non-interactive TUI smoke mode, and wiring everything into CI.  
> **Audit level**: Deep [C:USER]

---

## Scope In/Out

### 1.1 In scope [C:USER]

1. `scripts/verify-phase-a3.mjs` — a reusable Node.js verification script that runs the Phase A3 acceptance suite in a single command.
2. `.ody-code/reports/phase-a3-report.json` — a machine-readable JSON report produced by the script.
3. Update `docs/designs/rust-host-reversal-adr.md` Prototype Results tables from the JSON report.
4. `apps/ody-code/src/cli/` — add a `--smoke-test` flag and a non-interactive TUI smoke path for stdio, socket, and tcp transports.
5. `apps/vis/web` — fix pre-existing TypeScript errors so that `pnpm -r typecheck` passes.
6. `.github/workflows/rust-host.yml` — replace scattered verification steps with a single call to `scripts/verify-phase-a3.mjs` across the existing platform matrix.

### 1.2 Out of scope [C:USER]

| Item | Reason |
|---|---|
| Phase A1/A2 implementation (`serve` CLI contract, `rust-host-connect.test.ts`) | Already present and passing in the current dirty tree; A3 only consumes them. |
| SEA `#/host` import remediation | `pnpm --filter ody-code run build:native:sea` succeeds on Node 24.16.0 in the current tree; no remediation required for A3. |
| Rust host CoreAPI expansion (prompt/steer, setModel, etc.) | Phase D2; out of A3 prototype verification scope. |
| OAuth / MCP integration in Rust host | Out of prototype scope per ADR. |
| Monorepo-wide `#/foo` → `#foo` migration | Phase D3; future-proofing, not a blocker on Node 24. |

---

## Architecture / Design

```
local dev / CI runner
    │
    ▼
scripts/verify-phase-a3.mjs
    │ parses CLI flags / env → VerificationConfig
    ▼
StepRegistry (ordered, fail-fast)
    │
    ├──► RustTestStep            : cargo test -p ody-host
    ├──► CrossLangRpcStep        : pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts
    ├──► TuiSmokeStep (stdio)    : dev:cli-only -- --host=rust --host-stdio --smoke-test
    ├──► TuiSmokeStep (socket)   : dev:cli-only -- --host=rust --host-socket <tmp> --smoke-test
    ├──► TuiSmokeStep (tcp)      : dev:cli-only -- --host=rust --host-tcp 127.0.0.1:<port> --smoke-test
    ├──► SeaBuildStep            : pnpm --filter ody-code run build:native:sea
    ├──► SeaSmokeStep            : pnpm --filter ody-code run test:native:smoke
    └──► TypecheckStep           : pnpm -r typecheck
    │
    ▼
StepResult[]  (stdout/stderr redacted)
    │
    ▼
ReportWriter → .ody-code/reports/phase-a3-report.json
    │
    ▼
AdrUpdater → docs/designs/rust-host-reversal-adr.md
```

Data changes at each arrow:
- CLI/env → `VerificationConfig`: defaults resolved, binary paths discovered, temp dirs prepared.
- `VerificationConfig` → `StepRegistry`: each step receives its own `StepContext` (timeout, env, cwd, extra args).
- Subprocess output → `StepResult`: raw stdout/stderr is captured, then redacted.
- `StepResult[]` → JSON report: aggregated, with metadata and summary.
- Report → ADR: table cells updated to PASS / FAIL / BLOCKED.

### 2.1 Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | [`2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/verification-script.md`](2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/verification-script.md) | Verification script, `StepRegistry`, JSON report schema, redaction, ADR update | done |
| 2 | [`2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/tui-smoke.md`](2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/tui-smoke.md) | `--smoke-test` flag, non-interactive TUI smoke for stdio/socket/tcp | done |
| 3 | [`2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/vis-typecheck.md`](2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/vis-typecheck.md) | `apps/vis/web` typecheck failure remediation | done |
| 4 | [`2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/ci-integration.md`](2026-06-26-backend-architecture-evolution-phase3-fixup-roadma/ci-integration.md) | `.github/workflows/rust-host.yml` rewrite, matrix, artifact upload | done |

---

## Data Models

Top-level data structures live in the part files. The cross-cutting model is:

```ts
interface VerificationConfig {
  hostBinaryPath: string;
  reportDir: string;
  defaultTimeoutMs: number;
  stepTimeoutsMs: Record<string, number>;
  skipSea: boolean;
  keepTemp: boolean;
}

interface StepResult {
  id: string;
  status: 'passed' | 'failed' | 'skipped';
  command: string;
  args: string[];
  cwd: string;
  exitCode: number | null;
  signal: string | null;
  durationMs: number;
  stdoutRedacted: string;
  stderrRedacted: string;
  errorMessage?: string;
}

interface PhaseA3Report {
  metadata: {
    nodeVersion: string;
    platform: string;
    arch: string;
    timestamp: string;
    commit?: string;
    hostBinaryPath: string;
  };
  environment: {
    cwd: string;
    pnpmVersion: string;
    cargoVersion: string;
    rustcVersion: string;
  };
  steps: StepResult[];
  summary: {
    overallStatus: 'passed' | 'failed' | 'partial';
    passedCount: number;
    failedCount: number;
    skippedCount: number;
    totalDurationMs: number;
  };
}
```

See Part 1 for field contracts and defaults.

---

## Algorithms

Top-level orchestration:

```
main(argv):
  ensureNodeVersion('24.15.0')
  config := parseConfig(argv)
  report := runVerification(config)
  writeReport(report)
  updateAdr(report)
  exit(report.summary.overallStatus === 'passed' ? 0 : 1)

runVerification(config):
  ctx := buildContext(config)
  steps := buildStepRegistry(config)
  results := []
  for step in steps
    if shouldSkip(step, config)
      results.push(skippedResult(step))
      continue
    result := await runStep(step, ctx)
    results.push(result)
    if result.status === 'failed' break
  return aggregateReport(results)
```

See Part 1 for step execution, redaction, and ADR update algorithms; Part 2 for TUI smoke; Part 3 for typecheck remediation; Part 4 for CI wiring.

---

## Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| Node version < 24.15.0 | Throw before any subprocess; exit 1. | None. | Switch Node version. |
| Missing `ody-host` binary | Throw in context setup; exit 1. | None. | Run `pnpm run build:host`. |
| Any verification step fails | Fail-fast: stop registry, write partial report, update ADR with FAIL. | Remaining steps skipped. | Fix failing step; rerun. |
| Step timeout | Kill process tree, return failed result. | Partial report preserved. | Increase timeout or fix slow step. |
| Secret leak suspected | Redact before write; raw logs kept locally only when `--keep-temp` is set. | None. | Update redaction patterns. |

See the part files for subsystem-specific error handling.

---

## Self-Review

### 6.1 Security lens

- Secret redaction regexes in Part 1 cover JSON-style `api_key` values, HTTP `Authorization: Bearer ...` tokens, and inline `api_key=...` assignments. They may miss custom secret names; the regex list is extensible. [C:INFERRED]
- No secrets are passed on command lines in the verification script; all are env vars. [C:USER]
- CI artifact upload uses `if: always()`, which uploads logs even on failure; logs are redacted before write. [C:USER]

### 6.2 Test lens

- Every verification step has a must-pass case (green run) and must-reject case (simulated failure or bad env). [C:USER]
- `redact` has explicit must-pass and must-reject cases in Part 1. [C:USER]
- TUI smoke asserts `SMOKE_OK` output and non-interactive exit. [C:USER]
- Typecheck remediation asserts `pnpm -r typecheck` exits 0. [C:USER]

### 6.3 Ops lens

- Verification script may take 5–10 minutes end-to-end; default step timeout is 5 minutes, with per-step overrides. [C:INFERRED]
- Subprocess trees are killed on timeout to avoid CI runner leaks. [C:USER]
- Matrix runs in parallel; `fail-fast: false` prevents one platform failure from hiding others. [C:USER]
- Report file name is fixed (`phase-a3-report.json`), so concurrent CI jobs on different matrix entries do not collide. [C:INFERRED]

### 6.4 Integration lens

- All hooks the design relies on exist in code:
  - `package.json` scripts `build:host`, `test:host`, `proto:rust-host`. [C:INFERRED]
  - `packages/node-sdk/test/rust-host-connect.test.ts` exists and passes. [C:INFERRED]
  - `apps/ody-code/scripts/native/build.mjs` and `smoke.mjs` exist. [C:INFERRED]
  - `apps/ody-code/src/cli/options.ts` validates `--host=rust` transport flags. [C:INFERRED]
  - `OdyTUI` constructor and `start()` exist in `apps/ody-code/src/tui/ody-tui.ts`. [C:INFERRED]
- The design lands at the user-named target: Phase A3 of the roadmap, implemented via `scripts/verify-phase-a3.mjs` and related files. [C:USER]

### 6.5 Scope lens

- This design remains one coherent verification-plus-fixup task for Phase A3, even though it spans four subsystems, because all parts serve a single acceptance gate. [C:USER]
- No Phase B/C/D work is included. [C:USER]

---

## User Approval

- [x] Scope In/Out reviewed and accepted.
- [x] Architecture / Design reviewed and accepted.
- [x] Part files reviewed and accepted.
- [x] Assumptions & Unverified Items reviewed and accepted.
- [x] Risk Register reviewed and accepted.
- [x] Self-Review findings reviewed and accepted.

Approval state: approved pending ExitDesignMode confirmation.

---

## Reuse Analysis

| File / Module | What it solves | Reuse decision |
|---|---|---|
| `packages/node-sdk/test/rust-host-connect.test.ts` | Spawns real `ody-host`, runs session lifecycle, handles stdio/uds/tcp, cleans up. | Reuse patterns (`withRustHost`, binary resolution, temp home) as reference for `TuiSmokeStep` implementation. [C:INFERRED] |
| `apps/ody-code/scripts/native/build.mjs` / `smoke.mjs` | SEA build orchestration and native smoke assertions. | Reuse as the commands invoked by `SeaBuildStep` and `SeaSmokeStep`; no change required. [C:INFERRED] |
| `apps/ody-code/scripts/native/check-bundle.mjs` | Validates that the bundled SEA output has no unresolved relative imports. | Keep as-is; A3 only verifies it passes, does not modify it. [C:INFERRED] |
| `apps/ody-code/src/cli/options.ts` (lines ~121-143) | Validates `--host=rust` transport defaults. | Extend to recognize `--smoke-test` in `CLIOptions`. [C:INFERRED] |
| `apps/ody-code/src/cli/run-shell.ts` (lines ~62-94) | Creates `RustHostConnector` and connects to Rust host. | Smoke path will call the same `createRustHarness` / connector flow, then exit. [C:INFERRED] |
| `apps/ody-code/src/tui/index.ts` / `OdyTUI` | TUI lifecycle and session startup. | Add smoke branch near startup; reuse `harness.createSession` and `tui.exit`. [C:INFERRED] |
| `apps/vis/web/tsconfig.json` + source | Existing typecheck configuration and errors. | Fix source-level unused imports/variables; do not change tsconfig rules. [C:INFERRED] |
| `.github/workflows/rust-host.yml` | Existing CI matrix and steps. | Replace steps with verification-script call; reuse matrix and environment setup. [C:INFERRED] |

---

## 9. Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| A1 | [C:INFERRED] The current dirty tree already resolves the `serve` CLI mismatch and `#/host` SEA block; A3 only needs to verify, not re-fix. | High | A3 would slip back into A1/A2/B1 work and miss the verification deadline. | Run `cargo test -p ody-host`, `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts`, and `pnpm --filter ody-code run build:native:sea` on Node 24.15.0+ before implementation. |
| A2 | [C:INFERRED] `apps/vis/web` typecheck failures are limited to unused imports/variables plus one wasm `instance` type error, with no deeper architectural issues. | Medium | A3 scope expands into vis/web refactoring or dependency upgrades. | Inspect `apps/vis/web` source and `tsc --noEmit` output. |
| A3 | [C:INFERRED] TUI can be launched in a non-interactive smoke mode without requiring an LLM provider or real API key. | High | `--smoke-test` implementation becomes complex or infeasible. | Review `apps/ody-code/src/tui/index.ts` startup flow and `OdyTUI` lifecycle. |
| A4 | [C:USER] `pnpm -r typecheck` is the desired full-workspace check. | High | Wrong acceptance criteria; team expectations not met. | Confirmed by user choice in clarifying questions. |
| A5 | [C:INFERRED] CI matrix (darwin-arm64, darwin-x64, linux-x64) has enough resources to run the full A3 suite in reasonable time. | Medium | Builds time out or queue too long. | Time the full suite during implementation; split slow steps if needed. |

---

## 10. Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | `apps/vis/web` wasm-loader `instance` error reflects a real type mismatch that needs dependency changes. | Medium | Typecheck fix drags in unrelated work. | Cap the fix to the reported files; if deeper, use `// @ts-expect-error` with a TODO and ticket. |
| R2 | TUI `--smoke-test` needs terminal/TTY handling that differs across platforms. | Medium | Smoke fails on Linux CI but passes locally on macOS. | Run smoke in CI matrix from day one; set `CI=true` and a deterministic `TERM` value. |
| R3 | CI runner does not have Rust toolchain or Node 24.15.0 exactly. | Low | Workflow fails for environment reasons. | Pin via `.nvmrc` and `dtolnay/rust-toolchain@stable`; fail fast with clear messages. |
| R4 | Verification script captures secrets before redaction if a subprocess crashes and dumps env. | Low | PII leak in CI artifact. | Redact before writing; avoid passing secrets on command lines; use env vars only. |
| R5 | A verification step leaves orphan `ody-host` or `ody` processes on failure. | Medium | CI runner resource leaks. | Always run steps inside `finally` cleanup; kill subprocess trees on timeout/exit. |

---

## 11. Cross-Cutting Decisions

- **Audit level**: Deep — every assumption and each section's key claim must be confirmed. [C:USER]
- **Node runtime**: Node.js `>=24.15.0`, pinned via `.nvmrc`. [C:USER]
- **Fail-fast behavior**: stop on first failure, write partial report. [C:USER]
- **Secret handling**: redact `api_key`, `access_token`, `password`, `secret`, `authorization` from captured output. [C:USER]
- **Report format**: JSON with metadata, per-step results, environment summary, and overall summary. [C:USER]
- **TUI smoke scope**: stdio, socket, and tcp transports; no LLM call. [C:USER]
- **Typecheck scope**: full `pnpm -r typecheck`, including `apps/vis/web`. [C:USER]
- **CI matrix**: darwin-arm64, darwin-x64, linux-x64. [C:USER]
