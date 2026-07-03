# 4.3.3 Permission system Implementation Plan

**Goal:** 在已落地的 `agent-rs` 4.3.0 records 层和 4.3.2 ToolManager 之上，实现 `PermissionManager` 与全部 ~18 个决策策略（policies），使 permission 模块能独立编译测试，并通过 L3 事件流对照验证四种模式（manual/yolo/auto/plan）下每条对同一工具的决策与 TS 逐字段一致。

**Architecture:** 在 `agent-rs` crate 中新增 `permission` 模块，包含 `PermissionManager`（核心状态机）、`matches_rule`（pattern matching）、`policies/`（~18 个 policy 实现）。`PermissionManager` 通过 `PermissionManagerContext` trait 抽象对 Agent 其余子系统的最小依赖（records、tools、kaos、config、sessionMode、hooks、rpc、telemetry），避免循环引用。`PrepareToolExecutionHook` 已在 `agent_loop::types` 定义，`PermissionManager.before_tool_call` 实现该 trait 并以 policy chain 顺序运行：PreToolCallHook → mode-based → deny rules → auto-mode approve → session-history → ask-rules → allow-rules → file-access → plan-mode-guard → yolo-approve → default-approve → fallback-ask。所有状态变更通过 `AgentRecords::log_record` 写 WAL（`permission.set_mode` / `permission.record_approval_result`）。

**Tech Stack:** Rust 2021 edition, `serde` + `serde_json`, `thiserror`, `async_trait`; 复用 `agent-rs` 的 `records::nested::{PermissionMode, PermissionApprovalResultRecord, ApprovalResponse}`、`agent_loop::types::{PrepareToolExecutionResult, ToolExecutionHookContext, ResolvedToolExecutionHookContext, RunnableToolExecution, ToolAccesses, ToolResourceAccess}`、`tool::types::ToolInfo`；matches-rule 需新实现 `parse_pattern`（port 自 `@odysseythink/agent-core-shared` 的 `parsePattern`）。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/lib.rs` | 新增 `pub mod permission;` |
| `rust-ody/crates/agent-rs/src/permission/mod.rs` | 模块入口，re-export |
| `rust-ody/crates/agent-rs/src/permission/types.rs` | `PermissionRule`、`PermissionRuleDecision`、`PermissionRuleScope`、`PermissionDecision`、`PermissionData`、`PermissionPolicy` trait、`PermissionPolicyContext`、`PermissionPolicyResult`、`PermissionPolicyResolution`、`ApprovalRequest` |
| `rust-ody/crates/agent-rs/src/permission/matches_rule.rs` | `parse_pattern` + `match_permission_rule` + `PermissionRuleMatch` |
| `rust-ody/crates/agent-rs/src/permission/manager.rs` | `PermissionManager` + `PermissionManagerContext` trait |
| `rust-ody/crates/agent-rs/src/permission/policies/mod.rs` | `create_permission_decision_policies()` 工厂函数 |
| `rust-ody/crates/agent-rs/src/permission/policies/yolo_mode_approve.rs` | `YoloModeApprove` |
| `rust-ody/crates/agent-rs/src/permission/policies/auto_mode_approve.rs` | `AutoModeApprove` |
| `rust-ody/crates/agent-rs/src/permission/policies/auto_mode_ask_user_question_deny.rs` | `AutoModeAskUserQuestionDeny` |
| `rust-ody/crates/agent-rs/src/permission/policies/plan_mode_guard_deny.rs` | `PlanModeGuardDeny` |
| `rust-ody/crates/agent-rs/src/permission/policies/user_configured_rules.rs` | `UserConfiguredDeny` / `UserConfiguredAllow` / `UserConfiguredAsk` |
| `rust-ody/crates/agent-rs/src/permission/policies/session_approval_history.rs` | `SessionApprovalHistory` |
| `rust-ody/crates/agent-rs/src/permission/policies/browser_tool_ask.rs` | `BrowserToolAsk` |
| `rust-ody/crates/agent-rs/src/permission/policies/exit_plan_mode_review_ask.rs` | `ExitPlanModeReviewAsk` |
| `rust-ody/crates/agent-rs/src/permission/policies/plan_mode_tool_approve.rs` | `PlanModeToolApprove` |
| `rust-ody/crates/agent-rs/src/permission/policies/file_access_ask.rs` | `SensitiveFileAccessAsk` / `GitControlPathAccessAsk` / `CwdOutsideFileWriteAsk` + helper functions |
| `rust-ody/crates/agent-rs/src/permission/policies/fallback_ask.rs` | `FallbackAsk` |
| `rust-ody/crates/agent-rs/src/permission/policies/default_tool_approve.rs` | `DefaultToolApprove` |
| `rust-ody/crates/agent-rs/src/permission/policies/idea_tool_directory.rs` | `IdeaToolDirectory` |
| `rust-ody/crates/agent-rs/src/permission/policies/git_cwd_write_approve.rs` | `GitCwdWriteApprove` |
| `rust-ody/crates/agent-rs/src/permission/policies/pre_tool_call_hook.rs` | `PreToolCallHook` |
| `rust-ody/crates/agent-rs/tests/permission_manager.rs` | `PermissionManager` 单元测试 |
| `rust-ody/crates/agent-rs/tests/permission_policies.rs` | 全部 policies 单元测试（按 mode + tool + context 组合） |
| `rust-ody/crates/agent-rs/tests/permission_matches_rule.rs` | `matches_rule` 单元测试 |
| `rust-ody/crates/agent-rs/tests/permission_fixture_parity.rs` | L3 fixture 生成 + Rust↔TS 事件流对照 |

---

## Dependency Overview

```text
[4.3.0 records/nested: PermissionMode, PermissionApprovalResultRecord, ApprovalResponse]
[4.3.2 tool/types: ToolInfo]
[agent_loop/types: PrepareToolExecutionResult, ToolExecutionHookContext, ResolvedToolExecutionHookContext, RunnableToolExecution]
[agent_loop/tool_access: ToolAccesses, ToolResourceAccess]
        │
        ├──▶ [core.md: Task 1] permission/types.rs — 全部 permission 类型
        │         │
        │         ▼
        │    [core.md: Task 2] matches_rule.rs — parse_pattern + match_permission_rule
        │         │
        │         ▼
        │    [core.md: Task 3] manager.rs — PermissionManager + PermissionManagerContext trait
        │         │
        │         ├──▶ [policies.md: Task 4] simple mode-based policies (7 个)
        │         ├──▶ [policies.md: Task 5] rule-based policies (4 个：user-configured ×3 + session-approval-history ×1)
        │         ├──▶ [policies.md: Task 6] file-access policies (3 个)
        │         └──▶ [policies.md: Task 7] plan/design mode policies (5 个)
        │
        ▼
