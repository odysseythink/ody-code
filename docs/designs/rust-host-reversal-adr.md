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
