# Phase 3 Fixup Roadmap — Rust Host Reversal Prototype Hardening

> **Document Type**: Remediation Roadmap  
> **Scope**: Make the Phase 3 Rust Host prototype actually runnable end-to-end, pass acceptance criteria, and resolve the blockers discovered during the source audit on 2026-06-25.  
> **Target File**: `.ody-code/roadmaps/backend-architecture-evolution-phase3-fixup-roadmap.md`

---

## 1. Goal

Phase 3 code is structurally complete but not yet integrated: the Rust host compiles and tests green, the TS connector tests green against mocks, but a real TS -> Rust host session fails because of a CLI argument mismatch and a missing cross-language test. This roadmap brings the prototype to a state where:

1. `pnpm run proto:rust-host` launches the TUI against a real Rust host and survives session creation.
2. A cross-language RPC test exercises the real `ody-host` binary in CI.
3. The SEA native-asset path for `ody-host` is verified on at least one platform.
4. The full workspace `typecheck` is green or has documented, scoped exceptions.
5. Phase 1-C package extraction is finished (no remaining code-review / e2e / mcp / office-hours logic inside `packages/agent-core/src`).

---

## 2. Current State Snapshot (2026-06-25)

| Area | Status | Evidence |
|---|---|---|
| `ody-host` crate | Compiles, 30 tests pass | `cargo test -p ody-host` |
| Rust transport (stdio/socket/tcp) | Implemented | `rust-ody/crates/ody-host/src/transport/` |
| TS `SDKRpcClient.connect` | Implemented, mock tests pass | `packages/node-sdk/src/rpc.ts`, `sdk-rpc-client-connect-binary.test.ts` |
| `RustHostConnector` / `RustHostHarness` | Implemented, unit tests pass | `apps/ody-code/src/host/` |
| CLI `--host=rust` options | Implemented, validation tests pass | `apps/ody-code/src/cli/options.ts`, `commands.ts` |
| SEA asset collection for `ody-host` | Implemented, smoke test passes | `apps/ody-code/scripts/native/assets.mjs`, `apps/ody-code/test/native/smoke.test.ts` |
| CI workflow | Created | `.github/workflows/rust-host.yml` |
| Go/No-Go ADR | Written | `docs/designs/rust-host-reversal-adr.md` |
| **Cross-language real-RPC test** | **Missing** | `packages/node-sdk/test/rust-host-connect.test.ts` does not exist |
| **CLI argument compatibility** | **Broken** | TS spawns `ody-host serve --stdio`; Rust binary rejects `serve` |
| **Manual TUI smoke** | **Untested** | `pnpm run proto:rust-host` currently fails before TUI starts |
| **Full SEA build** | **Blocked on Node version / imports** | Requires Node >=24.15.0; Node 24 deprecates `#/foo` imports |
| **Workspace typecheck** | **Failing** | `apps/vis/web` reports unused-variable and wasm-loader errors |
| **agent-core extraction residue** | **Incomplete** | `office-hours/state.ts`, `tools/builtin/code-review`, `tools/builtin/e2e`, `skill/builtin/mcp-config.ts` remain |

---

## 3. Discovered Blockers

### B1 — TS spawns `ody-host serve ...`, Rust host has no `serve` subcommand

**Severity**: P0 — blocks real TS -> Rust connection.  
**Location**: `packages/node-sdk/src/rpc.ts:200,215,236` vs `rust-ody/crates/ody-host/src/config.rs` / `main.rs`.  
**Details**: `createExternalTransport` builds argv `[binaryPath, 'serve', '--stdio', ...]`. The Rust `clap` parser accepts only global flags (`--stdio`, `--socket-path`, ...). Running `ody-host serve --stdio` yields:

```text
error: unexpected argument 'serve' found
```

### B2 — Missing cross-language RPC test

**Severity**: P0 — required by `transport.md B7` / `packaging.md D7`.  
**Location**: `packages/node-sdk/test/rust-host-connect.test.ts` should exist.  
**Details**: Existing tests (`sdk-rpc-client-connect.test.ts`, `sdk-rpc-client-connect-binary.test.ts`) test against TS `createCoreServer` or mock scripts, not the real `ody-host` binary.

### B3 — Node version / `#/foo` imports deprecation

