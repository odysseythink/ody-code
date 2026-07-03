# Phase 4.4.4 Collaboration Tools Implementation Plan

**Goal:** Migrate the three collaboration builtin tools (`Skill`, `AskUserQuestion`, `Agent`) from TypeScript to Rust in `tools-rs`, wire them into `agent-rs`, and prove equivalence via L3 parity scenarios.

**Architecture:** Add a `collaboration` submodule to `tools-rs/src/builtin/` that defines minimal trait boundaries for agent-provided capabilities (skill registry/session-mode, interactive question RPC, subagent host, background task registration). `agent-rs` depends on `tools-rs`, implements these traits, and bridges `tools_rs::builtin::BuiltinTool` to `agent_loop::types::ExecutableTool`. The three tools are instantiated with trait-object references so they remain testable with mocks, and are registered in `agent-rs::tool::manager::ToolManager`.

**Tech Stack:** Rust 2021 (`tokio`, `serde`, `chrono`, `async-trait`), `tools-rs` existing infrastructure (`BuiltinTool` trait, `InputSchema`, `ToolExecution`), `agent-rs` subsystems (`SkillManager`, `SessionModeManager`, `BackgroundManager`, `TurnFlow`), TypeScript/Vitest for parity harness.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

```
rust-ody/crates/tools-rs/
  Cargo.toml                                   # add async-trait dependency
  src/lib.rs                                    # expose pub mod builtin::collaboration
  src/builtin/mod.rs                            # extend ExecutableToolContext + add pub mod collaboration
  src/builtin/collaboration/
    mod.rs                                      # trait boundaries (SkillProvider, QuestionProvider, SubagentHost, BackgroundRegistrar)
    skill.rs                                    # SkillTool
    ask_user.rs                                 # AskUserQuestionTool
    agent.rs                                    # AgentTool
  src/bin/tools-golden.rs                       # add Op variants for collaboration tools
rust-ody/crates/agent-rs/
  Cargo.toml                                    # add tools-rs dependency
  src/agent_loop/types.rs                       # extend ExecutableToolContext with turn_id/tool_call_id
  src/tool/manager.rs                           # register Skill/AskUserQuestion/Agent tools
  src/tool/types.rs                             # maybe adjust ExecutableTool; add tool bridge adapter
  src/agent.rs                                  # hold SkillManager; wire background; implement provider traits
  src/turn/types.rs                             # expand TurnSubagentHost trait with spawn/resume
  src/turn/fixture_agent.rs                     # update ExecutableToolContext usages
  src/bin/*.rs                                  # update any direct ExecutableToolContext construction
packages/integration-tests/src/parity/
  fixtures/tools-rs/
    collaboration-tools.json                    # L1 fixture for Skill/AskUserQuestion/Agent
  tools-rs-golden.ts                            # extend GoldenOp + runCase for new ops
  known-gaps.md                                 # add 4.4.4 deferred items
packages/integration-tests/test/parity/tools-rs/l1-golden.test.ts  # register new fixture
.github/workflows/rust-host.yml                # ensure golden job covers new fixture
```

## Dependency Overview

```
Part 1: infra.md (shared context + trait boundaries + tool bridge)
  │
  ├──► Part 2: skill-tool.md (SkillTool + SkillProvider)
  │
  ├──► Part 3: ask-user-tool.md (AskUserQuestionTool + QuestionProvider + BackgroundRegistrar)
  │
  └──► Part 4: agent-tool.md (AgentTool + SubagentHost + background agent task)
              │
              ▼
        Part 5: integration.md (agent-rs wiring + ToolManager registration + L3 parity)
```

