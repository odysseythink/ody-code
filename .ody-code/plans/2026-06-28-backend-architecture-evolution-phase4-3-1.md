# 4.3.1 Context & projection Implementation Plan

**Goal:** 在 `agent-rs` 中完整迁移 `ContextMemory` 与投影层，使 Rust 的 context 状态机、投影输出、token 估算、通知 XML 与 TS `packages/agent-core/src/agent/context` 逐值一致，为 4.3.5 TurnFlow 提供对话历史基础设施。

**Architecture:** 在 `agent-rs/src/context/` 下新增模块：`types`（复用 records 的 `PromptOrigin`/`ContextMessage`，补齐常量与 host trait）、`projector`（纯函数投影与孤儿 tool-result 治愈）、`tokens`（字符启发式 token 估算）、`notification_xml`（后台/cron 通知 XML 渲染）、`memory`（可变 `ContextMemory`）。`ContextMemory` 不直接持有 `Agent`，而是依赖一组小型 trait（`RecordLog`、`MicroCompaction`、`InjectionLifecycle` 等）构成的 `ContextAgent` host，便于当前用 test double 对照，也便于 4.3.9 接入真实 Agent。L1 对照覆盖 projector/tokens/notification；L3 对照覆盖完整 loop event 序列产生的 context 事件流与最终投影结果。

**Tech Stack:** Rust 2021, `serde`/`serde_json`, `thiserror`; TS 侧 vitest + 轻量 parity harness; 复用 `kosong-rs::message::{Message,ContentPart}` 与 `agent-rs::records::*`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/context/mod.rs` | context 模块入口，导出子模块与公开类型 |
| `rust-ody/crates/agent-rs/src/context/types.rs` | `USER_PROMPT_ORIGIN`、`AgentContextData`、host trait（`RecordLog`/`MicroCompaction`/`InjectionLifecycle`/`BackgroundNotifications`/`ReplayBuilder`/`StatusEmitter`/`ContextSwitchFlusher`/`Clock`/`ContextAgent`） |
| `rust-ody/crates/agent-rs/src/context/projector.rs` | `project()`、`drop_orphan_tool_results()` 及相邻 user message 合并 |
| `rust-ody/crates/agent-rs/src/context/tokens.rs` | `estimate_tokens` 族纯函数 |
| `rust-ody/crates/agent-rs/src/context/notification_xml.rs` | `render_notification_xml()` 与 XML attribute 转义 |
| `rust-ody/crates/agent-rs/src/context/memory.rs` | `ContextMemory` 可变状态机 |
| `rust-ody/crates/agent-rs/tests/context_projector.rs` | projector L1 单元测试 |
| `rust-ody/crates/agent-rs/tests/context_tokens.rs` | token 估算 L1 单元测试 |
| `rust-ody/crates/agent-rs/tests/context_notification_xml.rs` | notification XML L1 单元测试 |
| `rust-ody/crates/agent-rs/tests/common/mod.rs` | ContextMemory L1 test double 公共模块 |
| `rust-ody/crates/agent-rs/tests/context_memory_basic.rs` | ContextMemory 构造/追加/清空/投影 L1 测试 |
| `rust-ody/crates/agent-rs/tests/context_memory_undo_compaction.rs` | ContextMemory undo/compaction L1 测试 |
| `rust-ody/crates/agent-rs/tests/context_memory_loop_event.rs` | ContextMemory loop 事件/工具结果 L1 测试 |
| `rust-ody/crates/agent-rs/src/bin/context_golden.rs` | L1/L3 golden 二进制：读取 fixture 输出 JSON |
| `packages/integration-tests/src/parity/context-golden.ts` | TS 侧 runner，对同一份 fixture 执行等价计算 |
| `packages/integration-tests/src/parity/fixtures/context/l1-project.json` | L1 projector + orphan-heal fixture |
| `packages/integration-tests/src/parity/fixtures/context/l1-tokens.json` | L1 token 估算 fixture |
| `packages/integration-tests/src/parity/fixtures/context/l1-notification.json` | L1 notification XML fixture |
| `packages/integration-tests/src/parity/fixtures/context/l3-memory.json` | L3 ContextMemory 操作序列 fixture |
| `packages/integration-tests/test/parity/context/l1-golden.test.ts` | L1 TS↔Rust parity 测试 |
| `packages/integration-tests/test/parity/context/l3-golden.test.ts` | L3 TS↔Rust parity 测试 |
| `packages/agent-core/test/helpers/index.ts` | 追加 ContextMemory / projector / notification / token 估算的 test-support 导出 |
| `packages/integration-tests/package.json` | 新增 `test:parity:agent:context` script |

---

## Dependency Overview

```text
[types.md Task 1: context module scaffold + types/constants]
        │
        ▼
