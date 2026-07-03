# 4.2.0 kosong 共享数据模型 + generate 循环 Implementation Plan

**Goal:** 在 Rust 侧新建 `kosong-rs` crate，迁移 provider 无关的公共数据模型与 `generate` 循环，并通过 L1 golden 对照证明 Rust 实现与 TypeScript `packages/kosong/src` 逐值一致。

**Architecture:** `kosong-rs` 与 `kaos-rs` 并列，作为 `ody-host` 未来接入 LLM 层的纯算法 crate；本次只落地类型、`ChatProvider` trait、`generate()` 循环与错误分类，不碰 HTTP/SSE provider。公共类型与 TS 侧 `Message`/`ContentPart`/`ToolCall`/`StreamedMessagePart`/`FinishReason`/`ThinkingEffort`/`TokenUsage`/`GenerateResult`/`GenerateOptions` 一一对应；`generate()` 循环完全复刻 TS 的合并、并行 tool-call 路由、abort 检查、空响应/think-only 拒绝逻辑。对照方式沿用 `kaos-rs` 已验证的 golden fixture 模式：JSON fixture 同时喂给 TS `generate()` 与 Rust `kosong-golden` 二进制，归一化后深比较。

**Tech Stack:** Rust 2021 / tokio / serde / thiserror / async-trait；TypeScript / vitest；与 `kaos-rs` 共享的 golden harness 模式。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

新增/修改文件清单（按最终状态）：

| 路径 | 责任 |
|---|---|
| `rust-ody/Cargo.toml` | workspace members 加入 `kosong-rs` |
| `rust-ody/crates/kosong-rs/Cargo.toml` | crate 元数据 + 依赖 |
| `rust-ody/crates/kosong-rs/src/lib.rs` | crate 入口，暴露 `message`、`provider`、`generate`、`errors`、`usage` 模块 |
| `rust-ody/crates/kosong-rs/src/message.rs` | `Role`、`ContentPart`、`ToolCall`、`ToolCallPart`、`StreamedMessagePart`、`Message`、`merge_in_place`、类型判断 helper |
| `rust-ody/crates/kosong-rs/src/provider.rs` | `ThinkingEffort`、`FinishReason`、`TokenUsage`、`GenerateOptions`、`ProviderRequestAuth`、`ChatProvider` trait、`GenerateResult`、`GenerateCallbacks` |
| `rust-ody/crates/kosong-rs/src/errors.rs` | `ChatProviderError` 及其子错误、`is_retryable_generate_error`、`is_context_overflow_status_error` |
| `rust-ody/crates/kosong-rs/src/generate.rs` | `generate()` 循环 + `StreamedMessage` trait + abort 处理 + flush 逻辑 |
| `rust-ody/crates/kosong-rs/src/mock.rs` | 确定性 mock provider，按 fixture 中的 parts 序列产出 |
| `rust-ody/crates/kosong-rs/src/bin/golden.rs` | `kosong-golden` 二进制入口：解析 fixture、驱动 mock provider、输出 JSON |
| `packages/integration-tests/src/parity/kosong-golden.ts` | TS golden runner：解析同一 fixture、调用 TS `generate()`、输出同形结果 |
| `packages/integration-tests/src/parity/fixtures/kosong/l1-generate-text.json` | L1 文本合并 golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong/l1-tool-call-single.json` | L1 单 tool-call golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong/l1-tool-call-parallel.json` | L1 并行 tool-call golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong/l1-empty-rejection.json` | L1 空响应拒绝 fixture |
| `packages/integration-tests/src/parity/fixtures/kosong/l1-thinking-only-rejection.json` | L1 think-only 拒绝 fixture |
| `packages/integration-tests/test/parity/kosong/l1-golden.test.ts` | vitest 测试：TS↔Rust 逐 fixture 比对 |

---

## Dependency Overview

```text
Phase A: Foundation
  Task 1: kosong-rs crate 骨架
  Task 2: message 类型与 merge 规则
  Task 3: provider trait / GenerateOptions / GenerateResult
  Task 4: 错误分类

Phase B: Generate Loop
  Task 5: StreamedMessage trait + mock provider
  Task 6: merge_in_place 实现
  Task 7: generate() 主循环 + abort
  Task 8: tool-call 路由与 flush
  └─ 依赖 Phase A 全部

Phase C: Parity
  Task 9: kosong-golden 二进制 + fixture 格式
  Task 10: TS kosong-golden runner
  Task 11: L1 generate fixture
  Task 12: TS↔Rust parity 测试
  └─ 依赖 Phase B 全部
```

可并行度：Phase A 内部 Task 2/3/4 可并行，但建议串行以降低类型返工；Phase B 必须等 Phase A；Phase C 必须等 Phase B。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `generate()` 中并行 tool-call 的 index 路由与 TS 不一致 | fixture 必须覆盖单/多/交错 delta 三种情况 |
| think-only 拒绝的判定边界（`hasText` 要求 `trim().length > 0`） | fixture 显式覆盖空白 text、think+text、think+tool-call、纯 think |
| abort 时机：TS 在 `provider.generate()` 前后、每次 part 后、每次 callback 后检查 | Rust 必须逐点复刻，fixture 覆盖 mid-stream abort |
| `StreamedMessage` 在 Rust 中需要同时实现 `Stream` 又携带 `id/usage/finishReason` | 采用 struct + `Stream` impl 或 `async-stream`；计划中选择 struct + 手工 poll |
| 错误 message shape（尤其是空响应消息包含 provider/model 名） | fixture 中 message 用 `<provider>` / `<model>` 占位，归一化后比较 |

**Open Questions:** 无。4.2.0 的契约已由 TS 侧锁定，无需新增架构决策。

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-2-0/foundation.md` | crate 骨架 + 数据模型 + trait + 错误 | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-2-0/generate-loop.md` | `generate()` 循环 + mock provider | done |
| 3 | `2026-06-27-backend-architecture-evolution-phase4-2-0/parity.md` | golden harness + fixture + TS↔Rust 对照 | done |

---

## Spec-coverage table

| 路线图 4.2.0 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.2.0.1 新建 `kosong-rs` crate，加入 workspace | Task 1 | covered |
| 4.2.0.2 迁移归一化类型（Message/ContentPart/ToolCall/StreamedMessagePart/ThinkingEffort/FinishReason/TokenUsage/GenerateResult/GenerateOptions/ProviderType） | Task 2, Task 3 | covered |
| 4.2.0.3 实现 `generate()` 循环（abort、onMessagePart/onToolCall、tool-call 并行路由、mergeInPlace、空响应/think-only 拒绝） | Task 5–8 | covered |
| 4.2.0.4 定义 `ChatProvider` trait | Task 3, Task 5 | covered |
| 4.2.0.5 错误分类 | Task 4 | covered |
| 4.2.0.6 L1 generate 循环 golden fixture | Task 9–12 | covered |
