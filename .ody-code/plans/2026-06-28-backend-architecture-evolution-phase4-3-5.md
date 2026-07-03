# 4.3.5 Turn flow & LLM adapter Implementation Plan

**Goal:** 在 `agent-rs` 中完整迁移 TS `packages/agent-core/src/agent/turn` 的 `TurnFlow`、`KosongLLM`、`RemoteKosongLLM`、`ToolCallDeduplicator`、`canonical-args`，使 Rust 的 turn 状态机、LLM 适配、工具去重、telemetry 归一化与 TS 逐值一致，并通过 L3 对照门 G4-3-5。

**Architecture:** 新增 `agent-rs/src/turn/` 模块，内部以 `TurnAgent` trait 抽象 Agent 各子系统（context、usage、config、tools、permission、injection、compaction、session-mode、goal、records、telemetry、hooks）。`TurnFlow` 持有 `&dyn TurnAgent` 并复刻 `prompt`/`steer`/`cancel`/`wait`、单 turn 工作器、多 step 循环、`driveGoal` 自动 continuation 等生命周期。`kosong_llm.rs` 把 `kosong-rs::generate` 包装成 `agent_loop::llm::Llm` trait；`tool_dedup.rs` 实现同 step / cross step 工具调用去重；`canonical_args.rs` 提供 telemetry 参数规范化。L3 对照通过 fixture 驱动 TS `TurnFlow` 与 Rust golden binary，比对归一化后的 AgentEvent 序列。

**Tech Stack:** Rust 2021, `async-trait`, `tokio`, `serde`/`serde_json`, `sha2`, `thiserror`; TS vitest + 现有 `packages/integration-tests/src/parity` harness；复用 `agent_loop::{types, events, llm}`, `context::{ContextMemory, PromptOrigin, ContextAgent}`, `kosong-rs::{generate, ChatProvider, Message, ContentPart, ToolCall, TokenUsage, FinishReason}`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/lib.rs` | 新增 `pub mod turn;` |
| `rust-ody/crates/agent-rs/src/turn/mod.rs` | turn 模块入口，re-export 公开类型 |
| `rust-ody/crates/agent-rs/src/turn/types.rs` | `TurnEndResult`、`TurnAgent` trait、子系统访问 trait（`TurnContext`、`TurnUsage`、`TurnConfig`、`TurnTools`、`TurnPermission`、`TurnInjection`、`TurnCompaction`、`TurnSessionMode`、`TurnGoal`、`TurnHooks`、`TurnTelemetry`、`TurnRecords`、`TurnLlmResolver`） |
| `rust-ody/crates/agent-rs/src/turn/error.rs` | `summarize_turn_error`、`classify_api_error`、`goal_failure_pause_reason` 等错误分类 helper |
| `rust-ody/crates/agent-rs/src/turn/telemetry.rs` | `canonical_telemetry_args`、`telemetry_tool_outcome`、`telemetry_tool_error_type` |
| `rust-ody/crates/agent-rs/src/turn/turn_flow.rs` | `TurnFlow`：prompt/steer/cancel/wait、turnWorker、runOneTurn、runStepLoop、driveGoal |
| `rust-ody/crates/agent-rs/src/turn/kosong_llm.rs` | `KosongLLM`：把 `kosong-rs::generate` 包装为 `agent_loop::llm::Llm` |
| `rust-ody/crates/agent-rs/src/turn/remote_kosong_llm.rs` | `RemoteKosongLLM` + `RemoteLlmStreamRegistry`：worker 模式 LLM 代理（可选，若 worker 模式未启用则先放 stub） |
| `rust-ody/crates/agent-rs/src/turn/tool_dedup.rs` | `ToolCallDeduplicator`：same_step / cross_step 检测、结果复用、提醒文案 |
| `rust-ody/crates/agent-rs/src/turn/canonical_args.rs` | JSON 键排序规范化（telemetry / dedup key） |
| `rust-ody/crates/agent-rs/src/bin/turn_l3.rs` | L3 golden binary：读取 fixture 输出事件 JSONL |
| `rust-ody/crates/agent-rs/tests/turn_flow.rs` | Rust 侧 TurnFlow 单元测试（test double） |
| `rust-ody/crates/agent-rs/tests/turn_kosong_llm.rs` | KosongLLM 适配器单元测试 |
| `rust-ody/crates/agent-rs/tests/turn_tool_dedup.rs` | 工具去重单元测试 |
| `packages/integration-tests/src/parity/fixtures/turn/` | L3 fixtures：end-turn、single-tool-call、tool-not-found、steer-buffer、cancel-mid-step、same-step-dedup、cross-step-dedup、goal-continuation |
| `packages/integration-tests/src/parity/turn-fixture.ts` | fixture JSON 的 TS schema 与类型守卫 |
| `packages/integration-tests/src/parity/turn-l3-driver.ts` | TS 侧 `TurnFlow` golden driver |
| `packages/integration-tests/src/parity/normalize-turn.ts` | TurnFlow snapshot 归一化 |
| `packages/integration-tests/test/parity/turn-l3.test.ts` | TS  runner 自测 |
| `packages/integration-tests/test/parity/turn-l3-parity.test.ts` | L3 TS↔Rust 对照测试 |
| `packages/integration-tests/package.json` | 新增 `test:parity` script |
| `.github/workflows/ci.yml` | 新增 `Run TurnFlow L3 TS↔Rust parity tests` step |

---

## Dependency Overview

```text
[agent_loop: RunTurnInput, LoopHooks, Llm trait, LoopEventDispatcher]   (4.3.4)
[context: ContextMemory, ContextAgent, PromptOrigin, USER_PROMPT_ORIGIN] (4.3.1)
[config: ConfigState data surface]                                       (4.3.2)
[tool: ToolManager loop_tools surface]                                   (4.3.2)
[permission: PermissionManager before_tool_call surface]                 (4.3.3)
[usage: UsageRecorder begin_turn/end_turn/record]                        (4.3.2)
[records: AgentRecord turn.* / usage / context variants]                 (4.3.0)
[kosong-rs: generate, ChatProvider, Message, ToolCall, TokenUsage]       (4.2.7)
        │
        ▼
