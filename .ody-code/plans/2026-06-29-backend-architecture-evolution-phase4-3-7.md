# 4.3.7 Session modes & prompt injection Implementation Plan

**Goal:** 在已落地的 `agent-rs` 4.3.1 Context、4.3.2 Config/Tool、4.3.5 TurnFlow 之上，迁移 `SessionMode` 状态机（plan/design/office-hours/game-design 四种模式的 enter/exit/cancel/handoff）、`InjectionManager`（step 前注入 goal/reminder/todo/skills/knowledge）、`ReplayBuilder`（resume 校验记录），使 Rust 侧在多模式切换、模型恢复、partition 切换、注入 prompt 内容等方面与 TS 逐字段一致，并通过 L3 对照门 G4-3-7。

**Architecture:** 在 `agent-rs` crate 中新增 `session_mode`、`injection`、`replay` 三个模块。`SessionModeManager` 通过 `SessionModeContext` trait 抽象对 Agent 其余子系统的最小依赖（records、config、context、replay、permission、kaos），持有 4 个 `SessionModeKindBehavior` 实现（Plan/Design/OfficeHours/GameDesign）。`InjectionManager` 通过 `InjectionManagerContext` trait 抽象依赖，聚合 8 个 `DynamicInjector` 实现并以 TS 顺序在 step 前串行注入。`ReplayBuilder` 轻量复刻 TS 的 push/setMode/buildResult 语义。所有状态变更通过 `AgentRecords::log_record` 写 WAL。