**Severity**: P0 — blocks source-run and SEA build on Node 22; deprecated on Node 24.  
**Location**: `packages/node-sdk/src/index.ts`, `apps/ody-code/src/host/index.ts`, and many other files.  
**Details**: Node 22 throws `Invalid module "#/kimi-harness" is not a valid internal imports specifier name`. Node 24 accepts it but emits `DEP0166` deprecation warning. Project `.nvmrc` requires `24.15.0`, so the immediate fix is to run on Node 24, but the long-term fix is to migrate `#/foo` to `#foo`.

### B4 — Workspace typecheck failures

**Severity**: P1 — blocks the "all green" invariant.  
**Location**: `apps/vis/web` references `packages/agent-core`, `packages/e2e-testing`, `packages/mcp-host`.  
**Details**: Unused variables and a wasm-loader type error. These are pre-existing but prevent `pnpm -r typecheck` from passing.

### B5 — agent-core extraction residue

**Severity**: P1 — Phase 1-C not fully complete, leaving the reversal surface larger than planned.  
**Location**: `packages/agent-core/src/office-hours/state.ts`, `tools/builtin/code-review/`, `tools/builtin/e2e/`, `skill/builtin/mcp-config.ts`.  
**Details**: `code-review`, `mcp-host`, and `e2e-testing` are already standalone packages, but their original code still lives in `agent-core`.

### B6 — Rust host CoreAPI surface is minimal

**Severity**: P2 — expected for a prototype, but limits the TUI smoke to basic session/chat only.  
**Location**: `rust-ody/crates/ody-host/src/host.rs:58-70`.  
**Details**: Only `getCoreInfo`, `createSession`, `resumeSession`, `listSessions`, `closeSession`, `chat`, `getConfig`, `setConfig`, `getExperimentalFlags` are implemented.

---

## 4. Fixup Tasks

### Phase A — Unblock Real TS -> Rust Connection (P0)

#### A1: Decide and implement the CLI contract

**Goal**: TS and Rust agree on how to launch the host.

**Options**:

| Option | Change | Pros | Cons |
|---|---|---|---|
| A1a — Rust adopts `serve` subcommand | Add `serve` command to `ody-host` clap setup; global flags move under the subcommand | Zero TS changes; matches existing `ody serve` convention in TS | Duplicates the TS `serve` command concept in Rust; more CLI surface |
| A1b — TS stops passing `serve` | Change `createExternalTransport` argv from `['serve', '--stdio', ...]` to `['--stdio', ...]` (and likewise for socket/tcp) | Minimal Rust changes; matches Rust host's current CLI | Requires updating any docs/tests that assumed `serve` |

**Recommendation**: **A1b** — the Rust host is a standalone binary, not a subcommand runner; aligning TS spawn args to the actual binary surface is the smaller, more honest change.

**Files**:
- `packages/node-sdk/src/rpc.ts` (lines ~200, 215, 236)
- `packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts` (update mock scripts if they relied on `serve`)
- `docs/designs/rust-host-reversal-adr.md` (record final contract)

**Acceptance**:
- `ody-host --stdio` launched by `SDKRpcClient.connect({ transport: 'stdio', binaryPath })` stays alive.
- `ody-host --socket-path /tmp/x.sock` launched by `{ socketPath, spawn: true }` stays alive and emits ready message.
- `ody-host --tcp-host 127.0.0.1 --tcp-port 0` launched by `{ host, port, spawn: true }` stays alive and emits ready message.

---

#### A2: Create real cross-language RPC test

**Goal**: Verify TS client <-> Rust host over stdio and UDS.

**File**: `packages/node-sdk/test/rust-host-connect.test.ts`

**Scope**:
- Spawn `rust-ody/target/release/ody-host --stdio` via `SDKRpcClient.connect({ transport: 'stdio', binaryPath })`.
- Call `client.createSession({ workDir, id })`, assert returned `id` matches.
- Call `client.listSessions(...)`, assert the created session appears.
- Spawn UDS mode, connect, create a session, close.
- Close the client and assert the host process exits cleanly.

**Test data**: use `tmpDir` + deterministic session id; no LLM call required.

**Acceptance**:
- `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts` passes locally on Node 24.
- The test is added to `.github/workflows/rust-host.yml` without `continue-on-error`.
- The CI step uploads logs on failure.

---

#### A3: Re-verify on Node 24.15.0

**Goal**: Establish the canonical runtime for Phase 3.

**Steps**:
1. Switch to Node 24.15.0 (`nvm use 24.15.0` or equivalent).
2. `pnpm install --frozen-lockfile`.
3. Run:
   - `cargo test -p ody-host`
   - `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts`
   - `pnpm run proto:rust-host` (manual smoke — create a session, exit)
