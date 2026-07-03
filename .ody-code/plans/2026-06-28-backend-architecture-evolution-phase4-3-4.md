# 4.3.4 Stateless Loop Engine Implementation Plan

**Goal:** 在 `agent-rs` 中实现与 TS `packages/agent-core/src/loop/` 逐值一致的无状态 turn/step 引擎，让同一份 mock-LLM fixture 在 TS 与 Rust 两侧产出相同的 `LoopEvent` 序列，并通过 L3 对照门 G4-3-4。

**Architecture:** 新增 `agent-rs/src/agent_loop/` 模块（Rust 关键字 `loop` 不可用，故模块名 `agent_loop`、目录 `agent_loop/`），复用 `kosong-rs` 的 `ChatProvider`/`generate`/`Message`/`Tool`/`TokenUsage` 类型与 `records::nested` 已有的 `LoopRecordedEvent`/`ExecutableToolResult`。内部定义 `Llm` trait、`LoopHooks`、`ExecutableTool` 与 `LoopEventDispatcher`。`run_turn` 负责跨 step 收敛（max steps、abort、retry、usage 累加、`should_continue_after_stop` hook），`execute_loop_step` 负责单 step 的 LLM 调用与事件分发，`tool_call`/`tool_scheduler` 负责 provider-order 的工具验证/准备/授权/执行/结果归一，`events` 提供可记录的 durable 事件与仅 live 事件的统一分发器。L3 对照不经过 CoreHost，而是直接用同一 fixture 分别驱动 TS `runTurn` 与一个 Rust golden binary，比对归一化后的事件 JSONL。

**Tech Stack:** Rust (tokio, serde_json, async-trait, thiserror, jsonschema, regex, tracing), TypeScript (vitest), 复用现有 `packages/integration-tests/src/parity/` 归一化与断言工具。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

新增/修改文件一览：

```
rust-ody/crates/agent-rs/
  Cargo.toml                              # 添加 jsonschema、regex、tracing 依赖
  src/lib.rs                              # 导出 agent_loop 模块
  src/agent_loop/
    mod.rs                                # 子模块聚合与公共 re-export
    types.rs                              # ExecutableTool, LoopHooks, TurnResult 等
    events.rs                             # LoopEvent, LoopEventDispatcher, live events
    llm.rs                                # Llm trait 与 KosongLlm 适配器桩
    tool_access.rs                        # ToolAccesses 冲突检测
    tool_scheduler.rs                     # 并发调度器
    retry.rs                              # chat_with_retry / 退避
    errors.rs                             # LoopMaxStepsExceeded / abort 判断
    turn_step.rs                          # execute_loop_step
    run_turn.rs                           # run_turn
    tool_call.rs                          # run_tool_call_batch / 工具生命周期
  src/bin/loop_l3.rs                      # L3 golden binary: fixture -> events JSONL
  tests/loop_scaffold.rs                  # Task 1 编译/序列化测试
  tests/loop_types.rs                     # Task 2 类型/工具 trait 测试
  tests/loop_llm.rs                       # Task 2 Llm trait 测试
  tests/loop_events.rs                    # Task 3 dispatcher 测试
  tests/tool_access.rs                    # Task 4 访问冲突测试
  tests/tool_scheduler.rs                 # Task 4 调度器并发/串行测试
  tests/run_turn.rs                       # Phase B run_turn 单元测试
  tests/tool_call_batch.rs                # Phase B tool-call 生命周期测试

packages/integration-tests/
  src/parity/fixtures/loop/
    single-text.json                      # 单 text 结束
    single-tool-call.json                 # 单 tool-call -> tool-result -> 结束
    parallel-tool-calls.json              # 并行 tool-calls
    tool-failure.json                     # 工具返回 error
    max-steps.json                        # 多 step 达到 maxSteps
    abort-mid-step.json                   # step 中 abort
    retry-recover.json                    # retry 后恢复
  src/parity/loop-l3.ts                   # TS 侧 runTurn golden driver
  test/parity/loop/l3-loop-engine.test.ts # L3 对照测试
```

---

## Dependency Overview

按「自底向上、先类型后运行时、先单元后对照」分为三个阶段。阶段内任务串行，阶段之间 phase A → phase B → phase C。