**Tech Stack:** Rust 2021 edition, `serde` + `serde_json`, `async_trait`; 复用 `agent-rs` 的 `records::nested::{SessionModeKind, AgentConfigUpdateData}`、`records::types::AgentRecord`、`context::{ContextMemory, ContextAgent, InjectionLifecycle, ReplayBuilder as ReplayBuilderTrait}`、`config::state::{ConfigState, AgentConfigContext}`、`turn::types::{TurnSessionMode, TurnInjection}`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/lib.rs` | 新增 `pub mod session_mode;` `pub mod injection;` `pub mod replay;` |
| `rust-ody/crates/agent-rs/src/session_mode/mod.rs` | 模块入口，re-export |
| `rust-ody/crates/agent-rs/src/session_mode/types.rs` | `SessionModeKindBehavior` trait、`SessionModeContext` trait、`ModeEnterContext`、`ModeExitContext`、`ModeBehaviorRegistry` |
| `rust-ody/crates/agent-rs/src/session_mode/behaviors/mod.rs` | `BaseSessionModeBehavior`（共享 doEnter/doExit/doCancel） |
| `rust-ody/crates/agent-rs/src/session_mode/behaviors/plan.rs` | `PlanModeBehavior` |
| `rust-ody/crates/agent-rs/src/session_mode/behaviors/design.rs` | `DesignModeBehavior` |
| `rust-ody/crates/agent-rs/src/session_mode/behaviors/office_hours.rs` | `OfficeHoursModeBehavior` |
| `rust-ody/crates/agent-rs/src/session_mode/behaviors/game_design.rs` | `GameDesignModeBehavior` |
| `rust-ody/crates/agent-rs/src/session_mode/manager.rs` | `SessionModeManager`：enter/exit/cancel/clear/handoff/restoreEnter、文件路径解析、design session tracking |
| `rust-ody/crates/agent-rs/src/session_mode/directory.rs` | `get_mode_output_subdirectory` — 模式输出目录解析 |
| `rust-ody/crates/agent-rs/src/session_mode/model_auth.rs` | `resolve_mode_model_alias` — 模式模型别名查找 |
| `rust-ody/crates/agent-rs/src/session_mode/topic_generator.rs` | `TopicGenerator` + `slugify_title`/`format_date_prefix`/`extract_first_heading`/`strip_date_prefix` |
| `rust-ody/crates/agent-rs/src/replay/mod.rs` | 模块入口 |
| `rust-ody/crates/agent-rs/src/replay/types.rs` | `AgentReplayRecord` 枚举、`ReplayBuilder` struct、`ReplayBuilderContext` trait |
| `rust-ody/crates/agent-rs/src/injection/mod.rs` | 模块入口 |
| `rust-ody/crates/agent-rs/src/injection/types.rs` | `InjectionManagerContext` trait、`InjectionVariant` 常量 |
| `rust-ody/crates/agent-rs/src/injection/dynamic_injector.rs` | `DynamicInjector` 基类（trait + 默认实现） |
| `rust-ody/crates/agent-rs/src/injection/base_session_mode.rs` | `BaseSessionModeInjector`（entry/reentry/full/sparse/exit 逻辑） |
| `rust-ody/crates/agent-rs/src/injection/contracts/mod.rs` | 契约文本常量模块入口 |
| `rust-ody/crates/agent-rs/src/injection/contracts/plan.rs` | Plan mode 注入契约文本（entry/reentry/full/sparse/exit + handoff + skills unavailable） |
| `rust-ody/crates/agent-rs/src/injection/contracts/design.rs` | Design mode 注入契约文本 |
| `rust-ody/crates/agent-rs/src/injection/contracts/office_hours.rs` | Office hours 注入契约文本 |
| `rust-ody/crates/agent-rs/src/injection/contracts/game_design.rs` | Game design 注入契约文本 |
| `rust-ody/crates/agent-rs/src/injection/session_mode_injectors.rs` | 4 个 session-mode injector（Plan/Design/OfficeHours/GameDesign） |
| `rust-ody/crates/agent-rs/src/injection/goal_injector.rs` | `GoalInjector` |
| `rust-ody/crates/agent-rs/src/injection/todo_list_injector.rs` | `TodoListReminderInjector` |
| `rust-ody/crates/agent-rs/src/injection/plugin_session_start.rs` | `PluginSessionStartInjector` |
| `rust-ody/crates/agent-rs/src/injection/permission_mode_injector.rs` | `PermissionModeInjector` |
| `rust-ody/crates/agent-rs/src/injection/knowledge_microagent.rs` | `KnowledgeMicroagentInjector` |
| `rust-ody/crates/agent-rs/src/injection/parts_manifest.rs` | `PartsManifest` 解析器（parse/parse_files/count_rows） |
| `rust-ody/crates/agent-rs/src/injection/manager.rs` | `InjectionManager`：组装所有 injector、`inject()`/`inject_goal()` 生命周期 |
| `rust-ody/crates/agent-rs/src/bin/session_mode_l3.rs` | L3 golden binary：读取 fixture 输出事件 JSONL |
| `rust-ody/crates/agent-rs/tests/session_mode_manager.rs` | `SessionModeManager` 单元测试 |
| `rust-ody/crates/agent-rs/tests/session_mode_behaviors.rs` | 4 个 behavior 单元测试 |
| `rust-ody/crates/agent-rs/tests/injection_manager.rs` | `InjectionManager` 单元测试 |
| `rust-ody/crates/agent-rs/tests/injection_injectors.rs` | 所有 injector 单元测试 |
| `rust-ody/crates/agent-rs/tests/replay_builder.rs` | `ReplayBuilder` 单元测试 |
| `rust-ody/crates/agent-rs/tests/topic_generator.rs` | `TopicGenerator` 单元测试 |
| `rust-ody/crates/agent-rs/tests/parts_manifest.rs` | `PartsManifest` 解析器单元测试 |
| `packages/integration-tests/src/parity/fixtures/session-mode/` | L3 fixtures：plan-enter-exit、design-enter-exit、office-hours-enter-exit、game-design-enter-exit、handoff、partition-defer、injection-content |
| `packages/integration-tests/src/parity/session-mode-fixture.ts` | fixture JSON 的 TS schema 与类型守卫 |
| `packages/integration-tests/src/parity/session-mode-l3-driver.ts` | TS 侧 SessionMode golden driver |
| `packages/integration-tests/src/parity/normalize-session-mode.ts` | SessionMode snapshot 归一化 |
| `packages/integration-tests/test/parity/session-mode-l3.test.ts` | TS runner 自测 |
| `packages/integration-tests/test/parity/session-mode-l3-parity.test.ts` | L3 TS↔Rust 对照测试 |

---

## Dependency Overview

```text
[records: SessionModeKind, AgentRecord::SessionModeEnter/Exit/Cancel]    (4.3.0)
[context: ContextMemory, ContextAgent, InjectionLifecycle, ReplayBuilder] (4.3.1)
[config: ConfigState, AgentConfigContext, ProviderConfig]                 (4.3.2)
[turn: TurnSessionMode, TurnInjection]                                    (4.3.5)
        │
        ├──▶ [core.md: Task 1] SessionModeKindBehavior trait + SessionModeContext trait
        │         │
        │         ├──▶ [core.md: Task 2] ReplayBuilder + AgentReplayRecord
        │         │
        │         └──▶ [core.md: Task 3] topic-generator + directory + model-auth
        │
        ├──▶ [core.md: Task 4] InjectionManagerContext trait + DynamicInjector + BaseSessionModeInjector
        │
        ▼
[behaviors.md: Task 5] BaseSessionModeBehavior（共享 enter/exit 逻辑）
        │
        ├──▶ [behaviors.md: Task 6] PlanModeBehavior + DesignModeBehavior
        ├──▶ [behaviors.md: Task 7] OfficeHoursModeBehavior + GameDesignModeBehavior
        │
        ▼
[behaviors.md: Task 8] SessionModeManager（enter/exit/cancel/handoff/file resolution/design sessions）
        │
        ▼