[parity.md: Task 8] L3 fixtures + Rust↔TS 事件流对照
```

- **Phase A（core.md Tasks 1–3）**: 类型 → matches_rule → PermissionManager，串行依赖。
- **Phase B（policies.md Tasks 4–7）**: 全部在 Phase A 之后；Tasks 4/5/6/7 彼此无依赖，可并行开发。Task 4（simple mode-based）只需要 `PermissionManagerContext.mode()` 和类型。Task 5（rule-based）需要 `matches_rule`。Task 6（file-access）需要 `kaos`/`config` 的 trait 方法。Task 7（plan/design）需要 `sessionMode` 的 trait 方法。
- **Phase C（parity.md Task 8）**: 依赖全部政策和 PermissionManager。

**共享签名变更**：`rust-ody/crates/agent-rs/src/lib.rs` 新增 `pub mod permission;`（core.md Task 1）。所有 policy 模块只管内部实现，不修改共享接口。`PermissionManager` 实现 `agent_loop::types::PrepareToolExecutionHook` trait（已在 agent_loop 中定义，无需变更）。

**硬前置**：
- 4.3.0 records 层（`PermissionMode`、`PermissionApprovalResultRecord` 等类型已存在）
- 4.3.2 ToolManager（`ToolInfo` 等类型已存在；`PermissionManagerContext` 的 `get_tool_info()` 方法依赖此层）
- `agent_loop::types`（`PrepareToolExecutionResult`、`ToolExecutionHookContext` 等已定义）

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `parse_pattern` 需要从 `@odysseythink/agent-core-shared` port 到 Rust，与 TS `picomatch` glob 语义必须一致 | 对照测试覆盖一组精心挑选的 pattern（`*` / `**` / `Read(/etc/**)` / `Bash(rm *)` 等），Rust 使用 `globset` crate 做 glob matching；若 `globset` 语义与 `picomatch` 不完全一致，在测试阶段发现并调整 |
| `ExitPlanModeReviewAsk` 调用 `agent.sessionMode.exit()`，而 sessionMode 在 4.3.7 | `PermissionManagerContext` trait 提供 `exit_session_mode()` 方法；4.3.3 测试使用 mock 实现验证 record / event 行为 |
| `file-access-ask` 的 `isSensitiveFile` 等敏感文件判定依赖 `tools/policies/sensitive.ts` | `is_sensitive_file` 直接 port 到 `permission::policies` 模块内作为纯函数；路径列表从 TS 源码复制，测试覆盖 |
| `GitCwdWriteApprove` 和 `GitControlPathAccessAsk` 依赖 `findGitWorkTreeMarker` | `find_git_work_tree_marker` 作为 `PermissionManagerContext` 的 trait 方法；测试 mock 返回固定 marker |
| L3 对照需要完整的 `PermissionPolicyContext` 构造（含 `RunnableToolExecution` 的 `accesses`、`display`、`matches_rule`） | parity.md 的 fixture 生成器为每种 scenario 构造 mock execution；重点验证 manual/yolo/auto/plan 四种模式的决策链 |

**已做 design-lite 决策：**
- **trait-based 接口隔离**：与 4.3.2 一致，`PermissionManager` 通过 `PermissionManagerContext` trait 抽象对 Agent 其余子系统的依赖（records、tools、kaos、config、sessionMode、hooks、rpc、telemetry、agent_type）。4.3.3 编译测试完全独立于 4.3.5 TurnFlow / 4.3.7 SessionMode。
- **`parse_pattern` 使用 `globset` crate**：TS 的 `parsePattern` 底层使用 `picomatch` 做 glob matching；Rust 的 `globset` 是 `ripgrep` 使用的高质量 glob 库，语义基本对齐。对照测试覆盖路径 pattern 以确保一致性。
- **policy 注册顺序等同于 TS `createPermissionDecisionPolicies()`**：policy chain 顺序是权限正确性的核心（先 deny 后 approve，session history 在 ask rules 之前），Rust 严格按 TS 顺序注册。

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-28-backend-architecture-evolution-phase4-3-3/core.md` | types + matches_rule + PermissionManager core | done |
| 2 | `2026-06-28-backend-architecture-evolution-phase4-3-3/policies.md` | 全部 ~18 个 policy 实现 + 工厂函数 | done |
| 3 | `2026-06-28-backend-architecture-evolution-phase4-3-3/parity.md` | L3 fixtures + Rust↔TS 事件流对照 | done |

---

## Spec-Coverage Table

| Roadmap 4.3.3 条目 | 覆盖 Part | 状态 |
|---|---|---|
| 4.3.3.1 迁移 `PermissionManager` | `core.md` Task 3 | covered |
| 4.3.3.2 迁移所有 policy | `policies.md` Tasks 4–7 | covered |
| 4.3.3.3 迁移 `matches-rule` | `core.md` Task 2 | covered |
| 4.3.3.4 L3 fixture | `parity.md` Task 8 | covered |

---

## Global Self-Review

- [x] 1. Spec-coverage: 上表覆盖 Roadmap 4.3.3 全部 4 个条目（PermissionManager、全部 policy、matches-rule、L3 fixture），无 GAP。
- [x] 2. Placeholder scan: 三个 part 文件中无 TODO/TBD。唯一显式留空的是 `ExitPlanModeReviewAsk` policy（标注 "full impl in 4.3.7"）和 `GitCwdWriteApprove`/`GitControlPathAccessAsk` 的 `find_git_work_tree_marker()` 依赖（标注 "stub"，在无 git repo 的测试环境中正确返回 None）。`PermissionManagerContext` trait 完整声明了所有 policies 需要的方法。
- [x] 3. No phantom tasks: 共 8 个 task（core.md 3 个 + policies.md 4 个 + parity.md 1 个），每个 task 都产生可验证的代码/测试/fixture 变更。无 `--allow-empty` 或 "already done" 类型任务。
- [x] 4. Dependency soundness: `core.md` Task 1→2→3 串行；`policies.md` Tasks 4–7 全部依赖 `core.md` Task 3 且彼此无依赖；`parity.md` Task 8 依赖所有前序 part。无反向依赖或跨 part 的 forward reference。
- [x] 5. Caller & build soundness: 本计划仅新增 `agent-rs` 内部 `permission` 模块，不修改 TS 共享签名或 `ody-host` RPC 路由。`lib.rs` 新增 `pub mod permission;`（core.md Task 1），每次 commit 后用 `cargo check -p agent-rs --tests` 验证。`globset` crate 依赖在 core.md Task 2 添加。TS 侧仅新增一个 parity test 文件读取 Rust 生成的 JSON fixture。
- [x] 6. Test-the-risk: 每个 state-mutating task 都有行为断言——`PermissionManager.set_mode` 写 WAL + emit status（core.md Task 3）、`record_approval_result` session scope 缓存 pattern（core.md Task 3）、`match_permission_rule` 全部匹配组合（core.md Task 2）、policy chain 10 个 scenario 覆盖 yolo/auto/manual 三种模式 + 4 种 rule 决策 + 3 种 file-access + plan-mode guard + default approve + idea directory（parity.md Task 8）。
- [x] 7. Type consistency: `PermissionMode` 复用自 `records::nested`（4.3.0 已有）；`PermissionRule`/`PermissionRuleDecision`/`PermissionRuleScope` 的 serde 标注与 TS 枚举值逐字一致；`PermissionPolicyResult` 使用 `#[serde(tag = "kind")]` 匹配 TS 的 discriminated union；`PermissionPolicyContext` 字段名和类型与 TS `ResolvedToolExecutionHookContext` 对齐。所有跨 part 的类型引用（`PermissionPolicyResolution`、`PermissionRule`、`PermissionPolicy` trait）在 core.md Task 1 定义后被 policies.md 和 parity.md 使用，无名称或类型不一致。
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