### Phase A — 类型与事件契约
1. **Task 1**: `agent-rs` 添加依赖并搭建 `agent_loop` 模块
2. **Task 2**: 迁移 loop 类型 (`types.rs`) 与 `llm.rs` Llm trait
3. **Task 3**: 迁移 loop 事件 (`events.rs`) 与统一 dispatcher
4. **Task 4**: 迁移 `tool_access.rs` 与 `tool_scheduler.rs`

### Phase B — 单 step / 多 step 引擎
5. **Task 5**: 实现 `retry.rs` 与 `errors.rs`
6. **Task 6**: 实现 `turn_step.rs` (execute_loop_step)
7. **Task 7**: 实现 `run_turn.rs` (run_turn)
8. **Task 8**: 实现 `tool_call.rs` (run_tool_call_batch / 工具生命周期)

### Phase C — L3 对照与 golden
9. **Task 9**: 创建 L3 fixtures 与 `loop_l3.rs` golden binary
10. **Task 10**: 创建 TS `loop-l3.ts` driver 与 `l3-loop-engine.test.ts`
11. **Task 11**: 注册 package script、运行 TS-vs-Rust L3 对照并修复偏差

---

## Risks & Open Questions

| 风险 | 影响 | 缓解策略 |
|---|---|---|
| `jsonschema` crate 的验证错误格式与 AJV 不一致，导致 `tool.result` payload 中 error message 偏差 | L3 红 | Task 8 中显式对齐错误消息模板（required/additionalProperties/一般错误），并用 fixture 验证 |
| Rust 的 `AbortSignal` 与 TS `AbortSignal` 语义不同（TS 是 DOM AbortSignal，Rust 是自定义原子标志） | abort 边界行为偏差 | 在 `Llm` trait 与工具上下文统一使用 `kosong-rs::provider::AbortSignal` 的克隆；测试覆盖 abort-before-step、abort-mid-stream、abort-mid-tool |
| 工具并发调度顺序导致事件顺序不稳定 | L3 红 | `tool_scheduler` 保持 provider-order 的 `tool.call`/`tool.result`，仅执行阶段并发； fixture 中的并行 tool-call 用固定顺序断言 |
| `LoopEvent` 序列化字段名与 TS 不完全一致（camelCase / snake_case） | L3 红 | 所有事件类型使用 `#[serde(rename_all = "camelCase")]` 或显式 `#[serde(rename)]`，并在 Task 3 用 JSON round-trip 测试逐个字段 |
| `LoopRecordedEvent` 已存在于 `records::nested.rs`，与新 `LoopEvent` 重复 | 维护成本 | 新 `LoopEvent` 为完整运行时事件（含 live-only variants）；`events.rs` 直接 `pub use` records 的 `LoopRecordedEvent`，避免 records 侧改动 |

---

## Spec-coverage Table

| 路线图条目 | 覆盖情况 | 任务 |
|---|---|---|
| 4.3.4.1 迁移 loop 类型与事件 | covered | Phase A Task 2, Task 3 |
| 4.3.4.2 实现 `executeLoopStep` | covered | Phase B Task 6 |
| 4.3.4.3 实现 `runTurn` | covered | Phase B Task 7 |
| 4.3.4.4 实现 tool-call 调度 | covered | Phase A Task 4 + Phase B Task 8 |
| 4.3.4.5 实现 retry 与 tool-access | covered | Phase B Task 5 + Phase A Task 4 + Phase B Task 8 |
| 4.3.4.6 L3 fixture | covered | Phase C Task 9, Task 10, Task 11 |
| G4-3-4 loop engine 全部 L3 fixture 绿 | covered | Phase C Task 11 |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-28-types/phase-a.md` | Phase A: 类型、事件、LLM trait、工具访问与调度器 | done |
| 2 | `2026-06-28-types/phase-b.md` | Phase B: retry、errors、turn_step、run_turn、tool_call | done |
| 3 | `2026-06-28-types/phase-c.md` | Phase C: L3 fixtures、golden binary、TS driver、对照测试 | done |
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/agent-core/scripts (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/helpers (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/scenarios (priority: important)
- /Users/ranwei/workspace/ody-code/scripts (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

