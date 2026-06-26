# Phase 1-A Wasm 高确定性计算热点 Implementation Plan

**Goal:** 将 tokenizer、diff/patch、glob 三个高确定性计算热点迁移到 Rust/Wasm，通过统一的双轨加载框架在 agent-core 中静默接入，并用 golden parity 测试与 G1-A 收益基准报告验证正确性与性能。

**Architecture:** Rust 侧在 `rust-ody` 单 crate 中以 `cdylib` 编译到 `wasm32-unknown-unknown`，保留 PoC 的 raw ABI（`alloc`/`dealloc` + 字符串指针）。TypeScript 侧在 `packages/agent-core/src/utils/wasm-*.ts` 实现统一双轨加载器：按 feature flag 决定是否加载 Wasm，加载/运行失败时静默降级到原有 JS 实现。每个 Wasm 化函数必须与原 TS 实现逐值一致，并通过 `rust-ody/ts/bench-phase1a.ts` 输出 G1-A 收益报告。

**Tech Stack:** Rust 2021 + `tiktoken-rs` / `similar` / `globset` → Wasm (`wasm32-unknown-unknown`), TypeScript 6.0 / Node.js ≥24.15 / Vitest, pnpm 10.33.0, cargo.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Path | Responsibility |
|---|---|
| `rust-ody/Cargo.toml` | Rust crate 依赖：新增 `tiktoken-rs`、`similar`、`globset` |
| `rust-ody/src/lib.rs` | Wasm 导出：保留 `alloc`/`dealloc`，新增 `count_tokens`、`compute_diff`、`format_git_diff`、`glob_match` |
| `packages/agent-core/src/flags/registry.ts` | 新增 `wasm-tokenizer`、`wasm-diff`、`wasm-glob` 实验 flag |
| `packages/agent-core/src/utils/wasm-loader.ts` | 通用双轨加载器 `loadWasmModule<T>`、flag 判断、降级包装 |
| `packages/agent-core/src/utils/wasm-string.ts` | 通用 raw-ABI 字符串调用封装 `callWasmStringFunction`、`callWasmU32Function` |
| `packages/agent-core/src/utils/wasm-tokenizer.ts` | Tokenizer 专用 Wasm 工厂与 JS fallback |
| `packages/agent-core/src/utils/wasm-diff.ts` | Diff 专用 Wasm 工厂与 JS fallback（`computeTextDiff`、`formatGitDiff`） |
| `packages/agent-core/src/utils/wasm-glob.ts` | Glob 专用 Wasm 工厂与 JS fallback |
| `packages/agent-core/src/utils/tokens.ts` | 新增 `initTokenizerWasm()`；`estimateTokens` 优先使用 Wasm |
| `packages/agent-core/src/code-review/diff.ts` | 新增 `computeTextDiff`、`formatGitDiff`；`fetchDiff` 返回前调用 `formatGitDiff` |
| `packages/agent-core/src/tools/support/path-glob-match.ts` | 新增 `initGlobWasm()`；`globMatch`/`pathGlobMatch` 优先使用 Wasm |
| `rust-ody/ts/bench-phase1a.ts` | 合成 + 真实样本基准，输出控制台与 Markdown 报告 |
| `packages/agent-core/test/utils/tokens-wasm-parity.test.ts` | Tokenizer golden parity 与 flag/降级测试 |
| `packages/agent-core/test/code-review/diff-wasm-parity.test.ts` | Diff golden parity 与降级测试 |
| `packages/agent-core/test/tools/support/glob-wasm-parity.test.ts` | Glob golden parity 与降级测试 |

---

## Dependency Overview

```text
Part 1: Rust/Wasm Foundation
  ├─ Task 1: Cargo dependencies
  ├─ Task 2: Shared Rust string/memory helpers
  └─ Task 3: Verify wasm32 build

Part 2: Wasm Loader Framework (agent-core)
  ├─ Task 4: Feature flags
  ├─ Task 5: Generic dual-track loader (depends on Task 4)
  └─ Task 6: Shared TS string-call helper (depends on Task 5)

Part 3: Tokenizer  ── depends on Part 1 + Part 2
  ├─ Task 7: Rust count_tokens
  ├─ Task 8: TS tokenizer integration (adds callWasmU32Function)
  └─ Task 9: Tokenizer parity tests

Part 4: Diff  ── depends on Part 1 + Part 2
  ├─ Task 10: Rust compute_diff + format_git_diff
  ├─ Task 11: TS diff integration
  └─ Task 12: Diff parity tests

Part 5: Glob  ── depends on Part 1 + Part 2 + Part 3 (Task 8 adds callWasmU32Function)
  ├─ Task 13: Rust glob_match
  ├─ Task 14: TS glob integration
  └─ Task 15: Glob parity tests

Part 6: Benchmark & Final Verification  ── depends on Part 3/4/5
  ├─ Task 16: Benchmark harness + report generator
  ├─ Task 17: Agent init wiring
  └─ Task 18: Whole-tree verification
```

