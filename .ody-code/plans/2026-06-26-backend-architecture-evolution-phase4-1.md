# Phase 4.1.0 kaos-rs Crate 骨架 + 路径/环境操作 + L1 Golden 对照 实施计划

**Goal:** 在 `rust-ody/crates/kaos-rs` 中建立 `kaos` 执行环境的 Rust 骨架，实现实例级 `cwd`、路径计算、环境探测与三个纯函数 helper（`decodeTextWithErrors`、`globPatternToRegex`、`BufferedReadable` 缓冲语义），并通过 L1 golden fixture 证明 Rust 输出与 TS `LocalKaos` 逐字段一致，为 4.1.1–4.1.4 提供可切换地基。

**Architecture:** `kaos-rs` 作为独立 crate 加入 Rust workspace，对外暴露 `Kaos` struct（携带 `cwd` 与 `Environment`）和纯函数模块；`ody-host` 暂不接入，仅在 4.1.4 依赖它。L1 对照复用 `packages/integration-tests/src/parity/` 的现有 harness：TS 侧直接调用 `LocalKaos`，Rust 侧通过 `kaos-rs` 的单元测试/golden binary 解析同一份 JSON fixture，最终由 parity normalizer + `assertParity` 做结构化 diff。

**Tech Stack:** Rust (tokio::fs/tokio::process 未启用，本阶段仅 std::path + 少量同步 I/O)、TypeScript / Vitest、`packages/integration-tests`、pathe、GitHub Actions。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Path | Responsibility |
|---|---|
| `rust-ody/crates/kaos-rs/Cargo.toml` | crate 元数据、依赖、workspace 注册 |
| `rust-ody/crates/kaos-rs/src/lib.rs` | 模块导出与 crate 根 |
| `rust-ody/crates/kaos-rs/src/kaos.rs` | `Kaos` struct、`with_cwd`、`chdir`、`getcwd` |
| `rust-ody/crates/kaos-rs/src/path.rs` | `path_class`、`normpath`、`gethome` |
| `rust-ody/crates/kaos-rs/src/environment.rs` | `Environment`、`detect_environment`、Windows Git Bash 定位 |
| `rust-ody/crates/kaos-rs/src/text.rs` | `decode_text_with_errors`（UTF-8/UTF-16LE strict/replace/ignore） |
| `rust-ody/crates/kaos-rs/src/glob.rs` | `glob_pattern_to_regex` |
| `rust-ody/crates/kaos-rs/src/buffered.rs` | `BufferedReader` 流缓冲语义 |
| `rust-ody/crates/kaos-rs/tests/golden.rs` | L1 golden 入口：读取 fixture 并驱动 kaos-rs |
| `packages/integration-tests/src/parity/fixtures/kaos/paths.json` | 路径/环境 golden 输入与预期 |
| `packages/integration-tests/src/parity/fixtures/kaos/text-decode.json` | decodeText 错误模式 golden |
| `packages/integration-tests/src/parity/fixtures/kaos/glob-patterns.json` | glob pattern → regex golden |
| `packages/integration-tests/src/parity/kaos-golden.ts` | TS 侧 golden fixture 解析与执行 |
| `packages/integration-tests/test/parity/kaos/l1-golden.test.ts` | TS-vs-Rust L1 对照测试 |
| `rust-ody/Cargo.toml:2` | workspace members 加入 `crates/kaos-rs` |
| `.github/workflows/rust-host.yml` | 新增 `cargo test -p kaos-rs` 与 L1 fixture artifact 步骤 |

---

## Dependency Overview

```
Part 1: Crate Foundation + Path/Environment
  T1 Create kaos-rs crate and register workspace
    ▼
  T2 Kaos struct + cwd semantics + path operations
    ▼
  T3 Environment detection

Part 2: Shared Pure Helpers
  T4 decodeTextWithErrors
    ▼
  T5 globPatternToRegex
    ▼
  T6 BufferedReadable buffering semantics

Part 3: L1 Golden Integration
  T7 Create L1 golden fixtures (paths + text + glob)
    ▼
  T8 Build TS-vs-Rust golden harness
    ▼
  T9 Run L1 gate and wire CI
```

并行规则：
- Part 1 内部串行：T2 依赖 T1；T3 依赖 T1，可与 T2 并行，但建议 T2 先合入以稳定 `Kaos` 边界。
- Part 2 内部：T4/T5/T6 互不依赖，可并行开发，但建议按 T4→T5→T6 顺序合入（难度递增）。
- Part 3：T7 依赖 Part 1/2 的类型函数就位；T8 依赖 T7；T9 依赖 T8。
- 跨 Part：Part 3 依赖 Part 1 与 Part 2 全部完成。

