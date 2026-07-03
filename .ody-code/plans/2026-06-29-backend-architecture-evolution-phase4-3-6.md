# 4.3.6 Compaction strategies Implementation Plan

**Goal:** 在 `agent-rs` 中完整迁移 TS `packages/agent-core/src/agent/compaction` 的 `CompactionStrategy` / `DefaultCompactionStrategy`、`FullCompaction`、`MicroCompaction`、`SplitPlanCheckpoint`、`NormalModeTaskCheckpoint` 与 `renderMessagesToText`，使 Rust 的压缩策略、分词阈值、事件流、records 与 context 变更与 TS 逐值一致，并通过 L1（固定 summary）与 L3（turn 事件流）对照门 G4-3-6。

**Architecture:** 新增 `agent-rs/src/compaction/` 模块。`CompactionStrategy` / `DefaultCompactionStrategy` 负责计算压缩点与阈值判断；`FullCompaction` 持有 turn 内状态并在 `before_step` / `after_step` / `handle_overflow_error` 生命周期中被调用，通过 `TurnAgent` 访问 context/records/config/tools/usage/telemetry/injection/hooks/llm；`MicroCompaction` 在 `detect()` 中按缓存年龄与上下文使用率截断旧 tool result；`SplitPlanCheckpoint` 与 `NormalModeTaskCheckpoint` 在 part/todo 边界触发阻塞压缩。L1 用固定 LLM summary fixture 验证压缩后 history/records；L3 扩展 `turn_l3` golden binary与 TS runner，比对 `compaction.*` 事件序列。

**Tech Stack:** Rust 2021, `async-trait`, `tokio`, `serde`/`serde_json`, `thiserror`；TS vitest + 现有 `packages/integration-tests/src/parity` harness；复用 `agent_loop::{types, events, llm}`, `kosong-rs::{generate, ChatProvider, Message, ContentPart, ToolCall, TokenUsage, FinishReason}`, `context::{tokens, projector}`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/lib.rs` | 新增 `pub mod compaction;` |
| `rust-ody/crates/agent-rs/src/compaction/mod.rs` | compaction 模块入口，re-export 公开类型 |
| `rust-ody/crates/agent-rs/src/compaction/types.rs` | `CompactionResult`、`CompactionBeginData`、`CompactionSource`、`CompactedHistory`、`CompactGenerateResult` |
| `rust-ody/crates/agent-rs/src/compaction/strategy.rs` | `CompactionStrategy` trait、`DefaultCompactionStrategy`、`can_split_after` |
| `rust-ody/crates/agent-rs/src/compaction/render_messages.rs` | `render_messages_to_text` |
| `rust-ody/crates/agent-rs/src/compaction/budget.rs` | `CompletionBudgetConfig`、`resolve_completion_budget`、`compute_completion_budget_cap`、`apply_completion_budget` |
| `rust-ody/crates/agent-rs/src/compaction/full.rs` | `FullCompaction`：begin/cancel/block/worker/retry/telemetry |
| `rust-ody/crates/agent-rs/src/compaction/micro.rs` | `MicroCompaction`：detect/apply/compact |
| `rust-ody/crates/agent-rs/src/compaction/split_checkpoint.rs` | `SplitPlanCheckpoint`：part 边界检测 |
| `rust-ody/crates/agent-rs/src/compaction/normal_task_checkpoint.rs` | `NormalModeTaskCheckpoint`：todo 边界检测 + E2E/test-review reminder |
| `rust-ody/crates/agent-rs/src/turn/types.rs` | 扩展 `TurnContext` / `TurnTools` / `TurnSessionMode` / `TurnLlmResolver` / `TurnFullCompaction` / `TurnMicroCompaction` / `TurnSplitPlanCheckpoint` / `TurnNormalTaskCheckpoint` |
| `rust-ody/crates/agent-rs/src/turn/turn_flow.rs` | 更新 `before_step` / `after_step` 调用签名 |
| `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs` | 实现扩展后的 trait；注入 compaction mock provider |
| `rust-ody/crates/agent-rs/src/context/tokens.rs` | 新增 `estimate_tokens_for_tools` |
| `rust-ody/crates/agent-rs/tests/compaction_strategy.rs` | strategy L1 单元测试 |
| `rust-ody/crates/agent-rs/tests/full_compaction.rs` | FullCompaction L1 单元测试 |
| `rust-ody/crates/agent-rs/tests/micro_compaction.rs` | MicroCompaction L1 单元测试 |
| `rust-ody/crates/agent-rs/tests/checkpoints.rs` | SplitPlanCheckpoint / NormalModeTaskCheckpoint L1 单元测试 |
| `rust-ody/crates/agent-rs/src/bin/compaction_l1.rs` | L1 golden binary：读取 fixture 输出 snapshot JSON |
| `packages/integration-tests/src/parity/fixtures/compaction/` | L1 fixtures：auto-trigger、manual、overflow-retry、micro、split-plan、normal-task |
| `packages/integration-tests/src/parity/compaction-fixture.ts` | fixture JSON schema 与类型守卫 |
| `packages/integration-tests/src/parity/compaction-l1-driver.ts` | TS 侧 `FullCompaction` golden driver |
| `packages/integration-tests/src/parity/normalize-compaction.ts` | Compaction snapshot 归一化 |
| `packages/integration-tests/test/parity/compaction-l1-parity.test.ts` | L1 TS↔Rust 对照测试 |
| `packages/integration-tests/test/parity/compaction-l3-parity.test.ts` | L3 TS↔Rust 对照测试（扩展 turn fixture） |
| `.github/workflows/ci.yml` | 新增 `Run Compaction L1/L3 TS↔Rust parity tests` step |

---

## Dependency Overview

```text
[context: ContextMemory history/token_count/apply_compaction]  (4.3.1)
[records: AgentRecord full_compaction.* / micro_compaction.* / context.apply_compaction] (4.3.0)
[config: ConfigState model/modelCapabilities/provider/system_prompt/loop_control] (4.3.2)
[tools: ToolManager loop_tools/store_data] (4.3.2)
[usage: UsageRecorder record] (4.3.2)
[kosong-rs: generate, ChatProvider, Message, Tool, TokenUsage, FinishReason] (4.2.7)
        │
        ▼
