# Phase 4.4.5 Session-Mode Workflow Tools Implementation Plan

**Goal:** Migrate all session-mode workflow builtin tools (planning enter/exit, office-hours, game-design) from TypeScript to Rust in `tools-rs`, wire them into `agent-rs`, and prove equivalence via L1/L3 parity tests.

**Architecture:** Add a `session_mode` submodule to `tools-rs/src/builtin/` that defines minimal trait boundaries for agent-provided capabilities (session mode state machine, i18n, state stores, kaos, telemetry, MCP). `agent-rs` depends on `tools-rs`, implements these traits, and bridges `tools_rs::builtin::BuiltinTool` to `agent_loop::types::ExecutableTool`. The session-mode tools are instantiated with trait-object references so they remain testable with mocks, and are registered in `agent-rs::tool::manager::ToolManager` alongside the existing collaboration tools.

**Tech Stack:** Rust 2021 (`tokio`, `serde`, `chrono`, `async-trait`), `tools-rs` existing infrastructure (`BuiltinTool` trait, `InputSchema`, `ToolExecution`), `agent-rs` subsystems (`SessionModeManager`, `Kaos`, `AgentEnvironment`), TypeScript/Vitest for parity harness.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/tools-rs/
  src/builtin/mod.rs                            # add pub mod session_mode; extend ToolExecution with display
  src/builtin/session_mode/
    mod.rs                                      # SessionModeProvider + StateStore + McpProvider + I18n traits
    planning.rs                                 # EnterPlanMode / ExitPlanMode / EnterDesignMode / ExitDesignMode
    office_hours.rs                             # Enter/Exit/SetLanguage/AppendLearning/AppendProfile/Search/EnsureRouting/SyncArtifact
    game_design.rs                              # Enter/Exit/SetLanguage/AppendLearning/AppendProfile/Search/EnsureRouting/SyncArtifact
    i18n.rs                                     # en/zh string tables for office-hours/game-design
  src/bin/tools-golden.rs                       # add Op variants for session-mode tools
rust-ody/crates/agent-rs/
  src/session_mode/
    manager.rs                                  # handoff_to(label), behavior registry in Agent build
    types.rs                                    # PendingDesignHandoff / PendingPlanHandoff carry selected_label
  src/agent.rs                                  # user_language, state store fields, set_user_language, provider wiring
  src/tool/
    bridge.rs                                   # forward ToolExecution.display
    manager.rs                                  # register session-mode tools in core_builtin_tools / loop_tools
    session_mode_provider.rs                    # AgentSessionModeProvider impl
    state_store.rs                              # FileSystemOfficeHoursStateStore / FileSystemGameDesignStateStore
    i18n.rs                                     # translations for agent-side strings
    session_mode_toolkit.rs                     # build_tools() called by TurnTools
  src/turn/types.rs                             # TurnSessionMode.data() if needed
packages/integration-tests/src/parity/
  fixtures/tools-rs/session-mode-tools.json     # L1 fixture
  tools-rs-golden.ts                            # extend GoldenOp + runCase
packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts
.github/workflows/rust-host.yml
```

## Dependency Overview

```
Part 1: infra.md (shared trait boundaries + ToolExecution.display + handoff label + behavior registry)
  │
  ├──► Part 2: planning-tools.md (Enter/Exit Plan/Design)
  │
  ├──► Part 3: office-hours-tools.md (8 office-hours tools)
  │
  ├──► Part 4: game-design-tools.md (8 game-design tools)
  │
  └──► Part 5: integration.md (state stores, i18n, agent wiring, ToolManager registration, L1/L3 parity)
