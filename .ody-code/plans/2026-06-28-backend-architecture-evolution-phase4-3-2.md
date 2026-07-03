# 4.3.2 Config / usage / tool & skill registry Implementation Plan

**Goal:** 在已落地的 `agent-rs` records 层之上，实现 Agent 的「配置面」与「工具面」：`ConfigState`、`UsageRecorder`、`ToolManager`、`SkillManager`，使它们能独立编译、单元测试通过，并为 4.3.9 的 Agent 组装与 L2 RPC 对照提供无环依赖的模块。

**Architecture:** 在 `agent-rs` crate 中新增 `config`、`usage`、`tool`、`skill` 四个顶层模块。每个模块以 Rust trait 抽象其对 Agent 其余子系统的最小依赖（如 `ConfigStateContext`、`ToolManagerContext`），避免与尚未迁移的 `ContextMemory`、`TurnFlow`、`PermissionManager` 等形成循环引用。状态变更统一通过 `AgentRecords::log_record` 写 WAL；`ToolManager` 的 MCP 相关能力先保留接口桩，待 4.3.9 接入真实 `McpConnectionManager`。`SkillManager` 的 `recordActivation(input)` 调用 `agent.turn.prompt()` 的路径也保留为 trait 方法，由 4.3.5 实现。

**Tech Stack:** Rust 2021 edition, `serde` + `serde_json`, `thiserror`; 复用 `kosong-rs` 的 `ModelCapability`、`ProviderConfig`、`TokenUsage`、`Tool` 以及 `agent-rs` records 层；TS 侧用 vitest 生成 fixture 做 L2 字段对照。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/lib.rs` | 导出新增 `config` / `usage` / `tool` / `skill` 模块 |
| `rust-ody/crates/agent-rs/src/config/mod.rs` | `ConfigState` 模块入口 |
| `rust-ody/crates/agent-rs/src/config/state.rs` | `ConfigState` 结构体与 `AgentConfigContext` trait |
| `rust-ody/crates/agent-rs/src/config/thinking.rs` | `ThinkingEffort` 与 `resolve_thinking_effort` |
| `rust-ody/crates/agent-rs/src/config/types.rs` | `AgentConfigData`、`AgentConfigUpdateData` |
| `rust-ody/crates/agent-rs/src/usage/mod.rs` | `UsageRecorder` 模块入口 |
| `rust-ody/crates/agent-rs/src/usage/recorder.rs` | `UsageRecorder` 实现 |
| `rust-ody/crates/agent-rs/src/tool/mod.rs` | `ToolManager` 模块入口与类型 |
| `rust-ody/crates/agent-rs/src/tool/types.rs` | `ToolInfo`、`ToolSource`、`UserToolRegistration`、`McpToolCollision` |
| `rust-ody/crates/agent-rs/src/tool/manager.rs` | `ToolManager` 实现与 `ToolManagerContext` trait |
| `rust-ody/crates/agent-rs/src/skill/mod.rs` | `SkillManager` / `SkillRegistry` 模块入口 |
| `rust-ody/crates/agent-rs/src/skill/types.rs` | skill 类型（`SkillDefinition`、`SkillSource`、`SkillActivatedEvent` 等） |
| `rust-ody/crates/agent-rs/src/skill/manager.rs` | `SkillManager` 实现与 `SkillActivationContext` trait |
| `rust-ody/crates/agent-rs/src/skill/registry.rs` | `SkillRegistry` trait 与内存实现 |
| `rust-ody/crates/agent-rs/tests/config_state.rs` | `ConfigState` 单元测试 |
| `rust-ody/crates/agent-rs/tests/config_fixture_parity.rs` | `ConfigState` Rust fixture round-trip |
| `rust-ody/crates/agent-rs/tests/usage_recorder.rs` | `UsageRecorder` 单元测试 |
| `rust-ody/crates/agent-rs/tests/usage_fixture_parity.rs` | `UsageRecorder` Rust fixture round-trip |
| `rust-ody/crates/agent-rs/tests/tool_manager.rs` | `ToolManager` 单元测试 |
| `rust-ody/crates/agent-rs/tests/tool_fixture_parity.rs` | `ToolManager` Rust fixture round-trip |
| `rust-ody/crates/agent-rs/tests/skill_manager.rs` | `SkillManager` 单元测试 |
| `rust-ody/crates/agent-rs/tests/skill_registry.rs` | `SkillRegistry` 单元测试 |
| `rust-ody/crates/agent-rs/tests/config_ts_fixture_parity.rs` | Rust 读取 TS config fixture |
| `rust-ody/crates/agent-rs/tests/usage_ts_fixture_parity.rs` | Rust 读取 TS usage fixture |
| `rust-ody/crates/agent-rs/tests/tool_ts_fixture_parity.rs` | Rust 读取 TS tools fixture |
| `scripts/generate-config-usage-tool-fixtures.ts` | TS 侧生成 L2 fixture 的脚本 |
| `packages/agent-core/src/agent/config/config.parity.test.ts` | TS 读取 Rust config fixture |
| `packages/agent-core/src/agent/tool/tool.parity.test.ts` | TS 读取 Rust tools fixture |
| `packages/agent-core/src/agent/usage/usage.parity.test.ts` | TS 读取 Rust usage fixture |

---

## Dependency Overview

```text
[schema.md / records.md from 4.3.0: AgentRecords, AgentRecord, nested types]
        │
        ├──▶ [config.md] ConfigState + thinking resolution
        │         │
        │         ▼
        │    [tool.md Task 1-2] tool types + ToolManagerContext trait
        │         │
        │         ▼
        │    [tool.md Task 3-5] ToolManager implementation
        │
        ├──▶ [usage.md] UsageRecorder
        │
        ├──▶ [skill.md] SkillManager + SkillRegistry trait
        │
        ▼
