# 4.2.7 CoreHost Provider Factory + L2/L3 Gates Implementation Plan

**Goal:** 在 `kosong-rs` 中实现按 `provider_id`/`model`/`base_url`/`api_key` 构造任意 ChatProvider 的工厂，让 `ody-host` 的 `CoreHost` 能根据会话 `modelAlias` 切换 provider，并令 `getConfig`/`getOdyConfig` 返回与 TypeScript 后端逐字段一致的 provider 信息与 capability，最终通过 L2/L3 对照与性能基准门。

**Architecture:** 新增 `kosong-rs::provider_factory` 模块，把 4.2.0–4.2.6 落地的各 provider 统一封装成 `create_chat_provider(config)`；`ody-host` 不再直接持有 `OpenAiProvider`，而是基于 `HostConfig` 与会话状态通过工厂构建 `Box<dyn ChatProvider>`，并新增 `ChatProviderLlmAdapter` 把 kosong 的 `ChatProvider` 流式输出转回 `ody-host` 内部已有的 `LlmProvider` 事件接口，从而最小化改动上层 turn 逻辑。`getConfig`/`getOdyConfig` 使用 `kosong-rs::catalog` 与 `capability_registry` 解析模型 capability。L2/L3 对照复用 `packages/integration-tests/src/parity/` 已有 harness，新增 `host-config` 与 `multi-turn-tool` scenario，并在 CI 中加入 `parity` job。

**Tech Stack:** Rust 2021 / tokio / reqwest / serde / async-trait；TypeScript / vitest；GitHub Actions。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

最终新增/修改文件清单（按子系统分组）：

| 路径 | 责任 |
|---|---|
| `rust-ody/crates/kosong-rs/src/provider_factory.rs` | `ProviderFactoryConfig`、`create_chat_provider`、`resolve_model_capability` |
| `rust-ody/crates/kosong-rs/src/lib.rs` | 暴露 `provider_factory` 模块与相关符号 |
| `rust-ody/crates/ody-host/src/provider_factory.rs` | `HostConfig` → `Box<dyn ChatProvider>` 的 host 侧封装 |
| `rust-ody/crates/ody-host/src/llm/chat_provider_adapter.rs` | `ChatProvider` → `LlmProvider` 的流式适配器 |
| `rust-ody/crates/ody-host/src/llm/mod.rs` | 暴露 `chat_provider_adapter` 子模块 |
| `rust-ody/crates/ody-host/src/config.rs` | 解析 `provider_id`、`model_alias`；不再硬编码 `openai` |
| `rust-ody/crates/ody-host/src/session/store.rs` | `SessionState` 增加 `provider_id` 字段 |
| `rust-ody/crates/ody-host/src/session/manager.rs` | `create_with_id` 初始化 `provider_id`；提供 `set_provider_id` |
| `rust-ody/crates/ody-host/src/host.rs` | `set_model`/`get_agent_config`/`get_ody_config` 接入工厂与 capability |
| `rust-ody/crates/ody-host/src/main.rs` | 启动时用工厂根据 `HostConfig` 构造 provider |
| `packages/integration-tests/src/parity/scenarios/host-config.ts` | L2 scenario：`setModel`/`getConfig`/`getOdyConfig` 对照 |
| `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts` | L3 scenario：mock provider 多轮 tool-call 事件流对照 |
| `packages/integration-tests/src/parity/scenarios/index.ts` | scenario 注册表 |
| `packages/integration-tests/src/parity/assert-parity.ts` | 归一化 `null`/`undefined` 差异 |
| `packages/integration-tests/src/parity/known-gaps.md` | 更新已修复/新增 gap |
| `packages/integration-tests/src/parity/benchmark.ts` | TTFB/throughput benchmark 脚本 |
| `packages/integration-tests/test/parity/benchmark.test.ts` | benchmark 回归测试 |
| `packages/integration-tests/test/parity/ts-vs-rust.test.ts` | 注册新 scenario |
| `.github/workflows/rust-host.yml` | 新增 benchmark step |

