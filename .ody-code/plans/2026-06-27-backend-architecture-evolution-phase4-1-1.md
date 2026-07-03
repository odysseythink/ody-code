# Phase 4.1.4 CoreHost 集成 / RPC 暴露 / Bash 工具迁移 实施计划

**Goal:** 将已完成 L1 对照的 `kaos-rs` 接入 `ody-host`，通过内部 `env.*` RPC 暴露 kaos 操作，把 `BashTool` 从裸 `tokio::process` 切到 kaos，最终用 L2 parity scenario 证明 TS `LocalKaos` 与 Rust `kaos-rs`（经 CoreHost）逐字段等价，为 4.2 及后续模块提供可切换的 I/O 地基。

**Architecture:** `ody-host` 依赖 `kaos-rs` 并在 `CoreHost` 中持有 `Arc<Kaos>`（host 级实例，后续按 session workDir 派生）。`CoreHost.dispatch` 增加内部方法前缀 `env.`，把 `env.stat`/`env.glob`/`env.readText`/`env.writeText`/`env.exec` 路由到 `kaos-rs`。`BashTool` 改为持有 `Arc<Kaos>`，执行 `kaos.exec('bash', '-c', command)`。L2 对照复用 `packages/integration-tests/src/parity/` 的 `ParityDriver` 与 `backends.ts`，新增 TS 后端 `env.*` 适配器与 kaos 专用 scenario，逐字段比对 TS 与 Rust 的响应。

**Tech Stack:** Rust (tokio, serde_json, kaos-rs) / TypeScript / Vitest / `packages/integration-tests` / GitHub Actions。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Path | Responsibility |
|---|---|
| `rust-ody/crates/ody-host/Cargo.toml:21` | 加入 `kaos-rs` 依赖 |
| `rust-ody/crates/ody-host/src/host.rs:12` | `CoreHost` 增加 `kaos: Arc<Kaos>` 字段 |
| `rust-ody/crates/ody-host/src/host.rs:42` | `CoreHost::new` 构造 `Arc<Kaos>` 并传给 `BashTool` |
| `rust-ody/crates/ody-host/src/env.rs` | 内部 `env.*` RPC 请求/响应类型与分发函数 |
| `rust-ody/crates/ody-host/src/tools/bash.rs:5` | `BashTool` 持有 `Arc<Kaos>` 并用 kaos 执行命令 |
| `rust-ody/crates/ody-host/src/tools/mod.rs:49` | （如需要）Tool execute 上下文扩展；本计划优先保持 trait 不变，通过构造函数注入 Kaos |
| `packages/integration-tests/src/parity/backends.ts:31-148` | TS/Rust parity 后端增加 `envCall`，TS 侧直接调 `LocalKaos` |
| `packages/integration-tests/src/parity/scenarios/kaos-ops.ts` | kaos L2 scenario：stat/glob/readText/writeText/exec |
| `packages/integration-tests/test/parity/kaos/l2-rpc.test.ts` | TS-vs-Rust L2 对照测试 |
| `rust-ody/ts/bench.ts` | stat/glob/read 大目录基准脚本（方法论，不测真实 LLM） |
| `.github/workflows/rust-host.yml` | 新增 `ody-host` 编译、L2 parity job 与基准产出步骤 |

---

## Dependency Overview

```
Part 1: Host Integration
  T1 Add kaos-rs dependency to ody-host
    ▼
  T2 CoreHost holds Arc<Kaos>
    ▼
  T3 Define env.* RPC types
    ▼
  T4 Wire env.* dispatch in CoreHost
    ▼
  T5 Unit test env.* dispatch

Part 2: Kaos Operations via RPC
  T6 env.stat
    ▼
  T7 env.glob
    ▼
  T8 env.readText
    ▼
  T9 env.writeText
    ▼
  T10 env.exec

Part 3: Bash Migration + L2 Parity + Benchmark
  T12 Migrate BashTool to kaos
    ▼
  T13 TS backend env.* adapter
    ▼
  T14 L2 parity scenario
    ▼
  T15 Benchmark script
    ▼
  T16 CI wiring
    ▼
  T17 Whole-tree verification
```

