# 4.2.4 Anthropic provider Implementation Plan

**Goal:** 在 `kosong-rs` 中迁移 TypeScript `packages/kosong/src/providers/anthropic.ts` 的 Anthropic Messages API provider，使其请求构造、thinking 配置、流式/非流式响应解析与错误映射均与 TS 实现逐值一致，并通过 L1 golden 对照验证。

**Architecture:** 新增 `kosong-rs/src/providers/anthropic.rs`（含内部 `AnthropicStreamedMessage` 流适配器），依赖 4.2.0 的公共类型与 4.2.1 的 `request_auth` / `tool_call_id` / `capability_registry` 工具；HTTP 使用 `reqwest`，SSE 按 Anthropic 事件格式自解析。对照沿用现有 kosong L1 golden 模式：JSON fixture 同时喂给 TS `AnthropicChatProvider`（mock SDK client）与 Rust `kosong-anthropic-golden` 二进制，归一化后逐字段比较 `StreamedMessagePart` 序列、`id`、`usage`、`finishReason`。

**Tech Stack:** Rust 2021 / tokio / reqwest / serde / async-trait / httptest；TypeScript / vitest。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

新增/修改文件清单（按最终状态）：

| 路径 | 责任 |
|---|---|
| `rust-ody/crates/kosong-rs/Cargo.toml` | 加入 `reqwest`、`httptest` 依赖 |
| `rust-ody/crates/kosong-rs/src/lib.rs` | 暴露 `providers` 模块与 Anthropic 相关符号 |
| `rust-ody/crates/kosong-rs/src/providers/mod.rs` | provider 子模块入口 |
| `rust-ody/crates/kosong-rs/src/providers/anthropic.rs` | `AnthropicChatProvider`、请求构造、`AnthropicStreamedMessage`、错误映射 |
| `rust-ody/crates/kosong-rs/src/bin/anthropic_golden.rs` | `kosong-anthropic-golden` 二进制：解析 fixture，输出 parts / request body |
| `packages/integration-tests/src/parity/kosong-anthropic-golden.ts` | TS golden runner：mock SDK client 调用 TS `AnthropicChatProvider` |
| `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-text.json` | 流式纯文本 golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-thinking.json` | 流式 thinking + signature golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-tool-call.json` | 流式单 tool-call golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-parallel-tool-calls.json` | 流式并行 tool-call golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-nonstream-text-tool.json` | 非流式 text + tool_use golden fixture |
| `packages/integration-tests/test/parity/kosong/l1-anthropic.test.ts` | vitest TS↔Rust 逐 fixture 比对 |

---

## Dependency Overview

```text
Phase A: Provider model & thinking configuration
  Task 1: 加入 reqwest/httptest 依赖
  Task 2: Claude 版本解析与 max_tokens ceiling
  Task 3: adaptive/budget thinking 配置
  Task 4: AnthropicChatProvider 构造与 ChatProvider trait 基础实现

Phase B: Request construction
  Task 5: Message → Anthropic MessageParam 转换（含 system/think/image/tool_result/cache_control）
  Task 6: Tool 转换与 cache_control 注入
  Task 7: 请求体组装、auth header、generate() 网络调用骨架

Phase C: Response parsing & L1 parity
  Task 8: 非流式响应解析
  Task 9: SSE 流式事件解析
  Task 10: Anthropic SDK 错误映射
  Task 11: L1 golden fixtures + Rust 二进制
  Task 12: TS golden runner + TS↔Rust parity 测试
```

**跨阶段约束**：
- Phase B 依赖 Phase A 的 provider 构造与 thinking 状态。
- Phase C 依赖 Phase B 的请求调用骨架（`generate()` 返回 `StreamedMessage`）。
- L1 fixtures 只包含解析输入（事件/响应对象）或请求捕获输入，不依赖真实 Anthropic API。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| Anthropic SSE 含 `event:` 行与 `data:` 行，需与 TS SDK 解析结果逐事件一致 | fixture 使用事件对象数组而非原始 SSE 字节，跳过 SDK 与 reqwest 的 SSE 分帧差异，只比对适配器输出 |
| `cache_control` 注入位置影响 prompt caching 行为 | fixture 显式断言最后一条消息/最后一个 tool 的 `cache_control`，与 TS 测试对齐 |
| thinking 配置在 adaptive/budget/disabled 之间切换逻辑复杂 | 按 TS 测试矩阵逐项落地，每行测试一个模型名 + effort 组合 |
| tool-call-id 归一化已由 4.2.1 实现，但 Anthropic 64 字符截断策略需复用 | 直接调用 `kosong-rs::tool_call_id::sanitize_tool_call_id(..., Some(64))` |
| `reqwest` 默认代理会在本地 mock 测试时拦截请求 | 测试 client 使用 `Client::builder().no_proxy().build()`，与 `ody-host` 一致 |

**已确认假设**：
- 4.2.0 的 `Message`/`Tool`/`StreamedMessagePart`/`FinishReason`/`ThinkingEffort`/`TokenUsage` 类型已稳定（见 `rust-ody/crates/kosong-rs/src/{message,provider,usage}.rs`）。
- 4.2.1 的 `request_auth::resolve_auth_backed_client`、`tool_call_id::sanitize_tool_call_id`、`capability_registry::get_anthropic_model_capability` 已可用。

---

## Spec-coverage table

| 路线图条目 | 内容 | 覆盖任务 | 状态 |
|---|---|---|---|
| 4.2.4.1 | 实现 `AnthropicChatProvider` | Part 1 Task 4, Part 2 Task 1-4 | covered |
| 4.2.4.2 | 实现 stream 事件解析 | Part 3 Task 1-2 | covered |
| 4.2.4.3 | 实现 non-stream 解析 | Part 2 Task 4, Part 3 Task 2 | covered |
| 4.2.4.4 | thinking 配置 | Part 1 Task 2-3 | covered |
| 4.2.4.5 | L1 SSE + non-stream fixture | Part 3 Task 5-6 | covered |
| §4.2 共享约束 | 复用 4.2.1 工具层、错误类型对齐 | 贯穿 Phase A-C | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-2-4/model.md` | Provider model & thinking configuration | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-2-4/request.md` | Request construction | done |
| 3 | `2026-06-27-backend-architecture-evolution-phase4-2-4/response.md` | Response parsing & L1 parity | done |

---

## Global Self-Review

- [x] 1. Spec-coverage table: 每个路线图条目已映射到 Part/Task；无 GAP。
- [x] 2. Placeholder scan: 索引无 TODO/TBD；具体实现代码在 Part 文件中逐项展开；Part 2 的 501 占位已在 Part 3 Task 4 替换。
- [x] 3. No phantom tasks: 每个 Task 均有 Files + 测试/验证步骤 + commit。
- [x] 4. Dependency soundness: Phase A → B → C，Part 文件内 Task 顺序满足 Depends on；无向后依赖。
- [x] 5. Caller & build soundness: 本计划不修改既有共享签名；新增 `StreamedMessage::from_stream` 在同一 Task 内完成并搜索调用方；新增 `providers` 模块由 `lib.rs` 统一暴露。
- [x] 6. Test-the-risk: 每个状态转换/边界条件均有行为断言（详见 Part 文件）。
- [x] 7. Type consistency: 全部复用 4.2.0/4.2.1 已定义类型，新增 Anthropic 内部类型字段与 TS 对齐，不引入公共签名漂移。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

