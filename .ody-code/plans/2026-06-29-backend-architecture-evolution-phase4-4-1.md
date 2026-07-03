# Phase 4.4.1 File & Shell Core Tools Implementation Plan

**Goal:** Migrate the seven file & shell core builtin tools (`Read`, `Write`, `Edit`, `Glob`, `Grep`, `ReadMediaFile`, `Bash` foreground) from TypeScript to Rust in `tools-rs`, and prove equivalence via L1 golden parity fixtures.

**Architecture:** Extend `tools-rs` with an async `BuiltinTool` trait and `ToolExecution`/`ExecutableToolContext` contracts, then implement each tool as a Rust struct that reuses the existing 4.4.0 infrastructure (path policy, input schema, result builder, file-type sniff, rg locator) and `kaos-rs` for filesystem/process I/O. L1 parity is driven by shared JSON fixtures consumed by both the Rust `tools-golden` binary and the TypeScript `tools-rs-golden` runner.

**Tech Stack:** Rust 2021 (`tokio`, `serde`, `async-trait`), `kaos-rs`, `tools-rs`, TypeScript/Vitest for the parity harness.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/tools-rs/
  Cargo.toml                              # add async-trait + base64
  src/lib.rs                              # expose pub mod builtin
  src/builtin/mod.rs                      # BuiltinTool trait, ToolExecution, ExecutableToolContext
  src/builtin/line_endings.rs             # detect/style/materialize helpers
  src/builtin/read.rs                     # ReadTool
  src/builtin/write.rs                    # WriteTool
  src/builtin/edit.rs                     # EditTool
  src/builtin/glob.rs                     # GlobTool
  src/builtin/grep.rs                     # GrepTool
  src/builtin/read_media.rs               # ReadMediaFileTool
  src/builtin/bash.rs                     # BashTool (foreground path only)
  src/golden.rs                           # extend Op enum with tool ops
packages/agent-core/package.json          # add subpath export ./tools/builtin/file-shell-core
packages/integration-tests/src/parity/
  fixtures/tools-rs/
    read-tool.json
    write-tool.json
    edit-tool.json
    glob-tool.json
    grep-tool.json
    read-media-tool.json
    bash-tool.json
  tools-rs-golden.ts                      # extend GoldenOp + runCase for tool ops
  known-gaps.md                           # add 4.4.1 deferred items
packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts   # register new fixtures
.github/workflows/rust-host.yml         # ensure tools-rs golden job runs new fixtures
```

## Dependency Overview

```
4.4.0 infrastructure (done)
  │
  ▼
Part 1: trait-read.md  ──► defines BuiltinTool trait + ReadTool
  │
  ├──► Part 2: write-edit.md  ──► line-endings + WriteTool + EditTool
  ├──► Part 3: glob-grep.md   ──► GlobTool + GrepTool
  └──► Part 4: media-bash.md  ──► ReadMediaFileTool + BashTool foreground
              │
              ▼
        Part 5: parity-ci.md  ──► L1 golden fixtures + parity runner + test + CI/known-gaps
```

- **Phase A** (Parts 1–4): Part 1 is the hard prerequisite because it defines the `BuiltinTool` trait and `ToolExecution` contract. Parts 2–4 are independent after Part 1 and can be developed in parallel.
- **Phase B** (Part 5): Depends on all tools being implemented. Produces the L1 golden parity gate.

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `BuiltinTool` trait was not delivered in 4.4.0; adding it now changes the crate surface. | Consolidate the trait + first tool in Part 1; the rest of the crate has no existing callers of a tool trait, so churn is contained. |
| L3 event-stream parity requires the agent loop (4.3.5), which is not ready. | 4.4.1 covers L1 golden only; L3 is explicitly deferred to the 4.4.8 integration gate and marked in `known-gaps.md`. |
| `BashTool` background execution needs `BackgroundManager` (4.3.8). | Implement foreground path only; `run_in_background=true` returns a clear error, deferred to 4.4.3. |
| `WriteTool`/`EditTool` plan/design-mode path redirection needs `SessionMode` (4.3.7). | Omit redirection in 4.4.1; write to the resolved path directly, deferred to 4.4.5/4.3.7. |
| `ReadMediaFileTool` video upload needs a provider video uploader (4.2.x). | Inline base64 `video_url` fallback; provider-specific upload deferred. |
| `GrepTool` depends on a working `rg` binary in CI. | Reuse `rg_locator::ensure_rg_path`; tests mock PATH or rely on pre-installed `rg`. |

## Spec-Coverage Table

| Roadmap § | Requirement | Task(s) | Status |
|---|---|---|---|
| 4.4.1.1 | `ReadTool`: line/offset/n_lines, line-endings, truncation, binary rejection | Part 1 Task 2 | covered |
| 4.4.1.2 | `WriteTool`: overwrite/append, parent dir check, byte count | Part 2 Task 2 | covered |
| 4.4.1.3 | `EditTool`: replace_once/all, line-end materialize, uniqueness check | Part 2 Task 3 | covered |
| 4.4.1.4 | `GlobTool`: brace expansion, MAX_MATCHES, include_dirs, mtime sort | Part 3 Task 1 | covered |
| 4.4.1.5 | `GrepTool`: rg args, output modes, sensitive filtering, pagination | Part 3 Task 2 | covered |
| 4.4.1.6 | `ReadMediaFileTool`: image/video detection, base64, dimensions | Part 4 Task 1 | covered |
| 4.4.1.7 | `BashTool` foreground: timeout, abort, SIGTERM→SIGKILL, env | Part 4 Task 2 | covered |
| 4.4.1.8 | L1 + L3 fixtures | L1: Part 5; L3: deferred to 4.4.8 (agent not ready) | partial |
| — | `WriteTool`/`EditTool` plan/design-mode path redirect | deferred to 4.4.5/4.3.7 | GAP |
| — | `BashTool` background execution | deferred to 4.4.3/4.3.8 | GAP |
| — | `ReadMediaFileTool` provider video uploader | deferred to 4.2.x | GAP |

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-4-1/trait-read.md` | `BuiltinTool` trait + `ReadTool` | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-4-1/write-edit.md` | line-endings helper + `WriteTool` + `EditTool` | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-4-1/glob-grep.md` | `GlobTool` + `GrepTool` | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-4-1/media-bash.md` | `ReadMediaFileTool` + `BashTool` foreground | done |
| 5 | `2026-06-29-backend-architecture-evolution-phase4-4-1/parity-ci.md` | L1 golden fixtures + parity runner + test + CI/known-gaps | done |

## Known Gaps

| Gap | Deferred To | Rationale |
|---|---|---|
| `BashTool` background execution (`run_in_background`) | 4.4.3 / 4.3.8 | Requires `BackgroundManager` and `TaskOutput`/`TaskStop` in Rust host |
| `WriteTool` / `EditTool` plan/design-mode path redirect | 4.4.5 / 4.3.7 | Requires `SessionMode` integration in Rust host |
| `ReadMediaFileTool` video uploader (provider-side) | 4.2.x | Inline base64 fallback is implemented; provider upload pipeline is a separate phase |
| Video dimensions in `ReadMediaFileTool` | Async follow-up | `infer` detects video type but `image` crate cannot decode video dimensions; needs a video decoder or dimensions API |
| L3 event-stream parity | 4.4.8 | Agent loop not ready in Rust host; full tool invocation through the event-stream transport |
| `GrepTool` `include_ignored` flag | 4.4.1 (partial) | `rg --no-ignore` not yet exposed via builtin input schema; can be added once permission model supports it |
