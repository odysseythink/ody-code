# 4.2.3 OpenAI Responses provider Implementation Plan

**Goal:** 在 `kosong-rs` 中迁移 TypeScript `packages/kosong/src/providers/openai-responses.ts` 的 OpenAI Responses API provider，使其请求构造、流式/非流式响应解析、reasoning summary、并行 function-call 路由与错误映射均与 TS 实现逐值一致，并通过 L1 golden 对照验证。

**Architecture:** 新增 `kosong-rs/src/openai_responses.rs`（含内部 `OpenAIResponsesStreamedMessage` 流适配器），复用 4.2.2/4.2.1 已落地的 `openai_common`（错误转换、`reasoning_effort` 映射）、`request_auth`（auth 解析与 header 合并）、`tool_call_id`（`sanitize_openai_responses_call_id`）、`capability_registry`（`uses_openai_responses_developer_role` / `get_openai_responses_model_capability`）与 `http_client`（HTTP 抽象）。HTTP 使用 `reqwest`，Responses API 的流对象是 JSON 对象异步序列（非 SSE），由 `openai_responses.rs` 自解析。对照沿用现有 kosong L1 golden 模式：新增 `kosong-responses-golden` 二进制与 TS runner `kosong-responses-golden.ts` 解析同一份 fixture，分别驱动 Rust provider 与 TS `OpenAIResponsesChatProvider`（通过 mock client），归一化后逐字段比较 `assistantMessage`。

**Tech Stack:** Rust 2021 / tokio / reqwest / serde / async-trait / futures-util；TypeScript / vitest / `@odysseythink/kosong`；JSON golden fixtures。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

新增/修改文件清单（按最终状态）：

| 路径 | 责任 |
|---|---|
| `rust-ody/crates/kosong-rs/Cargo.toml` | 增加 `kosong-responses-golden` binary |
| `rust-ody/crates/kosong-rs/src/lib.rs` | 声明并 re-export `openai_responses` 模块与类型 |
| `rust-ody/crates/kosong-rs/src/openai_responses.rs` | `OpenAIResponsesChatProvider`、请求构造、`OpenAIResponsesStreamedMessage`、错误映射 |
| `rust-ody/crates/kosong-rs/src/bin/responses_golden.rs` | `kosong-responses-golden` 二进制：解析 fixture，输出 `assistantMessage` / `error` |
| `packages/integration-tests/src/parity/kosong-responses-golden.ts` | TS golden runner：mock SDK client 调用 TS `OpenAIResponsesChatProvider` |
| `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-text.json` | 流式纯文本 golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-thinking.json` | 流式 reasoning summary golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-tool-call-single.json` | 流式单 function-call golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-tool-call-parallel.json` | 流式并行 function-call 路由 golden fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-incomplete.json` | `response.incomplete` + `max_output_tokens` finish reason fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-error.json` | error 事件 / response.failed / HTTP 错误 fixture |
| `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-nonstream.json` | 非流式 text + function-call fixture |
| `packages/integration-tests/test/parity/kosong/l1-responses-golden.test.ts` | vitest TS↔Rust 逐 fixture 比对 |
| `packages/integration-tests/package.json` | 增加 `test:parity:kosong:responses` script |

---

## Dependency Overview

```text
Phase A: Provider model & request construction
  Task 1: OpenAIResponsesOptions、provider 构造、ChatProvider trait 基础实现
  Task 2: Message → Response input item 转换（含 developer role、reasoning、tool result、audio）
  Task 3: Tool 转换与 request body 组装
  Task 4: generate() 网络调用骨架、auth header、abort 检查

Phase B: Response parsing
  Task 5: 非流式响应解析
  Task 6: 流式事件路由（output_text.delta、response.created/in_progress、completed/incomplete/failed、error）
  Task 7: 流式 function-call 解析与 final arguments suffix 校验
  Task 8: 流式 reasoning summary 解析