[parity.md] L2 fixtures + TS↔Rust field parity tests
```

- **可并行任务**：`config.md`、`usage.md`、`skill.md` 彼此独立，仅共享 4.3.0 的 records 层；`tool.md` 依赖 `config.md` 的 `AgentConfigContext` 与 `AgentConfigData` 形状（用于 `initialize_builtin_tools` 时读取 cwd / provider / capabilities）。
- **共享签名变更**：
  - `rust-ody/crates/agent-rs/src/lib.rs` 新增模块导出（index Task 1）。
  - `kosong-rs` 需要暴露 `ModelCapability`、`ProviderConfig`、`TokenUsage`、`Tool`（4.2.0 已完成；若缺则在本计划 `config.md` Task 1 中以 `pub use` 补齐）。
  - 4.3.9 之前不修改 `ody-host` 的 RPC 路由；L2 对照在 4.3.2 中通过模块级 fixture 完成，不触碰 CoreAPI 共享签名。
- **硬前置**：4.3.0 records 层必须已实现（crate 已存在）。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `ToolManager` 依赖大量尚未迁移的 Agent 子系统（background、cron、skills、goals、subagentHost、session-mode tools 等）| 用 trait 抽象所需的最小接口；MCP 与 builtin tools 先保留接口桩；`initialize_builtin_tools` 在 4.3.2 中只实现无依赖的核心 builtin（Read/Write/Edit/Glob/Grep/Bash），其余返回空 |
| `ConfigState.update` 调用 `agent.kaos.chdir` 与 `agent.tools.initializeBuiltinTools()`，形成模块间循环 | `AgentConfigContext` trait 只暴露 `records`、`model_provider`、`kimi_config_thinking`、`emit_status_updated`、`tools_init_callback`、`kaos_cwd` 等方法；实现由 4.3.9 提供 |
| `SkillManager.recordActivation` 调用 `agent.turn.prompt()`，而 TurnFlow 在 4.3.5 | `SkillActivationContext` trait 提供 `prompt` 方法；4.3.2 的测试使用 mock 实现验证 record / event 行为 |
| `ToolManager.loopTools` 的排序与过滤逻辑必须和 TS 逐字段一致 | 单元测试覆盖启用/禁用/注册/注销组合后的排序与 goal 工具隐藏 |
| MCP 工具冲突、auth tool、server 状态变更事件较复杂 | 4.3.2 保留接口与事件形状，逻辑用 mock MCP manager 做 L1 对照；真实网络留在 4.3.9 / 4.4 |

**已做 design-lite 决策：**
- 采用 **接口隔离（trait-based）方案**：每个子模块只声明自己需要的 Agent 能力 trait，而不是等待 4.3.9 的完整 `Agent` struct。Pros：4.3.2 可独立编译测试；避免 Rust 循环依赖；4.3.9 只需实现这些 trait。Cons：trait 方法名需与 TS 调用点一一对应，4.3.9 集成时需仔细对齐。
- 不替代方案：等待 4.3.9 完整 Agent 后再实现 4.3.2（会导致 4.3.2 无法独立合入、无法做 L2 对照）；或在 4.3.2 中直接引用未来模块（会导致编译失败）。

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-28-backend-architecture-evolution-phase4-3-2/config.md` | `ConfigState` + `thinking` 解析 + `AgentConfigContext` trait | done |
| 2 | `2026-06-28-backend-architecture-evolution-phase4-3-2/usage.md` | `UsageRecorder` | done |
| 3 | `2026-06-28-backend-architecture-evolution-phase4-3-2/tool.md` | tool types + `ToolManager` + `ToolManagerContext` | done |
| 4 | `2026-06-28-backend-architecture-evolution-phase4-3-2/skill.md` | `SkillRegistry` trait + `SkillManager` + `SkillActivationContext` | done |
| 5 | `2026-06-28-backend-architecture-evolution-phase4-3-2/parity.md` | L2 fixtures + TS↔Rust 字段对照测试 | done |