Parts 3 and 4 can be developed in parallel after Parts 1 and 2 are complete. Part 5 must wait until Part 3 Task 8 has added the shared `callWasmU32Function` helper.

---

## Risks & Open Questions

| Risk | Mitigation in Plan |
|---|---|
| R1: `tiktoken-rs` 编译后 Wasm 体积 >2MB | Task 1 后立即执行 Task 3 的体积检查；若超过阈值，Task 7 改为运行时加载 rank JSON 的降级方案 |
| R2: `globset` 与 `picomatch` 语义不可调和 | Task 15 用现有 `path-glob-match.ts` 全部语义做 parity；失败则在 Task 14 中禁用 `wasm-glob` |
| R3: `format_git_diff` Rust 解析器难以保证逐字节一致 | Task 12 中若 parity 失败，保留 `formatGitDiff` 的 JS fallback（identity / 最小清洗），Wasm 路径仅作为可选优化 |
| R4: CI 缺少 Rust wasm target | Task 3 验证 `rustup target add wasm32-unknown-unknown`；Task 18 前检查 flake.nix/CI |
| R5: 小输入下 Wasm 边界税抵消收益 | Task 16 测量多尺寸；Task 18 按 <2% 总延迟门控 |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-24-backend-architecture-evolution-phase1-a-1/rust-wasm-foundation.md` | Rust crate 依赖与共享 ABI | done |
| 2 | `2026-06-24-backend-architecture-evolution-phase1-a-1/wasm-loader-framework.md` | agent-core 双轨加载框架 | done |
| 3 | `2026-06-24-backend-architecture-evolution-phase1-a-1/tokenizer.md` | Tokenizer Wasm 化 | done |
| 4 | `2026-06-24-backend-architecture-evolution-phase1-a-1/diff.md` | Diff Wasm 化 | done |
| 5 | `2026-06-24-backend-architecture-evolution-phase1-a-1/glob.md` | Glob Wasm 化 | done |
| 6 | `2026-06-24-backend-architecture-evolution-phase1-a-1/benchmark-and-verify.md` | 基准报告与最终验证 | done |

---

## Spec-Coverage Table

| 设计章节/需求 | 覆盖方式 | 状态 |
|---|---|---|
| 真 BPE tokenizer (`cl100k_base`, `o200k_base` 等) | Part 3 Task 7-9 | covered |
| diff/patch 纯计算 (`compute_unified_diff`, `format_git_diff`) | Part 4 Task 10-12 | covered |
| glob/路径匹配 (复刻 picomatch) | Part 5 Task 13-15 | covered |
| 统一双轨加载框架 + flag 禁用 | Part 2 Task 4-6 | covered |
| Golden parity 测试 | Part 3/4/5 各 Task N (parity tests) | covered |
| G1-A 收益基准报告 | Part 6 Task 16-18 | covered |
| 启动流程调用 `initTokenizerWasm` / `initGlobWasm` | Part 6 Task 17 | covered |
| Out of Scope: 1-B/1-C/2-E tokenizer/diff/glob 以外模块 | — | no-op |

---

## Self-Review

- [x] 1. Spec-coverage table: 每一条设计 In-Scope 需求都已映射到 Part/Task，无 GAP。
- [x] 2. Placeholder scan: 所有 Part 文件中无 `TODO`/`TBD`/"implement later"/"add appropriate error handling" 等占位；每个任务给出完整代码、命令与预期输出。
- [x] 3. No phantom tasks: 每个 Task 都有明确的 Create/Modify/Test 文件、可运行的测试或手动验证步骤、以及 commit 动作；无 `--allow-empty` 或 "already done in Task N"。
- [x] 4. Dependency soundness: 每个 `Depends on:` 都指向更早的 Task/Part；Part 3/4 仅依赖已完成的 Part 1/2；Part 5 依赖 Part 3 Task 8 的 `callWasmU32Function`；Part 6 仅依赖已完成的 Part 3/4/5。
- [x] 5. Caller & build soundness: 共享签名变更（`estimateTokens` 内部状态、`fetchDiff` 返回前调用 `formatGitDiff`、`globMatch` 内部状态、`FLAG_DEFINITIONS` 类型）在对应 Task 中搜索并更新所有调用方（含测试），并以 `pnpm typecheck` 全树检查收尾。
- [x] 6. Test-the-risk: 每个 Wasm 化函数有 golden parity 测试（must-pass）；每个模块有 flag 禁用测试与 Wasm 失败降级测试；glob 测试包含 picomatch 关键语义（`*` 不跨 `/`、`{}` 展开、nocase）。
- [x] 7. Type一致性: `WasmModuleConfig`、`callWasmStringFunction`、`callWasmU32Function`、`count_tokens` ABI、`glob_match` options 字符串（`"true"`/`"false"`）等类型与命名在 Part 2/3 定义后，Part 4/5 严格复用，无重命名漂移。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
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
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/code-review (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/flags (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/mcp (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/rpc (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/checkpoint (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/export (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session (priority: important)
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
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/providers/web-search (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/support (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/utils (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/cron/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/harness (priority: important)
- /Users/ranwei/workspace/ody-code/rust-ody/ts (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