[injection.md: Task 9] 4 个 session-mode injector + contracts + parts manifest
[injection.md: Task 10] 5 个非 mode injector（Goal/TodoList/PluginSessionStart/PermissionMode/KnowledgeMicroagent）
        │
        ▼
[injection.md: Task 11] InjectionManager（组装 + inject/inject_goal 生命周期）
        │
        ▼
[parity.md: Task 12] L3 fixtures + golden binary + parity test
```

- **Phase A（core.md Tasks 1-4）**: 全部 trait 与基础类型定义。Task 1 → Tasks 2/3/4 可部分并行（Task 2/3/4 只依赖 Task 1 中的 trait 定义，彼此无依赖）。
- **Phase B（behaviors.md Tasks 5-8）**: Task 5 是 shared behavior 基类 → Tasks 6/7 并行实现各 mode behavior → Task 8 组装 SessionModeManager。
- **Phase C（injection.md Tasks 9-11）**: Tasks 9/10 彼此并行（分别实现 session-mode injectors 和非 mode injectors）→ Task 11 组装 InjectionManager。
- **Phase D（parity.md Task 12）**: 依赖全部前序模块，生成 L3 fixture 和 golden binary，跑 TS↔Rust 对照。

**共享签名变更**：`lib.rs` 新增 3 个 `pub mod` 声明（core.md Task 1 & 2 & 4）。所有 session-mode behavior 和 injector 只管内部实现，不修改共享接口。

**硬前置**：
- 4.3.0 records 层（`SessionModeKind` 枚举、`AgentRecord::SessionModeEnter/Exit/Cancel` 变体）
- 4.3.1 context 层（`ContextMemory`、`ContextAgent`、`InjectionLifecycle`、`ReplayBuilder` trait）
- 4.3.2 config 层（`ConfigState`、`AgentConfigContext`、model alias 解析）
- 4.3.5 turn 层（`TurnSessionMode`、`TurnInjection` trait）

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `SessionModeContext` trait 接口面大（~15 方法），需暴露 records/config/context/replay/permission/kaos 子系统 | 按 TS `SessionMode` 实际访问点拆分细粒度方法；trait 只暴露 `&self` 方法（不 `&mut self`），所有可变状态通过内部 `Mutex` 管理 |
| 4 个 mode behavior 的 `modeModelKey` 查找逻辑依赖 `kimiConfig.modeModels`，该配置结构在 4.3.2 中尚未完全落地的 `OdyConfig` | `SessionModeContext` 提供 `resolve_mode_model_alias(kind) -> Option<String>` 方法；测试用 mock 直接返回已知值 |
| InjectionManager 在 step 前串行运行 8 个 injector，每个 injector 可能调用 `agent.context.appendSystemReminder()`，担心 Rust 所有权问题 | `InjectionManagerContext` trait 提供 `append_system_reminder(text, variant)` 方法，`InjectionManager` 通过 `Arc` 持有 context 引用 |
| Parts manifest 解析器（markdown table parsing）需要与 TS 逐字节一致 | 直接 port TS 的 `parsePartsManifest`/`parseManifestFiles`/`countManifestRows` 逻辑；L1 单元测试覆盖各种 markdown table 格式 |
| `ExitPlanModeReviewAsk`（permission policy stub from 4.3.3）call `agent.sessionMode.exit()` | 本计划中 SessionModeManager 完整实现 `exit()`，4.3.9 集成时 `PermissionManagerContext.exit_session_mode()` 可路由到它 |
| L3 对照需要完整的 `SessionModeManager` + `InjectionManager` + `TurnFlow` 协作，而 4.3.9 才组装 Agent | parity.md 的 golden binary 使用 `FixtureContext`（仿照 4.3.5 的 `FixtureAgent`），mock 所有外部依赖；不依赖真实 Agent |

**已做 design-lite 决策：**
- **trait-based 接口隔离**：`SessionModeManager` 通过 `SessionModeContext` trait、`InjectionManager` 通过 `InjectionManagerContext` trait 抽象对 Agent 其余子系统的依赖。4.3.7 编译测试完全独立于 4.3.9 Agent 组装。
- **Behavior 注册表模式**：`ModeBehaviorRegistry` = `HashMap<SessionModeKind, Box<dyn SessionModeKindBehavior>>`，与 TS 的 `createDefaultModeBehaviorRegistry()` 对应。
- **Injector 注册顺序按 TS `InjectionManager` 构造函数**：PluginSessionStart → TodoList → Plan → Design → OfficeHours → GameDesign → PermissionMode → [KnowledgeMicroagent] → [Goal]。此顺序影响同 step 内多条 system reminder 的排列，Rust 严格按此顺序。

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-3-7/core.md` | SessionModeKindBehavior trait + SessionModeContext + ReplayBuilder + topic-generator/directory/model-auth + InjectionManagerContext + DynamicInjector + BaseSessionModeInjector | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-3-7/session-mode.md` | BaseSessionModeBehavior + 4 个 mode behavior + SessionModeManager | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-3-7/injection.md` | 4 个 session-mode injector + contracts + parts manifest + 5 个非 mode injector + InjectionManager | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-3-7/parity.md` | L3 fixtures + golden binary + parity test | done |

---

## Spec-Coverage Table

| Roadmap 4.3.7 条目 | 覆盖 Part/Task | 状态 |
|---|---|---|
| 4.3.7.1 迁移 `SessionMode` + behaviors（enter/exit/cancel、model 恢复、context partition 切换、plan/design 文件读写） | `session-mode.md` Tasks 5-8 | covered |
| 4.3.7.2 迁移 topic-generator / directory / reviewer / model-auth | `core.md` Task 3 (topic-generator + directory + model-auth); reviewer deferred to 4.3.9 as it depends on `AdvancedSessionReviewer` | covered |
| 4.3.7.3 迁移 `InjectionManager` + 所有 injector（goal/todo-list/knowledge-microagent/parts-manifest + 4 个 session-mode injector） | `core.md` Task 4 (traits) + `injection.md` Tasks 9-11 | covered |
| 4.3.7.4 迁移 `ReplayBuilder`（记录 mode/config/permission/message 更新,用于 resume 校验） | `core.md` Task 2 | covered |
| 4.3.7.5 L3 fixture（enter plan/design/office-hours/game-design → 模型切换事件、partition 切换后的 context 事件、注入的 prompt 内容） | `parity.md` Task 12 | covered |

---

## Global Self-Review

- [x] 1. Spec-coverage: 上表覆盖 Roadmap 4.3.7 全部 5 个条目。`reviewer.ts`（`AdvancedSessionReviewer`）依赖 4.3.9 的 Agent 组装，标注为 deferred，不产生 GAP。所有其他条目均有具体 task 覆盖。
- [x] 2. Placeholder scan: 4 个 part 文件中无 TODO/TBD。唯一显式留空是 `reviewer.ts` deferred（已在 spec-coverage 表标注）、`PermissionModeInjector.previous_mode` 状态跟踪（内部 Mutex，已在代码中实现）、`KnowledgeMicroagentInjector` 和 `PluginSessionStartInjector` 的 stub 返回 `None`（pending 4.3.9 integration）。
- [x] 3. No phantom tasks: 共 12 个 task（core.md 4 个 + session-mode.md 4 个 + injection.md 3 个 + parity.md 1 个），每个 task 都产生可验证的代码/测试/fixture 变更。无 `--allow-empty` 或 "already done" 类型任务。
- [x] 4. Dependency soundness: 跨 part 依赖 `core.md → session-mode.md → injection.md → parity.md` 均为单向。每个 `Depends on:` 指向更早 task 或已存在的 4.3.0/4.3.1/4.3.2/4.3.5 符号。无反向依赖或 forward reference。
- [x] 5. Caller & build soundness: 本计划仅新增 `agent-rs` 内部 3 个新模块（`session_mode`、`injection`、`replay`），不修改 TS 共享签名或 `ody-host` RPC 路由。`lib.rs` 新增 3 个 `pub mod` 声明，每个 task 后 `cargo check -p agent-rs` 验证。parity.md 新增 `[[bin]]` 目标为增量变更。reviewer.ts deferred 不产生调用方变更。
- [x] 6. Test-the-risk: 每个 state-mutating task 都有行为断言——`ReplayBuilder` 4 个测试（message tagging、mode transitions、mode filtering、remove_last）、`SessionModeManager` 5 个测试（enter/exit/cancel/双 enter 抛错/file path）、`InjectionManager` 4 个测试（inject cycle/lifecycle callbacks）、plan injector 4 个测试（entry/exit/skip-off-turn/on_context_clear）、非 mode injector 5 个测试。L3 parity 6 个 scenario 覆盖全部 4 种 session mode + handoff + injection content。
- [x] 7. Type consistency: `SessionModeKind` 复用自 `records::nested`（4.3.0）。`SessionModeKindBehavior::kind()` 返回 `SessionModeKind`，在 `create_default_mode_behavior_registry()` 中按 `SessionModeKind` 索引。`InjectionManagerContext` 的 `is_session_mode_active()`/`session_mode_kind()` 与 `SessionModeManager` 的 `is_active()`/`kind()` 返回类型一致。`AgentReplayRecord` 枚举 tag 值与 TS 逐字对齐（`"message"`、`"session_mode_updated"`、`"config_updated"`、`"permission_updated"`、`"approval_result"`）。跨 part 无类型不一致。
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