---

## Spec-Coverage Table

| Roadmap 4.3.2 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.3.2.1 实现 `ConfigState` + thinking 解析 | `config.md` Task 1–3 | covered |
| 4.3.2.2 实现 `UsageRecorder` | `usage.md` Task 1–2 | covered |
| 4.3.2.3 实现 `ToolManager` + tool types | `tool.md` Task 1–5 | covered |
| 4.3.2.4 实现 `SkillManager` | `skill.md` Task 1–2 | covered |
| 4.3.2.5 L2 fixture | `parity.md` Task 1–3 | covered |

---

## Global Self-Review

- [x] 1. Spec-coverage：上表覆盖 Roadmap 4.3.2 全部 5 个条目，无 GAP。
- [x] 2. Placeholder scan：part 文件中无 TODO/TBD；MCP 与 builtin tool 的未完成部分以接口桩 + `todo!()` 或空集合明确标出，并说明由 4.3.9/4.4 补齐。
- [x] 3. No phantom tasks：每个 task 都产生可验证的代码/测试/fixture 变更；无 `--allow-empty` 或 "already done" 类型任务。
- [x] 4. Dependency soundness：跨 part 依赖均从早到晚：`tool.md` 依赖 `config.md`；`parity.md` 依赖前四 part；无反向依赖。
- [x] 5. Caller & build soundness：本计划仅新增 `agent-rs` 内部模块与测试，不修改 TS 共享签名；`lib.rs` 模块导出变更以 `cargo check -p agent-rs --workspace --tests` 验证。
- [x] 6. Test-the-risk：每个状态变更任务都附带行为断言——`ConfigState.update` 写 WAL、`UsageRecorder.record` 累加按 model、`ToolManager.set_active_tools` 改变 enabled 集合、`ToolManager.loop_tools` 排序与 TS 一致、`SkillManager.activate`  emit 事件并调用 prompt trait。
- [x] 7. Type consistency：`AgentConfigData`、`AgentConfigUpdateData`、`ToolInfo`、`UserToolRegistration`、`UsageStatus` 等类型名/字段名与 TS 源一致；records 类型复用 4.3.0 定义。
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