并行规则：
- Part 1 内部串行：T2 依赖 T1；T3 依赖 T2；T4 依赖 T3；T5 依赖 T4。
- Part 2 内部：T6–T10 互不依赖，可并行开发，但建议按 stat → glob → readText → writeText → exec 顺序合入（便于逐步验证）。
- Part 3：T12 依赖 T2（CoreHost 有 Kaos）；T13 依赖 T6–T10 的 RPC 形态稳定；T14 依赖 T12 与 T13；T15 依赖 T14；T16 依赖 T12/T14/T15；T17 依赖 Part 3 全部前置任务。
- 跨 Part：Part 2 依赖 Part 1 的类型就位；Part 3 依赖 Part 1 与 Part 2 全部完成。

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| R1 | `CoreHost::new` 增加 `Arc<Kaos>` 字段/参数会扇出到 `main.rs` 与所有 host 单元测试 | T2 作为共享签名变更任务，同任务内更新所有调用方并跑 `cargo test --workspace` |
| R2 | `BashTool` 改用 `kaos.exec` 后，stdout/stderr 从 `String::from_utf8_lossy` 变为先取字节再解码，可能改变非法 UTF-8 的输出形状 | 保留与旧工具一致的 `String::from_utf8_lossy` 解码；L2 scenario 覆盖含非 UTF-8 的 echo 用例 |
| R3 | `env.*` 内部 RPC 与 `CoreAPI` 公共方法命名冲突 | 所有内部方法以 `env.` 前缀命名，`dispatch` 优先匹配完整前缀；文档明确标注为内部/测试用 |
| R4 | TS 后端 `env.*` 适配器需要绕过 `WorkerCoreAPI` 的公共 RPC 直接调 `LocalKaos`，可能破坏 parity 框架的「同一客户端」抽象 | 在 `ParityClientAPI` 中拦截 `env.*` 方法（`client.rpc` 调用前分发），保持 `SDKRpcClient` 不变 |
| R5 | L2 parity 在 Windows 与 POSIX 上路径/行尾/进程语义不同 | Normalizer 已处理路径与行尾；exec scenario 使用 `/bin/sh` 或 `node -e` 等跨平台命令，Windows 跳过 POSIX-specific kill 用例 |
| R6 | 基准脚本 `rust-ody/ts/bench.ts` 可能依赖未安装的包或 Node API | 仅使用 `node:fs`、`node:path`、`node:child_process` 与 `node:perf_hooks` 标准库，不引入外部依赖 |

---

## Spec-Coverage Table

| 设计 § | Requirement | 覆盖 Task(s) | 状态 |
|---|---|---|---|
| 4.1.4.1 | `ody-host` 依赖 `kaos-rs` | T1 | covered |
| 4.1.4.2 | `CoreHost` 持有 `Arc<Kaos>` | T2 | covered |
| 4.1.4.3 | 内部 RPC 暴露 kaos 操作（stat/glob/readText/writeText/exec） | T3, T4, T6–T10 | covered |
| 4.1.4.4 | `BashTool` 迁移到 kaos | T12 | covered |
| 4.1.4.5 | L2 parity scenario | T13, T14 | covered |
| 4.1.4.6 | 基准（stat/glob/read 大目录） | T15 | covered |
| G4-1 | `env.*` L2 对照绿 + BashTool 经 kaos 后现有 parity scenario 绿 | T14, T17 | covered |
| G4-1 | exec darwin+linux 对照绿 | T10, T14 | covered |
| CI | `cargo test --workspace` + L2 parity | T16 | covered |

---

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-1-1/host-integration.md` | ody-host 依赖、CoreHost Kaos 字段、env.* RPC 类型与分发 | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-1-1/kaos-ops.md` | env.stat / env.glob / env.readText / env.writeText / env.exec | done |
| 3 | `2026-06-27-backend-architecture-evolution-phase4-1-1/bash-parity.md` | BashTool 迁移、TS 后端适配器、L2 scenario、基准、CI | done |

---

## Global Self-Review

- [ ] 1. Spec-coverage table: 4.1.4 全部 6 个条目已映射到 T1–T17，无 GAP。
- [ ] 2. Placeholder scan: 索引无 TODO/TBD；具体实现代码与测试写在各 Part 文件中。
- [ ] 3. No phantom tasks: 每个 Task 均产生可编译/可运行的代码或 fixture；无 `--allow-empty` 或「已在 Task N 完成」的虚假任务。
- [ ] 4. Dependency soundness: Part 1→Part 2→Part 3；同 Part 内依赖均为前向；无反向依赖。
- [ ] 5. Caller & build soundness: T2 修改 `CoreHost` 字段但未改变 `CoreHost::new` 签名，`main.rs` 无需改动；T12 修改 `BashTool` 构造函数（共享签名变更），同任务内更新 `CoreHost::new` 中的注册与 `bash.rs` 单元测试，并以 `cargo check --workspace --tests` 收尾；T13 修改 `ParityBackend` 接口，同任务内更新 `makeTsBackend`/`makeRustBackend`，并以 `pnpm --filter @odysseythink/integration-tests typecheck` 验证；T17 以 `cargo test --workspace` 与 `pnpm -r typecheck` 收尾。
- [ ] 6. Test-the-risk: T2 断言 CoreHost 持有 Kaos；T6–T10 各自用 fixture/单元测试断言 RPC 返回值与文件系统状态；T12 断言 BashTool 执行命令返回 stdout/exitCode；T14 用 L2 parity 作为硬门；T17 作为全量回归硬门。
- [ ] 7. Type一致性: `Kaos`/`StatResult`/`Process` 等类型来自 4.1.0 的 `kaos-rs`；`env.*` 请求/响应类型在 T3 定义，T6–T14 直接复用，无重命名漂移。
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