[types.md Task 2: host traits (RecordLog / MicroCompaction / InjectionLifecycle / ...)]
        │
        ├──▶ [projector.md Task 3: project / drop_orphan_tool_results]
        │
        ├──▶ [tokens.md Task 4: estimate_tokens family]
        │
        ├──▶ [notification.md Task 5: render_notification_xml]
        │
        └──▶ [memory.md Task 6-8: ContextMemory state machine]
                  │
                  ▼
           [parity.md Task 9-11: L1/L3 fixtures + TS↔Rust parity]
```

- **可并行任务**：`projector.md`、`tokens.md`、`notification.md` 在 `types.md` 完成后彼此独立，可并行开发。
- **硬前置**：`memory.md` 依赖 `types.md` 的全部 host trait 与 `projector.md`/`tokens.md` 的实现；`parity.md` 依赖所有前置模块。
- **共享签名变更**：本计划新增 `ContextAgent` trait 与相关子 trait。`memory.md` 中 `ContextMemory` 的公开方法签名一旦确定，后续 4.3.5/4.3.7/4.3.9 才能基于其实现。若需修改，必须在本计划内一次性完成并全 workspace typecheck。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `ContextMemory` 需要调用 `microCompaction.compact()` 与 `injection` 生命周期，但 4.3.6/4.3.7 尚未实施 | 本计划先定义最小 trait 并在 test double 中 stub；真实实现留到 4.3.6/4.3.7 实现后 `impl` 这些 trait |
| `AgentRecords` 当前是泛型结构，`ContextMemory` 难以直接持有可变引用 | 本计划抽象 `RecordLog` trait，用 `Arc<dyn RecordLog>` 或 `&dyn RecordLog` 解耦；4.3.9 真实 Agent 中用一个内部可变的 wrapper 实现该 trait |
| `toolResultOutputForModel` 的错误/空输出字符串与 TS 必须逐字符一致 | 使用与 TS 完全相同的常量字符串；L1 fixture 覆盖错误、空、空白三种输出 |
| `undo` 的反向遍历索引与 `tokenCountCoveredMessageCount` 递减边界 | 写细粒度单元测试，构造 `tokenCountCoveredMessageCount` 处于历史中部的场景 |
| `project()` 与 `dropOrphanToolResults()` 的合并规则容易与 TS 漂移 | L1 直接比对 TS `project()`/`dropOrphanToolResults()` 输出；相邻 user 合并用 "\n\n" 连接文本 |

---

## Final Cross-File Review

- [ ] 1. Spec-coverage table：6 个 roadmap 条目均映射到具体 part/task，无 GAP。
- [ ] 2. Placeholder scan：6 个 part 文件中无 TODO/TBD；所有实现代码、fixture、test 均为完整可运行代码。
- [ ] 3. No phantom tasks：每个 task 产出文件变更与可验证测试；无 `--allow-empty` 或 "already done in Task N"。
- [ ] 4. Dependency soundness：跨 part 依赖 `types.md → projector/tokens/notification → memory → parity` 均为单向；每个 `Depends on:` 指向更早 task。
- [ ] 5. Caller & build soundness：共享签名变更（`RecordLog::restoring_time`、test-helpers 导出、新增 binary）均在同一 task 内更新调用方并跑全 workspace typecheck。
- [ ] 6. Test-the-risk：每个状态突变 task 都有行为断言；L1/L3 parity 用同一 fixture 双向验证 TS/Rust 输出。
- [ ] 7. Type一致性：跨文件使用的 `ContextMessage`、`PromptOrigin`、`AgentRecord`、`CompactionResult`、`LoopRecordedEvent` 均来自同一来源；方法签名与字段名前后一致。

---

## Spec-Coverage Table

| Roadmap 4.3.1 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.3.1.1 迁移 `ContextMessage` / `PromptOrigin` / `AgentContextData` 类型 | `types.md` Task 1 | covered |
| 4.3.1.2 实现 `ContextMemory`（append/clear/undo/applyCompaction、开放 step 跟踪、deferred messages） | `memory.md` Task 6-8 | covered |
| 4.3.1.3 实现 `project()` 与 `dropOrphanToolResults()` | `projector.md` Task 3 | covered |
| 4.3.1.4 实现 token 计数 | `tokens.md` Task 4 | covered |
| 4.3.1.5 实现 `notification-xml` | `notification.md` Task 5 | covered |
| 4.3.1.6 L1 + L3 fixture | `parity.md` Task 9-11 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-28-backend-architecture-evolution-phase4-3-1/types.md` | 类型、常量、host trait | done |
| 2 | `2026-06-28-backend-architecture-evolution-phase4-3-1/projector.md` | `project()` / `drop_orphan_tool_results()` | done |
| 3 | `2026-06-28-backend-architecture-evolution-phase4-3-1/tokens.md` | token 估算 | done |
| 4 | `2026-06-28-backend-architecture-evolution-phase4-3-1/notification.md` | notification XML | done |
| 5 | `2026-06-28-backend-architecture-evolution-phase4-3-1/memory.md` | `ContextMemory` | done |
| 6 | `2026-06-28-backend-architecture-evolution-phase4-3-1/parity.md` | L1/L3 fixtures + golden binary + TS runner | done |
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/agent-core/scripts (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/scenarios (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