```

- **Part 1** is the hard prerequisite: it defines the `SessionModeProvider` trait surface, extends `ToolExecution` with the `display` field needed by exit tools, adds `selected_label` to handoff payloads, and fixes `SessionModeManager` to use the default behavior registry. These are shared-signature changes; every caller is updated in the same task.
- **Parts 2–4** are independent once Part 1 lands. Each implements one family of tools in `tools-rs` with mock providers and unit tests.
- **Part 5** wires real implementations into `agent-rs`, registers tools, and runs L1/L3 parity. It depends on Parts 1–4. Internal order: Task 1 → Task 2 → Task 3 → Task 4 → Task 5 → Task 6 → Task 7 → Task 8 (Task 7/8 depend on Task 6).

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `ToolExecution` lacks a `display` field; exit tools need `plan_review` display. | Part 1 extends `ToolExecution` with `display: Option<JsonValue>` and updates `ToolBridge` / every `ToolExecution` construction site. |
| `SessionModeManager::handoff_to` does not carry `selected_label`. | Part 1 extends the payload structs and the method signature; all callers updated in the same task. |
| `Agent` currently has no `user_language`, `office_hours_state_store`, or `game_design_state_store`. | Part 5 adds the fields and persists them; tools use trait objects so Parts 2–4 can test with mocks. |
| `Agent` builds `SessionModeManager` with an empty behavior registry. | Part 1 switches to `create_default_mode_behavior_registry()` so plan/design/office-hours/game-design modes work. |
| E2E plan enrichment depends on `@odysseythink/e2e-testing`, which has no Rust port. | Deferred gap: `ExitPlanModeTool` in Rust skips E2E enrichment; recorded in `parity/known-gaps.md` with a Phase 4.5.3 follow-up. |
| MCP/gbrain sync for artifact tools depends on an unported MCP host. | `McpProvider` trait returns “unavailable” by default; CLI fallback is implemented using `kaos_rs::exec`. |
| i18n string tables must stay aligned with TS. | Part 5 ports the exact office-hours/game-design string keys from `packages/agent-core/src/i18n/translations.ts`. |
| L3 parity requires deterministic session-mode event ordering. | Use the existing `session_mode_l3` binary harness and mock provider scenarios. |

## Spec-Coverage Table

| Roadmap § | Requirement | Part:Task(s) | Status |
|---|---|---|---|
| 4.4.5.1 | Planning enter/exit tools (plan + design) | Part 2 Task 1–4 | covered |
| 4.4.5.2 | Office-hours tool set (enter/exit/language/append-learning/append-profile/search/ensure-routing/sync-artifact) | Part 3 Task 1–8 | covered |
| 4.4.5.3 | Game-design tool set (enter/exit/language/append-learning/append-profile/search/ensure-routing/sync-artifact) | Part 4 Task 1–8 | covered |
| 4.4.5.4 | L3 fixture: enter/exit events, partition switching, artifact sync | Part 5 Task 8 | covered |
| — | Shared `SessionModeProvider` trait + `ToolExecution.display` | Part 1 Task 1 | covered |
| — | Handoff `selected_label` propagation | Part 1 Task 2 | covered |
| — | Default mode behavior registry in `Agent` build | Part 1 Task 3 | covered |
| — | `OfficeHoursStateStore` / `GameDesignStateStore` persistence | Part 5 Task 1 | covered |
| — | i18n string tables for session-mode tools | Part 5 Task 2 | covered |
| — | `Agent.session_mode` async + `SessionModeProvider` trait usage | Part 5 Task 3 | covered |
| — | `AgentSessionModeProvider` + `KaosSessionModeContext` | Part 5 Task 4 | covered |
| — | `Agent` field/Builder wiring + default state store | Part 5 Task 5 | covered |
| — | ToolManager registration of session-mode tools | Part 5 Task 6 | covered |
| — | L1 golden fixture for session-mode tools | Part 5 Task 7 | covered |
| — | L3 parity driver calling Rust binary | Part 5 Task 8 | covered |
| — | E2E enrichment in `ExitPlanModeTool` | no-op (deferred gap) |
| — | Full MCP-based gbrain sync | no-op (deferred gap; CLI fallback only) |

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-4-5/infra.md` | Shared `SessionModeProvider` traits, `ToolExecution.display`, handoff label, behavior registry | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-4-5/planning-tools.md` | Enter/Exit Plan/Design tools | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-4-5/office-hours-tools.md` | 8 office-hours tools | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-4-5/game-design-tools.md` | 8 game-design tools | done |
| 5 | `2026-06-29-backend-architecture-evolution-phase4-4-5/integration.md` | State stores, i18n, agent wiring, ToolManager registration, L1/L3 parity | done |

## Global Self-Review

- [x] 1. Spec-coverage table: 4.4.5.1–4.4.5.4 及所有支撑项均映射到 Part/Task；E2E enrichment 与 full MCP sync 作为 deferred gap 显式标注 no-op。
- [x] 2. Placeholder scan: 无 TODO/TBD；所有依赖均指向已完成 Part 或本 Part 前置 Task。
- [x] 3. No phantom tasks: 每个 Part 的 Task 都产生文件/测试/commit；index 本身不承载 Task。
- [x] 4. Dependency soundness: Part 1 → Part 2/3/4 并行 → Part 5；Part 5 内部 Task 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8。
- [x] 5. Caller & build soundness: 共享签名变更集中在 Part 1（`ToolExecution.display`、`handoff_to` payload、`SessionModeManager` registry）与 Part 5（`Agent.session_mode` 异步化、`Agent` 字段、`TurnTools.loop_tools`）；每个任务都声明全工作区类型检查。
- [x] 6. Test-the-risk: 每个 Part 都包含行为测试，覆盖 mode enter/exit、handoff、state store append/search、i18n 替换、AGENTS.md 写入、artifact sync fallback、ToolManager 注册。
- [x] 7. Type consistency: Part 5 使用的 `SessionModeProvider`、`OfficeHoursStateStore`、`GameDesignStateStore`、`McpProvider` 签名与 Part 1 定义一致；`display` 字段贯穿 `ToolExecution` → `ToolBridge` → `RunnableToolExecution`。