---

## Risks & Open Questions

| # | Risk | Mitigation |
|---|---|---|
| R1 | Rust `std::path` 的 `normalize` 语义与 Node `pathe.normalize` 在盘符、`//`、`..` 出界等边界不一致 | 为每个边界建独立 fixture case，L1 失败即修正 |
| R2 | `decodeTextWithErrors` 的 `ignore` 模式需保留有效 U+FFFD，Rust 标准库 `String::from_utf8_lossy` 会替换所有非法序列为 �，无法区分 | 手写 UTF-8/UTF-16LE ignore 解码器，对照 TS `decodeUtf8Ignore` 逐字节 |
| R3 | `globPatternToRegex` 转义表与 TS 不一致（尤其是 `^` 在字符类首位的处理） | fixture 显式覆盖 `[^a]`、`[!a]`、`[a^]`、`\*` 等边界 |
| R4 | `BufferedReadable` 的“source 结束后仍可读取”语义在 Rust 中需额外封装 | 用 `tokio::io::AsyncBufRead` + 内部缓冲实现，单测断言 wait-before-read |
| R5 | 路径 fixture 含平台相关路径（Windows 盘符、Git Bash）导致 darwin/linux CI 失败 | fixture 按 `pathClass` 分组，非当前平台 case skip；Normalizer 后续再统一归一 |
| R6 | 本阶段不接入 `ody-host`，存在「crate 绿但 host 无法编译」风险 | T1 在 workspace 注册后立即 `cargo check --workspace`；T9 再跑一次全 workspace `cargo test` |

---

## Spec-Coverage Table

| 设计 § | Requirement | 覆盖 Task(s) | 状态 |
|---|---|---|---|
| 4.1.0.1 | 新建 `kaos-rs` crate 并加入 workspace | T1 | covered |
| 4.1.0.2 | `Kaos` struct 与实例级 `cwd` | T2 | covered |
| 4.1.0.3 | `pathClass / normpath / gethome / getcwd` | T2 | covered |
| 4.1.0.4 | 环境探测（平台/架构/shell/Windows Git Bash） | T3 | covered |
| 4.1.0.5 | `decodeTextWithErrors` | T4 | covered |
| 4.1.0.5 | `globPatternToRegex` | T5 | covered |
| 4.1.0.5 | `BufferedReadable` 缓冲语义 | T6 | covered |
| 4.1.0.6 | L1 golden 路径 fixture | T7 | covered |
| 4.1.0.6 | L1 golden text-decode fixture | T7 | covered |
| 4.1.0.6 | L1 golden glob-pattern fixture | T7 | covered |
| L1 对照 | TS-vs-Rust golden harness | T8 | covered |
| G4-1-0 | 全 L1 fixture 绿 + crate 编译 | T9 | covered |
| CI | `cargo test -p kaos-rs` + parity L1 | T9 | covered |

---

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-26-backend-architecture-evolution-phase4-1/foundation.md` | Crate 创建 + `Kaos` struct + 路径/环境操作 | done |
| 2 | `2026-06-26-backend-architecture-evolution-phase4-1/helpers.md` | `decodeTextWithErrors` + `globPatternToRegex` + `BufferedReadable` | done |
| 3 | `2026-06-26-backend-architecture-evolution-phase4-1/integration.md` | L1 golden fixtures + TS-vs-Rust harness + CI | done |

---

## Global Self-Review

- [ ] 1. Spec-coverage table: 4.1.0 全部 6 个条目（含 helper 拆分）已映射到 T1–T9，无 GAP。
- [ ] 2. Placeholder scan: 索引无 TODO/TBD；具体实现代码与测试写在各 Part 文件中。
- [ ] 3. No phantom tasks: 每个 Task 均产生可编译/可运行的代码或 fixture；无 `--allow-empty`。
- [ ] 4. Dependency soundness: Part 1→Part 2→Part 3；同 Part 内依赖均为前向；无反向依赖。
- [ ] 5. Caller & build soundness: 本阶段不修改 TS 共享签名；T1 修改 `rust-ody/Cargo.toml` workspace members，T9 以 `cargo check --workspace` 验证无破坏。
- [ ] 6. Test-the-risk: 状态变更类 Task（T2 cwd、T3 环境探测、T4 decode、T5 glob、T6 buffered、T7 fixture）均含行为断言；T9 以 L1 绿作为硬门。
- [ ] 7. Type consistency: `Kaos`/`Environment`/helper 签名在 T1–T6 定义，T7–T9 的 fixture/harness 直接复用，无重命名漂移。
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

