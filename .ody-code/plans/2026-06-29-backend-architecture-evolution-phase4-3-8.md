# 4.3.8 Background tasks & cron Implementation Plan

**Goal:** 在 `agent-rs` crate 中完整迁移 `packages/agent-core/src/agent/background/*` 与 `packages/agent-core/src/agent/cron/*`，让 Rust Agent 具备与 TS 逐事件一致的后台任务管理、定时任务调度及终端通知能力，并通过 L3 对照测试钉死等价性。

**Architecture:** 新增 `agent-rs/src/background` 与 `agent-rs/src/cron` 两个模块。`background` 层负责 `BackgroundTask` trait、三类具体任务（process/agent/question）、内存 ring-buffer、持久化与 `BackgroundManager` 生命周期；`cron` 层负责 5-field cron 表达式解析、抖动/时钟、`SessionCronStore`、调度器与 `CronManager`。两者都通过 `Arc<dyn TurnAgent>` 访问上下文/遥测/钩子/日志，通过 `Arc<TurnFlow>` 调用 `steer`/`has_active_turn`，并复用已有的 `context/notification_xml.rs` 与新增 `context/cron_fire_xml.rs` 生成注入文本。L3 对照使用 `packages/integration-tests/src/parity/background-cron-l3-driver.ts` 与 `rust-ody/crates/agent-rs/src/bin/background_cron_l3.rs` 两套 fixture 驱动，直接比较事件序列。

