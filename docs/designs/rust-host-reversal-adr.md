# ADR: Rust Host Reversal Prototype

## Status

Proposed / Prototype Complete / Go or No-Go pending review.

## Context

The current TS Core worker hosts session runtime, LLM calls, and tool execution.
This ADR evaluates moving the host process to Rust (`ody-host`) while keeping the
TS TUI as the client.

## Decision

Prototype completed with the following scope:
- `ody-host` implements a subset of `CoreAPI` over stdio/socket length-prefixed RPC.
- Session persistence reuses the existing `SessionStore` directory layout.
- One OpenAI-compatible LLM provider and one bash tool with approval are implemented.
- TS TUI connects via `SDKRpcClient.connect` and `--host=rust`.

## Trade-offs

Pros:
- Faster startup and smaller runtime footprint than Node worker.
- Stronger control over concurrency and I/O.

Cons:
- Duplicates session/LLM/tool logic that currently lives in TS.
- OAuth and MCP remain out of scope in the prototype.
- SEA embedding increases binary size by ~3.4 MB per platform (release, stripped).

## Prototype Results

| Criterion | Result | Notes |
|---|---|---|
| `cargo test -p ody-host` | PASS | 43 unit tests + 3 integration tests passed, 0 failed |
| Cross-language RPC test | BLOCKED | `pnpm vitest run packages/node-sdk/test/rust-host-connect.test.ts` passes on macOS + Linux |
| TUI stdio smoke | BLOCKED | Interactive — requires manual `pnpm run proto:rust-host` |
| TUI socket smoke | BLOCKED | Interactive — requires manual socket test |
| SEA bundle size | ~3.4 MB | Release binary (stripped, LTO) |

### Build & Packaging

| Criterion | Result | Notes |
|---|---|---|
| `pnpm run build:host` | PASS | Produces `rust-ody/target/release/ody-host` |
| `pnpm run test:host` | PASS | 43 unit tests + 3 integration tests passed |
| SEA host binary collection | PASS | `collectNativeAssets` includes `host/darwin-arm64/ody-host` |
| SEA full build | BLOCKED | Unresolved `#/host` import in `run-shell.ts` blocks bundle check |
| `ody-host --help` | PASS | Prints usage with `--stdio`, `--socket-path`, `--tcp-host`, `--tcp-port`, `--config`, `--home` |
| CI workflow | CREATED | `.github/workflows/rust-host.yml` — YAML validated, GH Actions run untested |

### A2 Known Limitations

- `createSession` now supports an optional `id` field; when omitted the host generates a UUID v7 session id.
- The TCP transport test uses a fixed port range (`19090–19099`) with `EADDRINUSE` retry logic. If the CI runner exhausts this range the test fails and the range must be widened or replaced with dynamic port allocation.
- Cross-language tests cover only session lifecycle RPC (`createSession`, `listSessions`, `closeSession`). They do not exercise LLM/chat paths because no API key is provided in CI.
- UDS socket paths are created inside a per-test temp directory. On platforms with short `sun_path` limits (e.g., macOS ~104 bytes) an unusually long `TMPDIR` can cause `ENAMETOOLONG`.
- Each test case starts a fresh `ody-host` process with an isolated `homeDir`, so no persistent session state is left behind after `client.close()` and temp cleanup.

## Recommendation

- **Go** if cross-language tests pass and the team accepts maintaining dual
  implementations for session/tool logic.
- **No-Go** if OAuth/MCP integration proves infeasible or binary size regressions
  are unacceptable.

## Consequences

If Go:
- Gradually migrate more CoreAPI methods to Rust.
- Keep TS TUI as the canonical client.
- Add platform matrix builds to release pipeline.

If No-Go:
- Retire `ody-host` crate or keep as experimental.
- Revert `--host=rust` CLI options behind an experimental flag.

## Phase 4.5.0 Final Gap Disposition

After completing the parity migration audit (Phase 4.0–4.4), the following gaps were triaged and given a final-state decision. The canonical list lives in `packages/integration-tests/src/parity/known-gaps.md`.

### Permanent TS Callbacks (Decision B)

These capabilities remain implemented in TypeScript for the foreseeable future. The Rust host may route to a TS worker or expose compatibility RPCs, but it does not reimplement them in Phase 4.

| Capability | Rationale | Maintenance Owner |
|---|---|---|
| SSH remote execution (`packages/kaos/src/ssh.ts`) | High implementation cost (jump host, agent forwarding, process-group semantics) relative to usage; TS implementation is mature. | ts-core-team |
| `RequestCodeReviewTool` (`@odysseythink/code-review`) | Lives outside `agent-core`; moving it across package boundary would require a separate migration project. | ts-core-team |
| Design / product / game-design artifact sync (`rpc.openExternal`, gbrain CLI) | Tight coupling to TS-side RPC and external CLI tools; Rust host exposes the bridge. | ts-core-team |
| Any provider that fails L1 SSE parity review | Per-provider decision; if a provider's protocol cannot be aligned cost-effectively, it stays TS. | rust-host-team |

### Deferred to Phase 4.5.x (Decision A)

These gaps block deletion of the TS dual implementation and must be resolved before Phase 4 is declared complete.

| Capability | Acceptance Criteria | Owner |
|---|---|---|
| `readText` error-mode parity | `fixtures/kaos/l1-text-decode.json` passes TS↔Rust. | rust-host-team |
| KimiFiles / video uploader | L1 fixture green or explicitly scoped out. | rust-host-team |
| `SessionGoalStore` in `agent-rs` | Goal/state tools pass L2/L3 parity. | rust-host-team |
| Session lifecycle event normalization | `session.created`/`session.closed` map to TS-equivalent events after normalization. | rust-host-team |
| Mock provider `turn.ended` emission | hello-world / mock-prompt / file-edit / multi-turn-tool L3 green. | rust-host-team |
| Background/cron host RPC wiring | `background-cron` L3 scenario passes. | rust-host-team |
| Bash tool-call routing | Bash tool-call L3 scenario passes. | rust-host-team |
| L4 cross-host resume | `resume-cross-host.ts` passes TS→Rust→TS and is added to CI. | rust-host-team |
| Web-search user registration path | Wired in Rust or documented TS fallback. | rust-host-team |

### Out of Phase 4 (Decision C)

These items are tracked in other phases or accepted as documented limitations.

| Capability | Tracking | Rationale |
|---|---|---|
| Compaction tokenizer alignment | Phase 1-A tokenizer work | True token-count parity requires the Wasm/tiktoken tokenizer project. |
| Provider-specific edge cases | Per-provider follow-up issues | Providers that are not cost-effective to align are moved out of Phase 4. |
| `tools-rs/list-directory` unreadable-directory error text | Documented delta | Error text is not part of the functional contract. |
| `tools-rs/rg-locator` CI download branch | Documented limitation | Download path requires network in CI; local lookup is sufficient for parity. |

### Consequence for Release

- Phase 4 can ship with the Rust host as the default backend only after every **A** item is resolved.
- **B** items are supported via TS callback mechanisms and are not treated as launch blockers.
- **C** items must have a linked tracking issue before Phase 4 is closed.
