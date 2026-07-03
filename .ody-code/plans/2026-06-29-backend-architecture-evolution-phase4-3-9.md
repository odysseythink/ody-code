# Phase 4.3.9 Agent Orchestrator & CoreHost Integration 实施计划

**Goal:** 在 `agent-rs` 中组装出实现 `TurnAgent` 的真实 `Agent` 类型，把它接入 `ody-host` 的 `CoreHost` RPC 路由，并通过 TS↔Rust 的 L2/L3/L4 对照门。

**Architecture:** 复用 4.3.0–4.3.8 已落地的 `agent-rs` 子模块（records/context/config/permission/tool/usage/session-mode/background/cron/injection/replay/compaction/turn）。新增一个 `Agent` struct 实现 `TurnAgent` trait，并在 `ody-host` 侧增加 `HostAgentContext` 桥接：把 `AgentEvent` 映射为 `ody-host/events.rs` 的 `AgentEvent`，通过 `EventSink` 发出；把 approval/hook RPC 转发到 SDK 侧。`CoreHost` 从「直接调 provider」改为每个 session 持有一个 `Agent` + `TurnFlow`，`prompt/steer/cancel/setModel/setPermission/enterPlan` 等 RPC 路由到 `Agent`。session 持久化扩展为保存 agent records 路径与 resume 状态，使 TS 创建 → Rust resume → TS 再 resume 的 L4 场景成立。

**Tech Stack:** Rust (tokio / async-trait / serde_json / anyhow)、TypeScript (Vitest / parity harness / `@odysseythink/ody-code-sdk`)。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## 文件结构总览

| 文件 | 职责 | 所属 Part |
|---|---|---|
| `rust-ody/crates/agent-rs/src/agent.rs` | 真实 `Agent` struct + `AgentBuilder` | Part 1 |
| `rust-ody/crates/agent-rs/src/lib.rs` | 导出 `agent` 模块 | Part 1 |
| `rust-ody/crates/agent-rs/src/turn/types.rs` | `TurnAgent` trait（已有，视需要添加 `set_context_mode` 等辅助方法） | Part 1 |
| `rust-ody/crates/ody-host/Cargo.toml` | 添加 `agent-rs` 依赖 | Part 2 |
| `rust-ody/crates/ody-host/src/host.rs` | `CoreHost` 持 `Agent`、改写 `prompt/steer/cancel` 等 dispatch | Part 2 |
| `rust-ody/crates/ody-host/src/events.rs` | 扩展/调整 host 事件类型以覆盖 agent 事件 | Part 2 |
| `rust-ody/crates/ody-host/src/agent_bridge.rs` | `HostAgentContext`、`HostEventEmitter`、`HostApprovalClient` 等桥接实现 | Part 2 |
| `rust-ody/crates/ody-host/src/session/store.rs` | `SessionState` 增加 `agent_records_path` / `resume_state` | Part 2 |
| `rust-ody/crates/ody-host/src/session/manager.rs` | `Session` 创建/恢复时构造/复用 `Agent` | Part 2 |
| `packages/integration-tests/src/parity/scenarios/agent-api-l2.ts` | L2 AgentAPI 方法 scenario | Part 3 |
| `packages/integration-tests/src/parity/scenarios/mock-prompt.ts` 等 | 复用并扩展现有 L3 scenario | Part 3 |
| `packages/integration-tests/src/parity/scenarios/session-mode-handoff.ts` | session-mode handoff L3 scenario | Part 3 |
| `packages/integration-tests/src/parity/scenarios/background-cron.ts` | background + cron L3 scenario | Part 3 |
| `packages/integration-tests/src/parity/scenarios/resume-cross-host.ts` | L4 TS→Rust→TS resume scenario | Part 3 |
| `packages/integration-tests/src/parity/normalize.ts` 及相关 normalizer | 扩展归一化规则 | Part 3 |
| `packages/integration-tests/test/parity/ts-vs-rust.test.ts` | 注册新增 scenario | Part 3 |
| `.github/workflows/rust-host.yml` | 增加 `parity-l4` job 与 benchmark 步骤 | Part 3 |
| `rust-ody/ts/bench.ts` | 常驻内存 / 冷启动 / 空闲 CPU benchmark | Part 3 |

---

## 依赖图与阶段划分

```
Part 1: agent-rs Agent 组装
  Task 1 — AgentBuilder 与上下文 trait 实现
  Task 2 — Agent 构造顺序与 turn/llm 适配

Part 2: ody-host CoreHost 集成
  Task 3 — Cargo 依赖与共享事件类型调整
  Task 4 — HostAgentContext / approval / hook 桥接
  Task 5 — CoreHost dispatch 路由到 Agent (prompt/steer/cancel/setModel/setPermission/enterPlan/getConfig...)
  Task 6 — session 持久化扩展与 resume 路径

Part 3: 对照测试与基准
  Task 7 — L2 AgentAPI 逐方法对照
  Task 8 — L3 scenario (multi-turn-tool / session-mode handoff / background-cron)
  Task 9 — L4 跨宿主 resume 对照
  Task 10 — 内存/冷启动/空闲 CPU benchmark
```

