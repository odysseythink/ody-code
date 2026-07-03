# Phase 4.4.3 Background & Cron Management Tools Implementation Plan

**Goal:** Migrate the six background & cron management builtin tools (`TaskList`, `TaskOutput`, `TaskStop`, `CronCreate`, `CronList`, `CronDelete`) from TypeScript to Rust in `tools-rs`, including the cron expression parser, jitter, and in-memory store infrastructure. Prove equivalence via L1 golden parity fixtures; defer real `BackgroundManager`/`CronManager` integration to 4.3.8 / 4.4.8.

**Architecture:** Add `background` and `cron` submodules to `tools-rs/src/builtin/`. Define internal `BackgroundManager`/`CronManager` trait interfaces consumed by the tools, with mock implementations for L1 testing. Implement the cron infrastructure (expression parser, deterministic jitter, time formatting, clock sources) as pure utility modules under `tools-rs/src/cron/`. Each tool implements the existing `BuiltinTool` trait. L1 parity uses shared JSON fixtures via the `tools-golden` binary and TS `tools-rs-golden` runner.

**Tech Stack:** Rust 2021 (`tokio`, `serde`, `chrono` for timezone-aware date math), `tools-rs` existing infrastructure (`BuiltinTool` trait, `InputSchema`, `ToolResultBuilder`, `AbortSignal`), TypeScript/Vitest for parity harness.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/tools-rs/
  Cargo.toml                                  # add chrono dependency
  src/lib.rs                                   # expose pub mod cron; pub mod builtin::background, pub mod builtin::cron
  src/cron/
    mod.rs                                     # pub mod cron_expr, jitter, time_format, clock
    cron_expr.rs                               # parse + computeNextCronRun + cronToHuman
    jitter.rs                                  # deterministic jitter for fire times
    time_format.rs                             # ISO 8601 local time formatting
    clock.rs                                   # ClockSources abstraction
  src/builtin/
    mod.rs                                     # add pub mod background, pub mod cron
    background/
      mod.rs                                   # BackgroundManager trait + BackgroundTaskInfo types
      task_list.rs                             # TaskListTool
      task_output.rs                           # TaskOutputTool
      task_stop.rs                             # TaskStopTool
    cron/
      mod.rs                                   # CronManager trait + SessionCronStore
      cron_create.rs                           # CronCreateTool
      cron_list.rs                             # CronListTool
      cron_delete.rs                           # CronDeleteTool
  src/golden.rs                                # add 6 new Op variants
packages/integration-tests/src/parity/
  fixtures/tools-rs/
    background-cron-tools.json                 # L1 fixture for all 6 tools
  tools-rs-golden.ts                           # extend GoldenOp + runCase for new ops
  known-gaps.md                                # add 4.4.3 deferred items
packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts   # register new fixture
.github/workflows/rust-host.yml                # ensure golden job covers new fixture
```

## Dependency Overview

```
Part 1: infra.md (BackgroundManager/CronManager traits + cron-expr + jitter + time-format + clock + SessionCronStore)
  │
  ├──► Part 2: background-tools.md (TaskListTool + TaskOutputTool + TaskStopTool)
  └──► Part 3: cron-tools.md (CronCreateTool + CronListTool + CronDeleteTool)
              │
              ▼
        Part 4: parity-ci.md (Golden ops + fixtures + parity runner + test + CI + known-gaps)
```

- **Phase A** (Part 1): Foundation — trait interfaces, cron expression parsing, time utilities. Sequential tasks within.
- **Phase B** (Parts 2–3): Tools — can run in parallel after Part 1 completes.
- **Phase C** (Part 4): Parity & CI — depends on all tools being implemented.

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `BackgroundManager`/`CronManager` traits defined in tools-rs may need adjustment when 4.3.8 delivers real implementations. | Traits are minimal (only the methods tools call). Real impls in `agent-rs` can implement these traits since `agent-rs` depends on `tools-rs`. |
| `cron-expr` parser complexity — TS implementation is 451 lines with many edge cases (wildcards, ranges, steps, named months/days). | Test-first with exhaustive fixture covering all cron syntax variants. Port the TS test cases directly. |
| Jitter must produce BIT-IDENTICAL results to TS for the same task ID. | Use the same algorithm (fraction from hex ID). L1 fixture compares computed `nextFireAt` for known IDs. |
| `chrono` crate brings timezone data — may affect binary size. | Use `chrono` with `Local::now()`; no IANA database needed for cron math (UTC-based epoch + local offset). |
| Real `BackgroundManager` integration (4.3.8) will need process management, output streaming, persistence — far beyond mock scope. | Part 1 defines only the query/control interface tools need. Real impl in 4.3.8 handles lifecycle. |

## Spec-Coverage Table

| Roadmap § | Requirement | Part:Task(s) | Status |
|---|---|---|---|
| 4.4.3.1 | `TaskListTool` | Part 2 Task 1 | covered |
| 4.4.3.2 | `TaskOutputTool`: output preview, paging hint | Part 2 Task 2 | covered |
| 4.4.3.3 | `TaskStopTool`: reason, terminal state | Part 2 Task 3 | covered |
| 4.4.3.4 | `CronCreateTool`: parse/validate/jitter/cap | Part 3 Task 1 | covered |
| 4.4.3.5 | `CronListTool` | Part 3 Task 2 | covered |
| 4.4.3.6 | `CronDeleteTool` | Part 3 Task 3 | covered |
| 4.4.3.7 | L3 fixture (background/cron event stream) | deferred to 4.4.8 | GAP |
| — | cron-expr parser (parse, computeNext, cronToHuman) | Part 1 Task 2 | covered |
| — | jitter (deterministic forward/pull-forward) | Part 1 Task 3 | covered |
| — | time-format (ISO 8601 local with offset) | Part 1 Task 4 | covered |
| — | clock sources abstraction | Part 1 Task 4 | covered |
| — | SessionCronStore (in-memory with ID gen) | Part 1 Task 1 | covered |
| — | BackgroundManager/CronManager trait interfaces | Part 1 Task 1 | covered |
| — | Real BackgroundManager/CronManager integration | deferred to 4.3.8 | GAP |

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-4-3/infra.md` | BackgroundManager/CronManager traits + SessionCronStore + cron-expr + jitter + time-format + clock | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-4-3/background-tools.md` | TaskListTool + TaskOutputTool + TaskStopTool | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-4-3/cron-tools.md` | CronCreateTool + CronListTool + CronDeleteTool | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-4-3/parity-ci.md` | Golden ops + fixtures + parity runner + test + CI + known-gaps | done |
