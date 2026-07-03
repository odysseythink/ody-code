# Phase 4.1.3 进程执行（exec / KaosProcess / kill）迁移实施计划

**Goal:** 在 `rust-ody/crates/kaos-rs` 中实现与 TS `LocalKaos.exec` / `LocalProcess` 逐行为对齐的进程执行能力（含 `exec` / `exec_with_env`、stdout/stderr 缓冲、`wait` / `exit_code`、POSIX 进程组 kill、Windows 进程树 taskkill），并通过 `packages/integration-tests/src/parity/kaos-golden` 的 L1 golden fixture 完成 TS↔Rust 对照，为 4.1.4 的 `BashTool` 迁移提供进程地基。

**Architecture:** 在 `kaos-rs` 新增 `process.rs` 模块承载 `Process` 结构与进程相关方法；`Kaos` struct 暴露 `exec` / `exec_with_env`，继承实例级 `cwd` 并可选注入环境变量。`Process` 内部使用 `tokio::process::Command` 启动子进程，启动独立的 tokio 任务并发收集 stdout/stderr 字节，保证 `wait()` 之后再读取也不会丢失数据。POSIX 平台通过 `process_group(0)` 让子进程成为进程组组长，`kill()` 使用 `killpg(-pid, signal)` 并兼容 `ESRCH` / `EPERM`；Windows 平台回退到 `taskkill /T`。L1 对照复用现有 `kaos-golden` binary 与 TS runner，新增 `l1-process-ops.json` fixture，最终由 `test/parity/kaos/l1-golden.test.ts` 做结构化 diff。

**Tech Stack:** Rust (tokio::process, nix on Unix), TypeScript / Vitest, `@odysseythink/kaos`, `packages/integration-tests`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Path | Responsibility |
|---|---|
| `rust-ody/crates/kaos-rs/src/process.rs` | `Process` struct、`exec` / `exec_with_env` 内核、POSIX 进程组 kill、Windows `taskkill` fallback、单元测试 |
| `rust-ody/crates/kaos-rs/src/kaos.rs:85-180` | 为 `Kaos` struct 绑定 `exec` / `exec_with_env` 公共方法 |
| `rust-ody/crates/kaos-rs/src/lib.rs:11` | 导出 `process` 模块 |
| `rust-ody/crates/kaos-rs/Cargo.toml:24-26` | 新增 `nix` 依赖（Unix only） |
| `rust-ody/crates/kaos-rs/src/golden.rs:35-70` | 扩展 `Op` 枚举支持 `exec` 与 `kill_tree` 用例 |
| `rust-ody/crates/kaos-rs/src/bin/golden.rs:1-11` | 无需修改，复用已有 golden binary |
| `packages/integration-tests/src/parity/kaos-golden.ts:20-95` | 扩展 TS golden runner 的 `GoldenOp` 类型与 `runTsCase` |
| `packages/integration-tests/src/parity/fixtures/kaos/l1-process-ops.json` | 进程操作 L1 golden fixture |
| `packages/integration-tests/test/parity/kaos/l1-golden.test.ts:38` | 在 fixtures 列表中加入 `l1-process-ops.json` |

---

## Dependency Overview

```
Prerequisite: Phase 4.1.0 kaos-rs crate 骨架已就位（Kaos struct / cwd / path / errors / golden harness）
  │
  ▼
Phase A — Rust 进程实现
  ├── Task A1: Process 结构 + exec 基本能力
  ├── Task A2: cwd 继承 + exec_with_env
  ├── Task A3: stdout/stderr 缓冲 + wait-before-read
  ├── Task A4: exit_code / 命令不存在
  ├── Task A5: POSIX 进程组 kill
  └── Task A6: Windows taskkill fallback（手动验证）
  │
  ▼
Phase B — L1 golden 对照
  ├── Task B1: 扩展 Rust `golden.rs` `Op::Exec` / `Op::KillTree`
  ├── Task B2: 扩展 TS `kaos-golden.ts` `exec` / `kill_tree`
  ├── Task B3: 创建 `l1-process-ops.json` fixture
  ├── Task B4: 在 Rust / TS parity 测试注册 fixture
  └── Task B5: 全量验证 + changeset
```

