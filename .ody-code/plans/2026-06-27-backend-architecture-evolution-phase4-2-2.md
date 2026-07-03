# 4.2.2 OpenAI Chat Completions 共享解析 + OpenAI Legacy Implementation Plan

**Goal:** 在 `kosong-rs` 中落地 OpenAI Chat Completions 协议族的共享解析层，并实现 `OpenAILegacyChatProvider`，使其与 TypeScript `packages/kosong/src/providers/openai-legacy.ts` 在 L1 SSE 重放中逐字段一致，成为 4.2.3/4.2.5 的复用地基。

**Architecture:** 在 `kosong-rs` 内新增三个纯 Rust 模块：`openai_common`（通用转换/错误/usage/finish_reason 映射）、`chat_completions_stream`（SSE 流式 tool-call 缓冲与索引路由）、`openai_legacy`（`OpenAILegacyChatProvider` 实现 `ChatProvider` trait）。为把 HTTP 层与解析层解耦以便 L1 无网络测试，新增 `http_client.rs` 抽象，生产环境封装 `reqwest`，测试环境注入返回固定 SSE 字节的 mock。L1 对照沿用 `kosong-utils-golden` 模式：新 binary `kosong-openai-golden` 与 TS runner `kosong-openai-golden.ts` 解析同一份 fixture，分别驱动 Rust provider 与 TS `OpenAILegacyChatProvider`（通过 mock client），归一化后深比较。

**Tech Stack:** Rust 2021 / tokio / reqwest / serde / futures-util；TypeScript / vitest / `@odysseythink/kosong`；JSON + SSE golden fixtures。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| 路径 | 责任 |
|---|---|
| `rust-ody/crates/kosong-rs/Cargo.toml` | 增加 `reqwest` 依赖与 `kosong-openai-golden` binary |
| `rust-ody/crates/kosong-rs/src/lib.rs` | 声明 `http_client`、`openai_common`、`chat_completions_stream`、`openai_legacy` 模块并 re-export |
| `rust-ody/crates/kosong-rs/src/http_client.rs` | `HttpClient` trait、`HttpResponse`、`ReqwestClient`、`MockHttpClient` |
| `rust-ody/crates/kosong-rs/src/openai_common.rs` | `convert_content_part`、`tool_to_openai`、`extract_usage`、`normalize_openai_finish_reason`、`convert_openai_error`、thinking/reasoning 映射、`convert_tool_message_content` |
| `rust-ody/crates/kosong-rs/src/chat_completions_stream.rs` | SSE 解析、`BufferedChatCompletionToolCall`、`convert_chat_completion_stream_tool_call`、reasoning_content 扫描 |
| `rust-ody/crates/kosong-rs/src/openai_legacy.rs` | `OpenAILegacyChatProvider`、`OpenAIMessage` 转换、request 构造、`ChatProvider` 实现 |
| `rust-ody/crates/kosong-rs/src/bin/openai_golden.rs` | `kosong-openai-golden` 二进制：解析 fixture、驱动 provider、输出 JSON |
| `packages/integration-tests/src/parity/kosong-openai-golden.ts` | TS golden runner：用 mock OpenAI client 驱动 `OpenAILegacyChatProvider` |
| `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-text.json` | 纯文本 SSE fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-thinking.json` | thinking 字段 fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-tool-call-single.json` | 单 tool-call SSE fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-tool-call-parallel.json` | 并行 tool-calls 索引路由 fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-truncated.json` | `finish_reason=length` fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-usage.json` | `stream_options.include_usage` 末帧 fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-error.json` | HTTP 错误/status 错误 fixture |
| `packages/integration-tests/test/parity/kosong/l1-openai-golden.test.ts` | vitest 测试：编译 binary 后逐 fixture TS↔Rust 比对 |

---

## Dependency Overview

```text
Phase A: HTTP abstraction + openai-common
  Task 1: HttpClient trait + ReqwestClient + MockHttpClient
  Task 2: openai_common 转换/错误/usage/finish_reason 映射
  │
  ├──► Phase B: Stream parser
  │      Task 3: chat_completions_stream SSE 解析与 tool-call 路由
  │      │
  │      ▼ Phase C: Provider
  │      Task 4: OpenAILegacyChatProvider ChatProvider 实现
  │      │
  │      ▼ Phase D: Parity harness（详见 `parity.md` Task 1–6）
  │      parity.md Task 1: 导出 OpenAI Legacy 模块与类型
  │      parity.md Task 2: `kosong-openai-golden` Rust 金标 binary
  │      parity.md Task 3: TypeScript 金标 runner
  │      parity.md Task 4: L1 SSE/HTTP fixtures
  │      parity.md Task 5: L1 对位测试
  │      parity.md Task 6: 集成脚本与 CI
```

- **Phase A 内部 Task 1/2 可并行**，但建议串行；Task 2 的测试使用 Task 1 的 mock。
- **Phase B 依赖 Phase A**：SSE 解析结果类型来自 `message.rs`/`provider.rs`（已落地于 4.2.0），错误分类依赖 `openai_common`。
- **Phase C 依赖 Phase A+B**：provider 使用 `openai_common` 转换与 `chat_completions_stream` 解析。
- **Phase D 依赖 Phase C**：binary 与 runner 都要能构造 `OpenAILegacyChatProvider`。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `reasoning_content` 多 key 扫描顺序或空值处理与 TS 不一致 | fixture 覆盖 `reasoning_content`/`reasoning_details`/`reasoning` 与显式 `reasoningKey` |
| 并行 tool-call 索引路由差异（`_streamIndex` 保留/剥离） | fixture 覆盖交错 header 与 argument delta |
| `finish_reason` 映射（`'stop'→completed`、`'length'→truncated` 等） | fixture 覆盖每种映射及 null 情况 |
| `usage.cached_tokens` 解析（top-level vs `prompt_tokens_details`） | fixture 覆盖两种来源 |
| tool-role 消息 `extract_text` 与 content-part array 切换边界 | fixture 覆盖纯文本、多模态、未配置 conversion |
| HTTP 错误分类（network/timeout/status/context-overflow） | fixture 覆盖 reqwest 错误、HTTP status、context_overflow 模式 |
| SSE 字节格式（`data:` 前缀、`[DONE]`、空行、多行 chunk） | fixture 使用真实 `.sse` 字节并复用同一 helper 生成 |

**Open Questions:** 无。4.2.0/4.2.1 已锁定类型与工具函数，4.2.2 只需按 TS 实现机械迁移。

---

## Spec-coverage table

| 路线图 4.2.2 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.2.2.1 实现 `openai-common`（convertContentPart / toolToOpenAI / extractUsage / normalizeOpenAIFinishReason / convertOpenAIError / thinking↔reasoning_effort） | Task 2 | covered |
| 4.2.2.2 实现 `chat-completions-stream`（convertChatCompletionStreamToolCall + BufferedChatCompletionToolCall / index 路由） | Task 3 | covered |
| 4.2.2.3 实现 `OpenAILegacyChatProvider`（message 转换 / reasoningKey round-trip / toolMessageConversion / request 构造 / stream+non-stream 解析） | Task 4 | covered |
| 4.2.2.4 L1 SSE fixture（纯文本/thinking/单 tool-call/并行 tool-calls/截断/错误/usage） | `parity.md` Task 4, Task 5, Task 6 | covered |
| 门 G4-2-2 | `parity.md` Task 5, Task 6 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-2-2/core.md` | Rust `kosong-rs` HTTP 抽象 + openai-common + stream 解析 + OpenAILegacyChatProvider | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-2-2/parity.md` | golden binary + TS runner + fixtures + L1 测试 + CI | done |
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