- **Phase A** (Part 1): Foundation — extend `ExecutableToolContext`, define collaboration trait boundaries, add `tools-rs` → `agent_loop::ExecutableTool` bridge. Sequential tasks within; the shared-signature changes (context fields) must be done once and propagated to every caller.
- **Phase B** (Parts 2–4): Tools — can run in parallel after Part 1 completes. Each tool is implemented in `tools-rs` with mock providers and L1 golden fixtures.
- **Phase C** (Part 5): Integration & parity — depends on all tools. Wires real `SkillManager`, question RPC, subagent host, and `BackgroundManager` into `agent-rs`, registers tools, and runs L3 parity scenarios.

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `agent-rs` currently does not depend on `tools-rs`; bridging the two `ExecutableTool` abstractions changes shared signatures. | Part 1 centralizes the bridge and the context-field expansion; one task updates every caller and ends with whole-workspace typecheck. |
| Collaboration tools need richer agent context than file/web/background tools (skill registry, session mode, RPC, subagent host). | Define minimal trait interfaces in `tools-rs` (like 4.4.3 did for `BackgroundManager`/`CronManager`); `agent-rs` implements them. Keeps `tools-rs` testable and avoids circular deps. |
| `Agent.subagent_host()` currently returns `None` and `TurnSubagentHost` only has `cancel_all`. | Part 4 redesigns the trait with `spawn`/`resume`; Part 5 provides a stub host that can be replaced by real subagent implementation later. |
| `Agent` currently has no `SkillManager` field; `AgentBuilder::skills_registry` is a stub. | Part 5 adds the field and wires it; until then tools use the trait interface with mocks. |
| `AskUserQuestion` needs `requestQuestion` reverse-RPC, which is host-level. | Part 3 defines a `QuestionProvider` trait; Part 5 implements it on top of `AgentEnvironment`. If the host cannot yet service questions, the tool returns the TS-equivalent unsupported error. |
| L3 parity requires deterministic mock providers and event ordering. | Use the same mock-LLM harness as 4.4.3/4.3.x; collaboration scenarios are deterministic scripts with fixed question answers / subagent completions. |

## Spec-Coverage Table

| Roadmap § | Requirement | Part:Task(s) | Status |
|---|---|---|---|
| 4.4.4.1 | `SkillTool`: inline skill call, recursion cap, mode hidden | Part 2 Task 1 | covered |
| 4.4.4.2 | `AskUserQuestionTool`: requestQuestion, background question task, dismissed handling | Part 3 Task 1 | covered |
| 4.4.4.3 | `AgentTool`: spawn/resume, foreground/background, timeout | Part 4 Task 1 | covered |
| 4.4.4.4 | L3 fixture: skill call → system reminder; question answer; subagent complete/fail/background | Part 5 Task 6 | covered |
| — | Extend `ExecutableToolContext` with `turn_id`/`tool_call_id` | Part 1 Task 1 | covered |
| — | `tools-rs` → `agent_loop::ExecutableTool` bridge | Part 1 Task 2 | covered |
| — | `SkillProvider` trait + `agent-rs` impl | Part 1 Task 3, Part 5 Task 2 | covered |
| — | `QuestionProvider` trait + `agent-rs` impl | Part 1 Task 3, Part 5 Task 3 | covered |
| — | `SubagentHost` trait + `agent-rs` impl | Part 1 Task 3, Part 4 Task 1, Part 5 Task 4 | covered |
| — | `BackgroundRegistrar` trait + `agent-rs` impl | Part 1 Task 3, Part 5 Task 4 | covered |
| — | Wire collaboration tools into `Agent`/`AgentBuilder`/`loop_tools` | Part 5 Task 5 | covered |
| — | L3 parity scenario for all three collaboration tools | Part 5 Task 6 | covered |

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-4-4/infra.md` | Shared context extension + collaboration trait boundaries + tool bridge | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-4-4/skill-tool.md` | SkillTool implementation + L1 fixture | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-4-4/ask-user-tool.md` | AskUserQuestionTool implementation + L1 fixture | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-4-4/agent-tool.md` | AgentTool implementation + L1 fixture | done |
| 5 | `2026-06-29-backend-architecture-evolution-phase4-4-4/integration.md` | agent-rs wiring, ToolManager registration, L3 parity | done |

## Global Self-Review

- [ ] 1. Spec-coverage table: 4.4.4.1–4.4.4.4 及所有支撑项均映射到 Part/Task，无 GAP。
- [ ] 2. Placeholder scan: 无 TODO/TBD；所有依赖均指向已完成的 Part 1–4 或本 Part 的前置 Task。
- [ ] 3. No phantom tasks: 每个 Part 的 Task 都产生文件/测试/commit；index 本身不承载 Task。
- [ ] 4. Dependency soundness: Part 1 → Part 2/3/4 并行 → Part 5；Part 5 内部 Task 1 → (2,3,4 并行) → 5 → 6。
- [ ] 5. Caller & build soundness: 共享签名变更集中在 Part 1 Task 1（`ExecutableToolContext`）、Part 2 Task 1（`ToolExecution.matches_rule`）与 Part 5 Task 5（`AgentBuilder/Agent` 字段）；每个任务都声明了全工作区类型检查。
- [ ] 6. Test-the-risk: 每个 Part 都包含行为测试，覆盖 skill 激活、question RPC、subagent completion、背景任务注册、loop_tools 返回、L3 turn 调用链。
- [ ] 7. Type consistency: Part 5 使用的 `SkillProvider`、`QuestionProvider`、`SubagentHost`、`BackgroundRegistrar` 签名与 Part 1 定义一致；`QuestionRunFn` 使用 Part 3 修正后的签名。
