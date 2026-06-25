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
| `cargo test -p ody-host` | PASS | 1 unit test passed |
| Cross-language RPC test | N/A | `packages/node-sdk/test/rust-host-connect.test.ts` not yet created |
| TUI stdio smoke | PENDING | Requires D7 verification |
| TUI socket smoke | PENDING | Requires D7 verification |
| SEA bundle size | ~3.4 MB | Release binary (stripped, LTO) |

### Build & Packaging

| Criterion | Result | Notes |
|---|---|---|
| `pnpm run build:host` | PASS | Builds `rust-ody/target/release/ody-host` |
| `pnpm run test:host` | PASS | 1 unit test passed |
| SEA host binary collection | PASS | `collectNativeAssets` includes `host/darwin-arm64/ody-host` |
| SEA full build | BLOCKED | Unresolved `#/host` import in `run-shell.ts` blocks bundle check |
| CI workflow | CREATED | `.github/workflows/rust-host.yml` — untested in GH Actions |

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