[types.md Task 1: TurnAgent trait + 子系统访问 trait]
        │
        ▼
[turn.md Task 2: TurnFlow 状态机核心]
        │
        ├──▶ [adapter.md Task 3: KosongLLM / RemoteKosongLLM]
        │
        ├──▶ [adapter.md Task 4: ToolCallDeduplicator]
        │
        ├──▶ [adapter.md Task 5: canonical_args + telemetry helpers]
        │
        └──▶ [adapter.md Task 6: error classification + event mapping]
                  │
                  ▼
           [parity.md Task 2-6: fixtures + golden binary + TS runner + L3 对照]
```

- **可并行任务**：`KosongLLM`、`ToolCallDeduplicator`、`canonical_args`/`telemetry`、`error classification` 都在 `TurnAgent` trait 确定后彼此独立，可并行开发。
- **硬前置**：`turn.md` 依赖 `types.md`；`adapter.md` 依赖 `types.md`；`parity.md` 依赖所有前置模块。
- **共享签名变更**：本计划新增 `TurnAgent` trait 及其子 trait。`turn.md` 完成后，4.3.9 才能基于其实现真实 Agent；若需修改签名，必须在本计划内一次性完成并全 workspace typecheck。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `TurnAgent` trait 需要暴露 10+ 个子系统，接口面过大 | 按 TS `Agent` 的实际访问点拆分细粒度子 trait；`TurnAgent` 仅聚合 `&dyn` 引用；test double 分模块实现 |
| `TurnFlow` 使用大量异步 hook，Rust async trait 生命周期与 `&dyn TurnAgent` 组合可能产生自引用问题 | 所有 hook 调用通过 `Arc<dyn ...>` 或 `&dyn` 传递，避免 `TurnFlow` 持有自引用；signal 使用 `kosong_rs::provider::AbortSignal` 克隆 |
| `driveGoal` 依赖 `Agent.goals`，而 4.3.x 路线图中未显式拆分 `SessionGoalStore` | 本计划把 goal 访问抽象为 `TurnGoal` trait，先提供 stub 实现；L3 fixture 中 goal 状态由 fixture 直接驱动 |
| `RemoteKosongLLM` 依赖 SDK RPC `chatStreamInit` / `chatStreamCancel`，worker 模式可能未启用 | 先实现 In-Proc / 同进程版本；worker 模式若未就绪，则在 parity 中 skip 并登记 `known-gaps.md` |
| 工具去重阈值文案（3/5/8）和 `canonicalTelemetryArgs` 必须与 TS 逐字符一致 | 直接复制 TS 常量与 JSON 排序逻辑；L1 单元测试覆盖阈值边界 |
| `mapLoopEvent` 事件字段名 / camelCase 与 TS `AgentEvent` 不一致会导致 L3 红 | 事件类型在 `types.md` 定义时即与 TS 核对字段名；fixture 中直接 JSON 归一化比对 |

---

## Spec-Coverage Table

| Roadmap 4.3.5 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.3.5.1 实现 `TurnFlow`（prompt/steer/cancel/wait、turnWorker/runOneTurn/driveGoal、activeTurn 生命周期、steer buffer） | `turn.md` Task 2 | covered |
| 4.3.5.2 实现 `KosongLLM`（把 kosong `generate()` 包装成 loop `LLM` trait） | `adapter.md` Task 3 | covered |
| 4.3.5.3 实现 `RemoteKosongLLM`（worker 模式 LLM 代理） | `adapter.md` Task 3 | covered |
| 4.3.5.4 实现 `ToolCallDeduplicator`（same_step / cross_step 检测、结果复用） | `adapter.md` Task 4 | covered |
| 4.3.5.5 实现 telemetry 归一化（`canonical-args.ts`） | `adapter.md` Task 5 | covered |
| 4.3.5.6 L3 fixture（单 turn / 多 step / goal continuation / steer / cancel / tool-call 去重） | `parity.md` Task 2-6 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-28-backend-architecture-evolution-phase4-3-5/types.md` | `TurnAgent` trait + 子系统访问 trait + 事件/错误类型 | done |
| 2 | `2026-06-28-backend-architecture-evolution-phase4-3-5/turn.md` | `TurnFlow` 状态机核心 | done |
| 3 | `2026-06-28-backend-architecture-evolution-phase4-3-5/adapter.md` | `KosongLLM` / `RemoteKosongLLM` / `ToolCallDeduplicator` / `canonical_args` / telemetry / error | done |
| 4 | `2026-06-28-backend-architecture-evolution-phase4-3-5/parity.md` | L3 fixtures、golden binary、TS runner、对照测试 | done |

---

## Final Cross-File Review

- [x] 1. Spec-coverage table：6 个 roadmap 条目均映射到具体 part/task，无 GAP。
- [x] 2. Placeholder scan：4 个 part 文件中无 TODO/TBD；所有实现代码、fixture、test 均为完整可运行代码。
- [x] 3. No phantom tasks：每个 task 产出文件变更与可验证测试；无 `--allow-empty` 或 "already done in Task N"。
- [x] 4. Dependency soundness：跨 part 依赖 `types.md → turn.md/adapter.md → parity.md` 均为单向；每个 `Depends on:` 指向更早 task。
- [x] 5. Caller & build soundness：共享签名变更（新增 `TurnAgent` trait、`lib.rs` 导出、test-helpers 导出、新增 binary）均在同一 task 内更新调用方并跑全 workspace typecheck。
- [x] 6. Test-the-risk：每个状态突变 task 都有行为断言；L3 parity 用同一 fixture 双向验证 TS/Rust 输出。
- [x] 7. Type 一致性：跨文件使用的 `ContextMessage`、`PromptOrigin`、`AgentRecord`、`LoopEvent`、`TokenUsage`、`ToolCall` 均来自同一来源；方法签名与字段名前后一致。