**Tech Stack:** Rust 2021, `tokio`, `serde`/`serde_json`, `chrono`（本地时间 cron 计算）, `rand`（可注入的 id 生成）, `tokio::signal::unix`（SIGUSR1）, `kaos-rs`（process 执行）; TS 侧用 vitest + 直接 import agent-core 内部模块做 fixture 驱动。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/Cargo.toml` | 新增 `rand`、`chrono` 依赖 |
| `rust-ody/crates/agent-rs/src/lib.rs` | 导出 `background`、`cron` 模块 |
| `rust-ody/crates/agent-rs/src/background/mod.rs` | background 模块入口与常量 |
| `rust-ody/crates/agent-rs/src/background/types.rs` | `BackgroundTask` trait、`BackgroundTaskInfo`、`BackgroundTaskSink` |
| `rust-ody/crates/agent-rs/src/background/tasks.rs` | Process / Agent / Question 三种任务实现 |
| `rust-ody/crates/agent-rs/src/background/persistence.rs` | `BackgroundTaskPersistence` + legacy 归一化 |
| `rust-ody/crates/agent-rs/src/background/manager.rs` | `BackgroundManager` 生命周期、通知、输出快照 |
| `rust-ody/crates/agent-rs/src/cron/mod.rs` | cron 模块入口 |
| `rust-ody/crates/agent-rs/src/cron/types.rs` | `CronTask`、`SessionCronTaskInit` |
| `rust-ody/crates/agent-rs/src/cron/expr.rs` | 5-field cron 解析与 next-run 计算 |
| `rust-ody/crates/agent-rs/src/cron/jitter.rs` | 递归/单次任务的确定性抖动 |
| `rust-ody/crates/agent-rs/src/cron/clock.rs` | `ClockSources`（system/file/manual） |
| `rust-ody/crates/agent-rs/src/cron/store.rs` | `SessionCronStore` |
| `rust-ody/crates/agent-rs/src/cron/scheduler.rs` | `CronScheduler` 轮询与触发 |
| `rust-ody/crates/agent-rs/src/cron/persist.rs` | cron 专用的 per-id JSON store |
| `rust-ody/crates/agent-rs/src/cron/manager.rs` | `CronManager`：fire → steer、持久化、SIGUSR1 |
| `rust-ody/crates/agent-rs/src/cron/fire_xml.rs` | `render_cron_fire_xml` |
| `rust-ody/crates/agent-rs/src/context/cron_fire_xml.rs` | cron-fire XML 渲染（复用 context 命名空间） |
| `rust-ody/crates/agent-rs/src/persistence/per_id_json_store.rs` | 通用 per-id JSON 原子写存储 |
| `rust-ody/crates/agent-rs/src/turn/types.rs` | 新增 `AgentEvent::BackgroundTaskStarted` / `BackgroundTaskTerminated` / `CronFired` |
| `rust-ody/crates/agent-rs/src/bin/background_cron_l3.rs` | Rust 侧 L3 fixture 驱动二进制 |
| `rust-ody/crates/agent-rs/tests/background_cron_l3.rs` | Rust 侧 fixture 回归测试 |
| `packages/integration-tests/src/parity/background-cron-fixture.ts` | TS/Rust 共用 fixture 类型 |
| `packages/integration-tests/src/parity/background-cron-l3-driver.ts` | TS 侧 L3 fixture 驱动 |
| `packages/integration-tests/src/parity/normalize-background-cron.ts` | background-cron snapshot 归一化 |
| `packages/integration-tests/src/parity/background-cron-parity.ts` | parity 包装器（runTsSnapshot / runRustSnapshot / assertNoDiff） |
| `packages/integration-tests/test/parity/fixtures/background-cron/*.json` | L3 对照 fixtures |
| `packages/integration-tests/test/parity/background-cron-l3-parity.test.ts` | TS↔Rust fixture 级 parity 测试 |
| `packages/integration-tests/test/parity/background-cron-normalize.test.ts` | 归一化正确性测试 |
| `packages/integration-tests/test/parity/background-cron-ts-vs-ts.test.ts` | TS↔TS 自比对 |
| `packages/integration-tests/test/parity/background-cron-cross.test.ts` | 框架对齐的 TS↔Rust cross-check |

---

## Dependency Overview

```text
[schema.md Task 1: shared types + events + deps]
        │
        ├──▶ [background.md Task 1: BackgroundTask types]
        │         │
        │         ▼
        │    [background.md Task 2: persistence]
        │         │
        │         ▼
        │    [background.md Task 3: task implementations]
        │         │
        │         ▼
        │    [background.md Task 4: BackgroundManager]
        │
        ├──▶ [cron.md Task 1: cron expression parser]
        │         │
        │         ▼
        │    [cron.md Task 2: jitter + clock]
        │         │
        │         ▼
        │    [cron.md Task 3: store + scheduler]
        │         │
        │         ▼
        │    [cron.md Task 4: CronManager + fire XML]
        │
        ▼
[integration.md Task 1: FixtureAgent update + TurnFlow wiring]
        │
        ▼
[integration.md Task 2: L3 fixture + Rust driver module]
        │
        ├──▶ [integration.md Task 3: Rust binary]
        └──▶ [integration.md Task 4: TS driver]
        │
        ▼
[integration.md Task 5: shared fixtures + parity test]
        │
        ▼
[parity.md Task 1: normalizer correctness test]
        │
        ▼
[parity.md Task 2: TS↔TS self-comparison wrapper]
        │
        ▼
[parity.md Task 3: TS↔Rust cross-check]
        │
        ▼
[parity.md Task 4: CI hook + whole-tree verification]
```

- **可并行任务**：`background.md` 与 `cron.md` 都只在 `schema.md` 完成后开始，且彼此独立，可并行开发。
- **共享签名变更**：`schema.md Task 1` 修改 `AgentEvent` 枚举并新增 `BackgroundTaskInfo` 类型；同一任务内需检查所有 `match AgentEvent` 位置并补全 `_ =>` 或显式分支，最后跑 `cargo check -p agent-rs --workspace --tests`。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| Rust `chrono` 本地时区与 JS `Date` 本地语义不一致（夏令时、闰秒） | fixture 用简单表达式验证；cron 解析器直接对照 TS 输出同一组 `(expr, fromMs) -> nextMs` |
| 后台任务 id / cron id 随机导致对照不稳定 | 两边管理器都接受可注入 `id_generator`；fixture 指定固定 id |
| 进程信号在 macOS/Linux 行为差异 | process task 用 POSIX process group kill，测试限定 `cfg(unix)`；parity CI 跑 darwin+linux |
| 输出 ring-buffer 用字符数 vs 字节数与 TS 有偏差 | 小输出 fixture 不触发丢弃；大输出 fixture 单独验证 `outputSizeBytes` 语义 |
| `cron.fired` 事件需要 `TurnFlow::steer` 同步返回 | `TurnFlow::steer` 已是同步方法；manager 持有 `Arc<TurnFlow>` 直接调用 |
| SIGUSR1 信号处理器在测试进程泄漏 | `CronManager::stop` 显式 unbind；测试用 `manual` 时钟，不依赖信号 |

**已做 design-lite 决策（`schema.md Task 1` 详述）：**
- 后台任务与 cron 不新增 WAL record 变体，只通过 `AgentEvent` 与 per-id JSON 文件持久化，与 TS 当前行为一致。
- 管理器对 `TurnFlow` 的依赖采用**具体类型持有**（`Arc<TurnFlow>`）而非新增 trait，因为 4.3.5 已稳定实现且当前唯一实现者就是 `TurnFlow`；4.3.9 组装 Agent 时直接注入同一实例即可。

---

## Spec-Coverage Table

| Roadmap 4.3.8 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.3.8.1 迁移 `BackgroundManager` + task types | `background.md` Task 1–4 | covered |
| 4.3.8.2 迁移 `BackgroundTaskPersistence` | `background.md` Task 2 | covered |
| 4.3.8.3 迁移 `CronManager` | `cron.md` Task 1–4 | covered |
| 4.3.8.4 L3 fixture（后台任务生命周期 + cron fire → steer 事件） | `integration.md` Task 2–5、`parity.md` Task 1–4 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-29-backend-architecture-evolution-phase4-3-8/schema.md` | 共享类型、`AgentEvent` 扩展、常量、依赖声明、XML 辅助 | done |
| 2 | `2026-06-29-backend-architecture-evolution-phase4-3-8/background.md` | `BackgroundTask` trait、三种任务、持久化、`BackgroundManager` | done |
| 3 | `2026-06-29-backend-architecture-evolution-phase4-3-8/cron.md` | cron 解析、抖动/时钟、store、scheduler、`CronManager` | done |
| 4 | `2026-06-29-backend-architecture-evolution-phase4-3-8/integration.md` | `FixtureAgent` 改造、L3 fixture、Rust binary、TS driver | done |
| 5 | `2026-06-29-backend-architecture-evolution-phase4-3-8/parity.md` | 归一化、TS↔TS 自比对、TS↔Rust 对照、CI/验证命令 | done |

---

## Global Self-Review

- [ ] 1. Spec-coverage：上表覆盖 Roadmap 4.3.8 全部 4 个条目，无 GAP。
- [ ] 2. Placeholder scan：所有 part 文件均无 TODO/TBD；每个 task 给出完整代码、命令与预期输出。
- [ ] 3. No phantom tasks：每个 task 都产生可验证的文件/测试/fixture 变更；无 `--allow-empty` 或 "already done" 类型任务。
- [ ] 4. Dependency soundness：`background.md` / `cron.md` 均依赖 `schema.md`；`integration.md` 依赖 `background.md` + `cron.md`；`parity.md` 依赖 `integration.md` 的 fixture / driver / binary；所有 `Depends on:` 指向更早的 task 或 part，无反向依赖。
- [ ] 5. Caller & build soundness：`schema.md Task 1` 修改 `AgentEvent` 枚举，同一任务检查所有 `match AgentEvent` 位置并补全分支，以 `cargo check -p agent-rs --workspace --tests` 验证；`integration.md Task 2` 修改 `CronManager::new` 签名，同一任务用 `grep -rn` 更新所有调用者并以 `cargo check -p agent-rs` 收尾；TS 侧最终需跑 `pnpm --filter @odysseythink/integration-tests typecheck`。
- [ ] 6. Test-the-risk：每个状态变更任务都附带行为断言——后台任务注册/终止改变任务列表、stop 触发 killed 状态、ring-buffer 截断行为、persistence 文件存在性、cron 触发产生 steer 事件、7 天 stale 自动删除、持久化 cursor 防止 resume 重放；parity 层额外断言 `normalizeBackgroundCronSnapshot` 不误杀 must-survive 字段，且 TS 自比对 / TS↔Rust 对照 diff 为零。
- [ ] 7. Type consistency：`BackgroundTaskInfo`、`CronTask`、`AgentEvent` 新增变体、`PromptOrigin::BackgroundTask` / `CronJob` 等类型/字段名在 background/cron/integration/parity 各 part 中保持一致；`normalizeBackgroundCronSnapshot`、`runBackgroundCronL3Fixture`、`BackgroundCronSnapshot` 在 parity.md 中复用 integration.md 的定义，无重复/冲突类型。
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