4. Record results in ADR.

**Acceptance**:
- Rust tests pass.
- Cross-language test passes.
- TUI starts against Rust host and survives session creation.

---

### Phase B — Harden Build & CI (P1)

#### B1: Verify SEA build with embedded `ody-host`

**Goal**: Confirm the native-asset path works end-to-end.

**Steps**:
1. On Node 24, run:
   ```bash
   pnpm run build:host
   pnpm --filter ody-code run build:native:sea
   pnpm --filter ody-code run test:native:smoke
   ```
2. Confirm build log contains `Collected host binary: ... -> host/darwin-arm64/ody-host`.
3. Confirm smoke output contains `Native asset smoke passed: ...` including `ody-host`.

**Acceptance**:
- SEA build produces a binary.
- Native smoke passes.
- If blocked by imports, record whether the blocker is `#/foo` deprecation or something else.

---

#### B2: Fix or scope workspace typecheck

**Goal**: `pnpm -r typecheck` is green, or failures are explicitly scoped and ticketed.

**Options**:
- Fix `apps/vis/web` TypeScript errors (unused variables, wasm-loader `instance` property).
- If `apps/vis/web` is not part of the Phase 3 critical path, temporarily exclude it from `pnpm -r typecheck` and document the exception.

**Acceptance**:
- `pnpm -r typecheck` exits 0, OR
- There is a committed `.ody-code/notes/typecheck-exceptions.md` explaining which packages are excluded and why.

---

#### B3: Remove `continue-on-error` from Rust host CI

**Goal**: CI treats cross-language test as a hard gate once A2 is done.

**File**: `.github/workflows/rust-host.yml`

**Acceptance**:
- Cross-language step has `continue-on-error: false` (or the line removed).
- CI fails if `rust-host-connect.test.ts` fails.

---

### Phase C — Finish agent-core Extraction (P1)

#### C1: Extract remaining office-hours state store

**Goal**: Move `packages/agent-core/src/office-hours/state.ts` to a peer package or a shared location.

**Options**:
- Move to `@odysseythink/agent-core-shared` if callers need it.
- Move to `@odysseythink/office-hours` if that package exists or is created.
- If office-hours mode is being retired, delete the store and its callers.

**Acceptance**:
- `packages/agent-core/src/office-hours/` no longer exists.
- All imports updated.
- Tests pass.

---

#### C2: Extract code-review builtin tool

**Goal**: Move `packages/agent-core/src/tools/builtin/code-review/` into `@odysseythink/code-review`.

**Acceptance**:
- `packages/agent-core/src/tools/builtin/code-review/` removed.
- `packages/code-review/src/` absorbs the logic if not already present.
- `agent-core` depends on `@odysseythink/code-review` for the tool implementation.
- No circular dependencies (run `madge` / `dpdm`).

---

#### C3: Extract e2e builtin tool

**Goal**: Move `packages/agent-core/src/tools/builtin/e2e/` into `@odysseythink/e2e-testing`.

**Acceptance**:
- `packages/agent-core/src/tools/builtin/e2e/` removed.
- `packages/e2e-testing/src/` absorbs the logic.
- `agent-core` depends on `@odysseythink/e2e-testing` for the tool implementation.

---

#### C4: Extract MCP config skill

**Goal**: Move `packages/agent-core/src/skill/builtin/mcp-config.ts` into `@odysseythink/mcp-host`.

**Acceptance**:
- File removed from `agent-core`.
- `mcp-host` exposes the skill if needed.
- No `agent-core` -> `mcp-host` cycle.

---

### Phase D — Prototype Deepening (P2)

#### D1: Manual TUI stdio and socket smoke

**Goal**: Confirm the full user path works.

**Commands**:
```bash
# stdio
pnpm run proto:rust-host

# socket
SOCKET=$(mktemp -u)
pnpm -C apps/ody-code run dev:cli-only -- \
  --host=rust --host-socket "$SOCKET" \
  --host-binary $(pwd)/rust-ody/target/release/ody-host \
  --home $(mktemp -d)
```

**Acceptance**:
- TUI launches.
- Create a session.
- Send a simple prompt (expect either LLM response or clear "no provider configured" error).
- Exit cleanly without crashing.
- Record results in ADR.

---

#### D2: Expand Rust host CoreAPI (optional for prototype)

**Goal**: Implement enough methods for basic TUI functionality beyond session creation.