[shared.md Task 1-2: 扩展 TurnAgent 子 trait + budget helpers + strategy + render-messages]
        │
        ├──▶ [full.md Task 3-4: FullCompaction]
        │
        ├──▶ [micro-checkpoints.md Task 5: MicroCompaction + SplitPlanCheckpoint + NormalModeTaskCheckpoint]
        │
        └──▶ [parity.md Task 6-8: fixtures + golden binary + TS runner + L1/L3 对照]
```

- **可并行任务**：`strategy` / `render-messages` / `budget` 在共享签名确定后彼此独立；`FullCompaction` 与 `MicroCompaction + Checkpoints` 在共享签名确定后可并行开发。
- **硬前置**：所有 compaction 子模块依赖 `shared.md` 的 trait 扩展与 budget helper；`parity.md` 依赖所有前置模块实现。
- **共享签名变更**：本计划集中在一处修改 `TurnContext` / `TurnTools` / `TurnSessionMode` / `TurnLlmResolver` / `TurnFullCompaction` / `TurnMicroCompaction` / `TurnSplitPlanCheckpoint` / `TurnNormalTaskCheckpoint`，并在同一任务内更新 `fixture_agent.rs` 与 `turn_flow.rs` 调用方，最后跑全 workspace typecheck。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `TurnAgent` 子 trait 扩展面大，调用方扇出多 | 集中在一个任务内完成所有签名变更与调用方更新；仅增加 compaction 必需的最小方法 |
| `FullCompaction` worker 需要跨 await 持有 `Arc<dyn TurnAgent>` | `TurnFullCompaction` trait 方法接收 `Arc<dyn TurnAgent>`，避免自引用与生命周期问题 |
| `generate_one_off` 需要复刻 TS `Agent.generate` 的 auth/request-log 路径 | 本阶段先实现无 auth 的一-off generate；auth 与 request-log 由 4.3.9 Agent 组装时补齐，本计划在 `known-gaps.md` 登记 |
| `compaction-instruction.md` 模板内容需要与 TS 逐字符一致 | 直接复用 TS 模板文本；L1 fixture 中固定 summary 不依赖 LLM 输出，避免模板差异导致的事件漂移 |
| 分词器不一致导致 `computeCompactCount` 结果不同 | L1 测试使用固定 summary fixture，不调用真实 LLM；token 估算沿用现有 `context::tokens` 字符启发式，与 TS `estimateTokens` 对齐 |
| `SplitPlanCheckpoint` 需要 session-mode 文件路径与内容；`NormalModeTaskCheckpoint` 需要 todo store | `TurnSessionMode` 增加 `file_path()` / `data()`；`TurnTools` 增加 `store_data()`，最小化接口面 |
| `NormalModeTaskCheckpoint` 依赖 `@odysseythink/e2e-testing` 的 `detectChangedFiles` | Rust 侧用 trait 注入 `ChangedFilesDetector`，fixture 提供 stub；真实实现由 4.3.9/4.4 接入 |

---

## Spec-Coverage Table

| Roadmap 4.3.6 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.3.6.1 实现 `CompactionStrategy` / `DefaultCompactionStrategy`（threshold、compact count、overflow 回退） | `shared.md` Task 3 | covered |
| 4.3.6.2 实现 `FullCompaction`（begin/cancel/block、worker、retry、summary 提取、todo list 后缀） | `full.md` Task 3-4 | covered |
| 4.3.6.3 实现 `MicroCompaction` | `micro-checkpoints.md` Task 2 | covered |
| 4.3.6.4 实现 `SplitPlanCheckpoint` + `NormalModeTaskCheckpoint` | `micro-checkpoints.md` Task 3-4 | covered |
| 4.3.6.5 实现 `renderMessagesToText` | `shared.md` Task 4 | covered |
| 4.3.6.6 L1 + L3 fixture（超长 history → compact 后 records；compaction 事件流） | `parity.md` Task 1-3 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-3-6/shared.md` | 扩展 TurnAgent 子 trait + budget helper + CompactionStrategy + renderMessagesToText | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-3-6/full.md` | FullCompaction 实现 | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-3-6/micro-checkpoints.md` | MicroCompaction + SplitPlanCheckpoint + NormalModeTaskCheckpoint | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-3-6/parity.md` | L1/L3 fixtures、golden binary、TS runner、对照测试 | done |
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