- Phase A 与 Phase B 之间存在严格先后关系：Phase B 依赖 Phase A 提供的 `exec` / `exec_with_env` / `Process::stdout/stderr/exit_code/kill` API。
- Phase A 内部建议顺序执行：A3 缓冲依赖 A1 的数据结构；A5 kill 依赖 A1/A2 的进程启动能力；A6 与 A5 平台互斥，可视为同一 kill 方法的不同 `cfg` 分支。
- 整个 4.1.3 依赖 4.1.0 已交付的 crate 边界、错误类型、`Kaos` 实例级 `cwd`、以及 `golden.rs`  harness。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| POSIX 进程组 kill 在 macOS 与 Linux 的 `setpgid` / `process_group` 语义有细微差异 | A5 用例同时覆盖直接子进程与 `sh` 派生的孙进程，CI 矩阵跑 darwin-arm64 + linux-x64 |
| 大输出场景下 pipe buffer 导致死锁 | A3 的单元测试显式用 200KB+ 输出验证 stdout reader 与 `child.wait()` 并发 |
| Windows 进程树杀死法与 TS 的 `taskkill /T` 路径必须一致 | A6 保留与 TS 完全相同的参数：`/T /PID <pid>`（非 force）或 `/T /F /PID <pid>`（SIGKILL） |
| 命令不存在时的错误形态 TS 与 Rust 必须一致 | A4 将 `ENOENT` 统一映射为 `"not found"`，与现有 `canonicalIoError` 对齐 |
| `kill` 信号名称映射 | A5 仅支持 `SIGTERM`（默认）、`SIGKILL`、`SIGINT`，与 Node `process.kill` 常用信号对齐； fixture 中只使用 `SIGTERM` / `SIGKILL` |

---

## Spec Coverage

| 路线图 4.1.3 条目 | 覆盖位置 | 状态 |
|---|---|---|
| 4.1.3.1 `exec / execWithEnv`（tokio::process::Command、继承 cwd、环境变量隔离） | Part 1: Task A1, A2 | covered |
| 4.1.3.2 `KaosProcess`（stdout/stderr 流式 + 缓冲、pid、exitCode、wait、kill） | Part 1: Task A1, A3, A4 | covered |
| 4.1.3.3 POSIX 进程组 kill（detached + kill(-pgid, signal)、ESRCH/EPERM 容错） | Part 1: Task A5 | covered |
| 4.1.3.4 Windows 进程树 kill（taskkill /T fallback） | Part 1: Task A6 | covered |
| 4.1.3.5 L1 golden 进程 fixture（echo / 退出码 / stderr / 大输出 / 未找到命令） | Part 2: Task B1, B2, B3, B4 | covered |
| 门 G4-1-3（exec 在 darwin+linux L1 绿；kill 进程树用例绿） | Part 2: Task B4, B5 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-1/core.md` | Rust `kaos-rs` 进程实现 | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-1/parity.md` | L1 golden fixture + TS/Rust harness + parity test | done |

---

## Global Self-Review

- [ ] 1. Spec-coverage table：上表已覆盖 roadmap 4.1.3 全部 5 个编号条目 + G4-1-3 门。
- [ ] 2. Placeholder scan：index 与两个 part 文件均已完成，无 TODO/TBD，无 deferred-by-dependency 占位。Windows taskkill fallback 给出完整实现与手动验证步骤。
- [ ] 3. No phantom tasks：每个 Part 任务都产生可验证的代码/测试/配置变更；index 不产生 `--allow-empty`。
- [ ] 4. Dependency soundness：Phase B 依赖 Phase A；Phase A 内部 Task A3/A4/A5/A6 依赖 A1；A2 与 A3/A4 可并行但建议顺序；所有依赖均指向前置任务或 4.1.0 前提。
- [ ] 5. Caller & build soundness：
  - Rust `Op` 变体新增后，`bin/golden.rs` 与 `tests/golden.rs` 无需修改（增量变体兼容），Part 1 结束跑 `cargo test -p kaos-rs` + `cargo clippy -p kaos-rs`；Part 2 B1 跑 `cargo check -p kaos-rs`。
  - TS `GoldenOp` / `runTsCase` 导出变更后，Part 2 B2 跑 `pnpm -r typecheck` 覆盖所有调用方（含测试文件）。
  - fixture 文件名 `l1-process-ops.json` 在 Rust `tests/golden.rs` 与 TS `l1-golden.test.ts` 中一致，consumer 通过同一路径读取。
- [ ] 6. Test-the-risk：每个状态/边界变更（进程启动、环境变量、输出缓冲、退出码、进程树 kill）都有行为断言；Part 1/2 的测试代码会显式给出断言与输入。
- [ ] 7. Type consistency：`Process` / `exec` / `exec_with_env` 签名在 Part 1 定义后不再变更；Part 2 的 fixture/harness 严格复用同一签名。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/apps/ody-code/src/cli (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core-shared/src/errors (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/compaction (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/config (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/context (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/injection (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission/policies (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/records (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/replay (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/session-mode (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/tool (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/turn (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/usage (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/profile (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/rpc (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/checkpoint (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/export (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/skill/builtin (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/collaboration (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/file (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/game-design (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/goal (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/idea (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/office-hours (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/planning (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/shell (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/state (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/visual (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/web (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/cron/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/fixtures (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/scenarios (priority: important)
- /Users/ranwei/workspace/ody-code/packages/node-sdk/scripts (priority: important)
- /Users/ranwei/workspace/ody-code/packages/node-sdk/src (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