---

## Dependency Overview

```text
Phase A: kosong-rs provider factory
  Task 1: 定义 ProviderFactoryConfig 与工厂错误类型
  Task 2: 实现 create_chat_provider 分支（openai/kimi/anthropic/deepseek/glm/google-genai/openai_responses/vertexai/mock）
  Task 3: 实现 resolve_model_capability 与工厂单元测试

Phase B: ody-host provider 路由
  Task 4: 扩展 HostConfig/ProviderConfig 解析 provider_id 与 model_alias
  Task 5: 扩展 SessionState 持久化 provider_id
  Task 6: 实现 ChatProviderLlmAdapter（ChatProvider → LlmProvider）
  Task 7: 实现 ody-host provider_factory 封装
  Task 8: CoreHost set_model 路由与 getConfig/getOdyConfig capability 返回
  Task 9: main.rs 接入工厂；全树 typecheck

Phase C: L2/L3 parity + benchmark
  Task 10: 新增 host-config L2 scenario 与测试
  Task 11: 新增 multi-turn-tool L3 scenario 与测试
  Task 12: 新增流式 TTFB/throughput benchmark 与 CI parity job
```

**跨阶段约束**：
- Phase B 依赖 Phase A 的 `create_chat_provider` / `resolve_model_capability`。
- Phase C 依赖 Phase B 的 `set_model`/`getConfig` 行为与 mock provider 输出对齐。
- Phase B 中所有修改 `SessionState` / `ProviderConfig` / `CoreHost::new` 签名的任务必须在本阶段内一次性更新所有调用方（含测试）并以全树 typecheck 收尾。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `ody-host` 当前用 `LlmProvider` 接口，而 `kosong-rs` 用 `ChatProvider`；直接替换会扇出所有调用方 | 新增 `ChatProviderLlmAdapter` 做桥接，保持 `LlmProvider` 接口不变 |
| Rust 侧没有 TS 的完整 `models`/`providers` catalog，无法像 TS 那样通过 alias 解析 provider | 4.2.7 先支持 `modelAlias` 为 `"provider/model"` 或 `"provider:model"` 的显式前缀语法；纯模型名回退到 `HostConfig.provider.provider_id`。 catalog 解析留到 4.3.2 |
| `getConfig` 返回的 `modelCapabilities` 若用 catalog 解析，可能因 catalog 缺失而与 TS 不一致 | 无 catalog 命中时回退到 `UNKNOWN_CAPABILITY`，与 TS `UNKNOWN_CAPABILITY` 字段值一致；L2 fixture 使用已知 catalog 条目 |
| mock provider 在 TS 与 Rust 的事件流形状可能不同 | L3 scenario 使用 Kosong `MockProvider` 固定 parts 序列，TS 侧通过 `llmFactory` 注入等价的 `MockProvider` |
| 多 provider 构建依赖 `reqwest` / `httptest` 已在 workspace 中 | 任务中显式检查 `Cargo.toml` 依赖，不重复引入 |

**已确认假设**：
- 4.2.0–4.2.6 已在 `kosong-rs` 落地 `OpenAILegacyChatProvider`、`OpenAIResponsesChatProvider`、`KimiChatProvider`、`DeepSeekChatProvider`、`GLMChatProvider`、`GoogleGenAIChatProvider`、`AnthropicChatProvider` 及 `MockProvider`。
- `kosong-rs::capability_registry` 与 `catalog` 已能按模型名解析 `ModelCapability`（命中已知模型时）。
- `packages/integration-tests/src/parity/` harness 已支持 TS↔Rust backend 的 scenario 驱动与事件收集。

---

## Spec-coverage table