- **Part 1 是 Part 2 的硬前置**：`ody-host` 必须依赖并构造真实 `Agent`。
- **Part 2 是 Part 3 的硬前置**：没有 RPC 路由和事件输出，对照 scenario 无法运行。
- **Part 3 内部**：Task 7/8/9 顺序执行（L2 → L3 → L4），Task 10 与 9 可并行但建议最后做。
- **共享签名变更**：Task 3（Cargo 依赖 + 事件类型调整）与 Task 6（`SessionState` 字段新增）都需要在同一 task 内更新所有调用方并跑全树 typecheck。

---

## Parts 清单

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-3-9/agent-rs.md` | `Agent` 组装 + `TurnAgent` 适配 | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-3-9/ody-host.md` | CoreHost 集成 + session 持久化 | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-3-9/parity.md` | L2/L3/L4 对照 + benchmark | done |

---

## 规格覆盖表（对应 roadmap §4.3.9 条目）

| Roadmap 条目 | 内容 | 覆盖 Part/Task | 状态 |
|---|---|---|---|
| 4.3.9.1 | 实现 `Agent` 组装器（构造所有子模块、setContextMode、refreshLlm、llm、generate、useProfile、resume） | Part 1 Task 1-2 | covered |
| 4.3.9.2 | 实现 `AgentAPI` RPC 路由（prompt/steer/cancel/setModel/setThinking/setPermission/enterPlan/getConfig/...） | Part 2 Task 5 | covered |
| 4.3.9.3 | 实现 resume 路径（records replay → 恢复 context/config/permission/session-mode/tools/background/cron） | Part 2 Task 6 | covered |
| 4.3.9.4 | L2 对照：所有 `AgentAPI` 方法 TS vs Rust 返回值一致 | Part 3 Task 1 | covered |
| 4.3.9.5 | L3 对照：mock provider 多轮 tool-call、session-mode handoff、background/cron scenario | Part 3 Task 2-4 | covered |
| 4.3.9.6 | L4 对照：完整会话 TS 创建 → Rust resume → 继续 → TS 再 resume | Part 3 Task 5 | covered |
| 4.3.9.7 | 基准：常驻内存、冷启动、空闲 CPU 对照 | Part 3 Task 6 | covered |

---

## 风险与未决问题

1. **`TurnAgent` 与 `ContextAgent` 生命周期错位**。`ContextMemory` 需要 `&'a dyn ContextAgent`，而 `TurnAgent` 是 `Arc<dyn TurnAgent>`。真实 `Agent` 必须同时实现两者，或通过内部引用自指。计划中使用 `Arc<AgentInner>` + `unsafe self-reference` 或把所有子模块包进 `Agent` 并通过 `Arc::clone` 传引用；具体实现细节在 Part 1 中给出。
2. **`ody-host` 事件类型与 `agent-rs::AgentEvent` 字段不完全一致**。需要新增/重命名 host 事件字段（如 `tool.call.started`、`tool.result` payload、compaction 事件）。所有变更必须在 Task 3 中一次性完成并更新 host 内所有 emit 点。
3. **`ConfigState<C>`、`ToolManager<C>`、`UsageRecorder<C>` 等子模块都带有泛型 context**。真实 `Agent` 将作为它们的 context，但 `Agent` 本身又持有它们，形成循环。计划中使用 `Agent::new` 分阶段构造：先建 `AgentInner`（不含 config/tools/usage），再把这些子模块 attach 进 inner；具体代码在 Part 1 给出。
4. **approval / hook RPC**。`PermissionManagerContext` 的 `request_approval` 和 `fire_hook_*` 是异步 trait 方法，需要 `HostAgentContext` 持有 `EventSink` 的引用。`EventSink::request` 已经是 async，可直接复用。
5. **`generate_one_off` 与 `KosongLLM`**。`TurnLlmResolver` 要求实现 `generate_one_off`；真实 Agent 将复用 4.3.5 已存在的 `KosongLLM`/`RemoteKosongLLM` 适配器（若已落地），否则在 Part 1 中补齐最小实现。
6. **`SessionState` 增加 resume 字段后，旧 state.json 需要向后兼容**。新增字段使用 `#[serde(default)]`，读取旧文件时缺失字段自动取默认值。
7. **并发事件顺序归一化**。background/cron 多任务同时完成时，parity normalizer 需要按 taskId 排序；已在 harness 中存在，Part 3 中显式 assert。

---

## 自检清单（在 Part 文件生成后逐项确认）

- [x] 1. Spec-coverage table: 见上表，所有 4.3.9 条目均已映射到 Part/Task。
- [x] 2. Placeholder scan: 索引不含 TODO/TBD；Part 文件中每个 task 都有完整代码与命令。
- [x] 3. No phantom tasks: 每个 task 产生可验证的编译/测试/文件变更；无 `--allow-empty`。
- [x] 4. Dependency soundness: Part 2 依赖 Part 1，Part 3 依赖 Part 2；无反向依赖。
- [x] 5. Caller & build soundness: 共享签名变更（事件类型、`SessionState`、`Cargo.toml`）在同一 task 中更新所有调用方并以全树 typecheck 结束。
- [x] 6. Test-the-risk: 每个状态变更 task 都有行为断言；filter/regex 用 must-survive 输入验证。
- [x] 7. Type consistency: Part 1 定义的 `Agent`/`AgentBuilder` API 与 Part 2 调用处一致。
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