**Priority order**:
1. `prompt` / `steer` (chat round-trip already partially works via `chat`, but needs TS method mapping).
2. `setModel`, `setThinking`, `setPermission`.
3. `getConfig`, `setConfig` (already stubbed).
4. `getContext`, `getUsage`, `getStatus`.
5. `listSkills`, `activateSkill`.
6. OAuth / MCP (defer — out of prototype scope per ADR).

**Acceptance**:
- Each added method has a Rust unit test and a cross-language test.
- TUI can perform a full turn without "unknown method" errors.

---

#### D3: Address `#/foo` imports deprecation

**Goal**: Future-proof against Node versions after 24.

**Approach**:
- Migrate `packages/*/package.json` imports from `"#/*": [...]` to `"#*": [...]`.
- Update all source imports from `#/foo` to `#foo`.
- This is a wide-ranging refactor; do it **after** Phase 3 acceptance, as a separate PR.

**Acceptance**:
- No `DEP0166` warnings under Node 24.
- Tests and typecheck still pass.

---

## 5. Execution Order & Dependencies

```
A1 (CLI contract) ──┐
A2 (cross-lang test)├─► A3 (Node 24 verify) ──► B1 (SEA verify) ──► D1 (manual smoke)
Node 24 switch ─────┘       │
                            ▼
                     B2 (typecheck) ──► B3 (CI hard gate)
                            │
                            ▼
                     C1-C4 (agent-core extraction)
                            │
                            ▼
                     D2 (expand CoreAPI) ──► D3 (#/foo migration)
```

**Parallel tracks**:
- Phase A and Phase C can mostly run in parallel.
- Phase B depends on A.
- Phase D depends on A and B.

---

## 6. Time Estimate

| Phase | Tasks | Estimated Effort |
|---|---|---|
| A — Unblock connection | A1, A2, A3 | 0.5–1 day |
| B — Build/CI | B1, B2, B3 | 0.5–1 day |
| C — Package extraction | C1-C4 | 2–3 days (most churn) |
| D — Prototype deepen | D1, D2, D3 | 1–2 days |
| **Total** | | **4–7 days** |

---

## 7. Acceptance Criteria for "Phase 3 Complete"

1. `cargo test -p ody-host` passes.
2. `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts` passes on Node 24.
3. `pnpm run proto:rust-host` launches the TUI and allows session creation.
4. `pnpm --filter ody-code run build:native:sea && pnpm --filter ody-code run test:native:smoke` passes.
5. `pnpm -r typecheck` is green (or has documented exceptions).
6. `.github/workflows/rust-host.yml` cross-language step is a hard gate.
7. ADR is updated with verified results and a clear Go/No-Go recommendation.

---

## 8. Risks & Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Node 24 still has issues with `#/foo` beyond deprecation | High | Switch to `#foo` migration early; test on Node 24 nightly if possible |
| `agent-core` extraction creates circular dependencies | Medium | Run `madge` after each move; keep changes per package in separate commits |
| SEA binary size regressions unacceptable | Medium | ADR already records ~3.4 MB; re-measure and use as No-Go data |
| OAuth/MCP in Rust host proves infeasible | Low | Out of prototype scope; document as future gate for Phase 4 |
| TUI depends on methods not yet in Rust host | Medium | Phase D2 implements priority methods incrementally |

---

## 9. Go/No-Go Recommendation Gate

After Phase A + B + D1, reconvene the Go/No-Go decision:

- **Go** if: cross-language test passes, TUI smoke passes, SEA size acceptable, team accepts maintaining dual session/tool logic.
- **No-Go** if: cross-language RPC latency/fragility is unacceptable, binary size regression is rejected, or OAuth/MCP path looks infeasible for Phase 4.

If No-Go: keep `ody-host` behind an experimental flag (`flags.enabled('rust-host')`), revert `--host=rust` from default CLI help, and archive the crate as experimental.

---

## 10. References

- Original roadmap: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md`
- Phase 3 plan: `.ody-code/plans/2026-06-25-backend-architecture-evolution-phase3.md`
- Part 1 (core): `.ody-code/plans/2026-06-25-backend-architecture-evolution-phase3/core.md`
- Part 2 (transport): `.ody-code/plans/2026-06-25-backend-architecture-evolution-phase3/transport.md`
- Part 3 (TUI): `.ody-code/plans/2026-06-25-backend-architecture-evolution-phase3/tui.md`
- Part 4 (packaging): `.ody-code/plans/2026-06-25-backend-architecture-evolution-phase3/packaging.md`
- ADR: `docs/designs/rust-host-reversal-adr.md`