Phase C: L1 parity
  Task 9: lib.rs 导出与 `kosong-responses-golden` binary
  Task 10: TS golden runner
  Task 11: L1 golden fixtures
  Task 12: L1 parity 测试与 package script
```

**跨阶段约束**：
- Phase B 依赖 Phase A 的 provider 构造与请求调用骨架（`generate()` 返回 `StreamedMessage`）。
- Phase C 依赖 Phase B 的解析结果可通过 `generate()` 完整产出。
- L1 fixtures 只包含解析输入（响应对象/事件对象序列）或请求捕获输入，不依赖真实 OpenAI API。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| Responses API 的流是 JSON 对象序列（非 SSE），Rust 侧解析事件类型必须与 TS 逐事件一致 | fixture 使用事件对象数组，跳过 SDK 与 reqwest 的分帧差异，只比对适配器输出 |
| `response.id` 只在 `response.created/in_progress/completed` 捕获，不被 `item.id` 覆盖 | fixture 显式覆盖含 `item.id` 与 `response.id` 的场景，断言 Rust 侧 `stream.id()` |
| 并行 function-call 通过 `item_id`/`output_index` 路由，final arguments suffix 校验逻辑复杂 | fixture 覆盖交错 delta 与 `.done` 后缀不一致/一致两种情况 |
| reasoning summary 事件（`reasoning_summary_part.added` + `reasoning_summary_text.delta`）需拼接 think 内容 | fixture 覆盖多段 summary text 与 encrypted_content |
| `function_call_output` 中 audio_url 的 `mapAudioUrlToInputItem` 与 user message 路径共用但边界不同 | fixture 覆盖 tool result 含 audio data URI / http URL |
| tool-call-id 策略 `sanitizeOpenAIResponsesCallId(id, 64)` 处理 `\|` 分隔符 | 直接复用 `kosong-rs::tool_call_id::sanitize_openai_responses_call_id(..., Some(64))` |
| `developer` role 判定模型前缀表漂移 | 复用 `capability_registry::uses_openai_responses_developer_role` |

**已确认假设**：
- 4.2.0 的 `Message`/`Tool`/`StreamedMessagePart`/`FinishReason`/`ThinkingEffort`/`TokenUsage` 类型已稳定（见 `rust-ody/crates/kosong-rs/src/{message,provider,usage}.rs`）。
- 4.2.1 的 `request_auth::merge_request_headers` / `require_provider_api_key`、`tool_call_id::sanitize_openai_responses_call_id`、`capability_registry::uses_openai_responses_developer_role` / `get_openai_responses_model_capability` 已可用。
- 4.2.2 的 `openai_common::convert_openai_error` / `thinking_effort_to_reasoning_effort` / `reasoning_effort_to_thinking_effort` / `ToolMessageConversion` 已可用。

---

## Spec-coverage table

| 路线图 4.2.3 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.2.3.1 实现 `OpenAIResponsesChatProvider`（message → input items / developer role / function_call_output / request 构造） | Part 1 Task 1–4 | covered |
| 4.2.3.2 实现 stream 事件解析（output_text.delta / output_item.added\|done / function_call_arguments.delta\|done / reasoning_summary_* / response.completed\|incomplete\|failed / error） | Part 2 Task 6–8 | covered |
| 4.2.3.3 实现 non-stream 解析（output items → text/function_call/reasoning） | Part 2 Task 5 | covered |
| 4.2.3.4 tool-call-id 策略（`sanitizeOpenAIResponsesCallId(id, 64)`，处理 `\|` 分隔符） | Part 1 Task 1 | covered |
| 4.2.3.5 L1 SSE + non-stream fixture（reasoning summary、并行 function_call、incomplete、error 事件） | Part 3 Task 9–12 | covered |
| 门 G4-2-3 | Part 3 Task 12 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-2-3/model.md` | Provider model & request construction | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-2-3/response.md` | Response parsing | done |
| 3 | `2026-06-27-backend-architecture-evolution-phase4-2-3/parity.md` | L1 golden binary + TS runner + fixtures + test | done |