| 路线图条目 | 内容 | 覆盖任务 | 状态 |
|---|---|---|---|
| 4.2.7.1 | `kosong-rs` provider factory | Part 1 Task 1-3 | covered |
| 4.2.7.2 | `CoreHost` provider 可切换 | Part 2 Task 4-8 | covered |
| 4.2.7.3 | `getConfig`/`getOdyConfig` provider 信息 | Part 2 Task 8 | covered |
| 4.2.7.4 | L2 对照 | Part 3 Task 10 | covered |
| 4.2.7.5 | L3 对照 | Part 3 Task 11 | covered |
| 4.2.7.6 | 性能基准 | Part 3 Task 12 | covered |
| §4.2 共享约束 | No-Go 信号：单 provider 无法复刻则保留 TS | Part 2 Task 7/8 提供 `unsupported` 分支；Part 3 Task 11 验证 mock 语义 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-2-7/kosong.md` | kosong-rs provider factory | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-2-7/host.md` | ody-host config/routing | done |
| 3 | `2026-06-27-backend-architecture-evolution-phase4-2-7/parity.md` | L2/L3 parity + benchmarks | done |


---

## Global Self-Review

- [ ] 1. Spec-coverage table：索引中的表格已覆盖 4.2.7.1–4.2.7.6 及 §4.2 No-Go 约束；无 GAP。
- [ ] 2. Placeholder scan：Part 1/2/3 中无 TODO/TBD/"实现 later"；每个 task 均给出完整代码、命令与预期输出。
- [ ] 3. No phantom tasks：每个 task 均产生可验证变更（新增/修改文件 + 测试命令 + commit）；无 `--allow-empty` 或 "已在 Task N 完成"。
- [ ] 4. Dependency soundness：
  - Part 2 Task 4 依赖 Part 1 Task 3（kosong factory）。
  - Part 2 Task 6 依赖 Part 1 Task 3。
  - Part 2 Task 7 依赖 Part 2 Task 6。
  - Part 2 Task 8 依赖 Part 2 Task 5/7。
  - Part 2 Task 9 依赖 Part 2 Task 7/8。
  - Part 3 Task 10 依赖 Part 2 Task 8。
  - Part 3 Task 11 依赖 Part 3 Task 10。
  - Part 3 Task 12 依赖 Part 3 Task 11。
  无向后依赖，无引用未定义符号。
- [ ] 5. Caller & build soundness：
  - Part 2 Task 5 修改 `SessionState`，同 task 内更新 `store.rs` 测试与 `manager.rs` 构造位置。
  - Part 2 Task 6 若新增 `LlmError` 变体，同 task 内搜索 `LlmError::` 并更新所有 match 分支；Task 9 以 `cargo check --workspace` 收尾。
  - Part 3 Task 11 修改 `assert-parity.ts`，同 task 内以 `pnpm -r typecheck` 验证（命令在 Part 3 Task 11 中给出）。
- [ ] 6. Test-the-risk：
  - `set_model` 状态变更：Part 2 Task 8 `provider_routing_tests` 行为断言 provider/model 均更新。
  - `HostConfig` 解析：Part 2 Task 4 `provider_config_tests` 覆盖 CLI/文件/默认值。
  - `SessionState` 持久化：Part 2 Task 5 `state_json_roundtrip_with_provider_id` 覆盖。
  - provider factory 分支选择：Part 1 Task 2 测试覆盖 mock/openai/unknown。
  - capability 解析：Part 1 Task 3 测试覆盖 known/unknown/unsupported。
- [ ] 7. Type consistency：
  - `ProviderFactoryConfig` 的 `provider_id`/`model` 与 `ody-host` `ProviderConfig` 及 `HostConfig` 解析字段一致。
  - `resolve_model_capability` 返回 `Option<ModelCapability>`，`get_agent_config` 用 `UNKNOWN_CAPABILITY` 回退，字段名与 TS `ModelCapability` 一致。
  - `get_agent_config` 返回 JSON 的 `provider.id`/`model`/`modelAlias`/`modelCapabilities`/`thinkingLevel` 与 TS `AgentConfigData` 一致。
  - `get_ody_config` 返回的 `providers[0].id/apiKey/baseUrl/defaultModel` 与 TS `OdyConfig` provider 序列化一致。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity (priority: important)
- /Users/ranwei/workspace/ody-code/packages/integration-tests/src/parity/scenarios (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

