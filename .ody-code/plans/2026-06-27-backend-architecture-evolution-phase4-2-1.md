# 4.2.1 kosong 通用工具层迁移实施计划

**Goal:** 将 `packages/kosong/src/providers/{tool-call-id,request-auth,capability-registry}.ts` 与 `packages/kosong/src/catalog.ts` 的纯函数横切逻辑迁移到 `rust-ody/crates/kosong-rs`，并通过 L1 golden fixture 与 TS 实现逐值对照。

**Architecture:** 在 `kosong-rs` 中新增 `tool_call_id`、`request_auth`、`capability_registry`、`catalog` 四个无状态模块，全部依赖 4.2.0 已落地的 `Message`/`ToolCall`/`ProviderRequestAuth`/`ModelCapability` 类型；新增独立的 `kosong-utils-golden` 二进制与 TS `kosong-golden.ts` 的 utility 分支，使同一份 JSON fixture 分别驱动 TS 与 Rust 实现并比对输出。

**Tech Stack:** Rust (tokio, serde, regex), TypeScript (vitest, `@odysseythink/kosong`), JSON golden fixtures.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

```
rust-ody/crates/kosong-rs/
  src/
    lib.rs                          # 新增 mod 声明与 re-export
    provider.rs                     # 新增 ProviderType enum
    tool_call_id.rs                 # sanitize / normalize / makeUnique
    request_auth.rs                 # requireProviderApiKey / mergeRequestHeaders / resolveAuthBackedClient
    capability_registry.rs          # OpenAI/Anthropic/Google capability 查询 + developer role
    catalog.rs                      # inferWireType / catalogBaseUrl / catalogModelToCapability / catalogProviderModels
    bin/
      utils_golden.rs               # 新二进制：解析 fixture 并输出 { operations: [...] }
  Cargo.toml                        # 新增 [[bin]] kosong-utils-golden

packages/integration-tests/
  src/parity/
    kosong-utils-golden.ts          # 新增 runTsKosongUtilsGolden
    fixtures/kosong-utils/
      tool-call-id.json
      request-auth.json
      capability-registry.json
      catalog.json
  test/parity/kosong/
    l1-utils-golden.test.ts         # 编译 kosong-utils-golden，TS vs Rust 跑全部 utils fixture
```

---

## Dependency Overview

```
Phase 4.2.0 已完成（kosong-rs 共享数据模型 + generate 循环）
  │
  ├──► Task 1: 新增 ProviderType enum
  │       │
  │       ├──► Task 2: tool-call-id 模块
  │       ├──► Task 3: request-auth 模块
  │       ├──► Task 4: capability-registry 模块
  │       └──► Task 5: catalog 模块（依赖 ProviderType）
  │               │
  │               ▼
  │       Task 6: kosong-utils-golden 二进制
  │               │
  │               ▼
  │       Task 7: L1 fixture JSON 文件
  │               │
  │               ▼
  │       Task 8: TS 端 kosong-utils-golden harness
  │               │
  │               ▼
  │       Task 9: l1-utils-golden.test.ts + 全量回归
```

- **Task 1 是 Task 5 的硬前置**：`catalog::inferWireType` 需要返回 `ProviderType`。
- **Task 2/3/4 彼此独立**，可并行开发；它们都依赖 4.2.0 类型，不互相依赖。
- **Task 6 依赖 `core.md` 的 Task 1–5**：二进制必须能调用所有四个模块。
- **Task 7 依赖 Task 6**：fixtures 的 operation/case 形状由 binary 支持。
- **Task 8 依赖 Task 7**：harness 需要 fixtures 已存在以做类型检查。
- **Task 9 依赖 Task 6/7/8**：最终编译 binary 并运行 TS vs Rust parity 测试。

---

## Risks & Open Questions

| 风险 | 应对 |
|---|---|
| `resolveAuthBackedClient` 在 Rust 中需要泛型/闭包表达，可能与 TS 的 `TClient` 语义不完全一致 | 用简单 mock client（如 `String` 或 `MockClient` struct）写单元测试，断言缓存/per-request/factory 三条分支 |
| `normalizeToolCallIdsForProvider` 涉及 `Message` 深拷贝语义，Rust 与 TS 在空/undefined 字段序列化后可能不同 | fixture 预期值使用 JSON 深比较，Rust serde 的 `skip_serializing_if` 与 TS 删除 undefined 行为一致 |
| catalog 的 `ProviderType` 字符串映射（如 `google-genai` vs `google_genai`） | Rust enum 使用 `#[serde(rename = "google-genai")]` 保持与 TS 字符串一致 |
| 新二进制与现有 `kosong-golden` 职责混淆 | 保持 `kosong-golden` 只跑 generate 循环；新增 `kosong-utils-golden` 只跑 4.2.1 纯函数 |

---

## Spec Coverage

| 4.2.1 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.2.1.1 tool-call-id 语义（sanitize / OpenAI Responses call id / normalize / 64 字符截断 / 冲突重命名） | Task 2, Task 8 fixture | covered |
| 4.2.1.2 request-auth 语义（requireProviderApiKey / mergeRequestHeaders / resolveAuthBackedClient precedence） | Task 3, Task 8 fixture | covered |
| 4.2.1.3 capability-registry（按模型名前缀匹配 ModelCapability；OpenAI/Anthropic/Google 家族映射） | Task 4, Task 8 fixture | covered |
| 4.2.1.4 catalog 解析（inferWireType / catalogBaseUrl / catalogModelToCapability） | Task 1, Task 5, Task 8 fixture | covered |
| 4.2.1.5 L1 golden fixture（tool-call-id / capability / catalog 对照表） | Task 6, Task 7, Task 8, Task 9 | covered |
| 门 G4-2-1 | Task 9 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-27-backend-architecture-evolution-phase4-2-1/core.md` | Rust `kosong-rs` utility 模块 | done |
| 2 | `2026-06-27-backend-architecture-evolution-phase4-2-1/parity.md` | L1 golden harness + fixtures + 验证 | done |

---

## Final Self-Review

- [ ] 1. Spec-coverage table：见上文 Spec Coverage，4.2.1 全部 5 个条目均已映射到具体 Task，无 GAP。
- [ ] 2. Placeholder scan：index、core.md、parity.md 中均无 `TODO`/`TBD`/`待实现`；每个 Task 给出完整代码或 fixture 内容。
- [ ] 3. No phantom tasks：9 个 Task 每个都产生可验证的文件改动或测试命令，无 `--allow-empty` 或 "已在 Task N 完成" 的空转。
- [ ] 4. Dependency soundness：每个 `Depends on:` 均指向更早的 Task；跨 part 依赖（`parity.md` 依赖 `core.md` Task 1–5）已声明。
- [ ] 5. Caller & build soundness：唯一共享签名变更是 `core.md` Task 3 新增 `ChatProviderError::MissingApiKey`，同一任务内更新 `errors.rs` 与 `generate.rs` 的 match 并以 `cargo check -p kosong-rs` 收尾；parity 部分未改动共享签名，以 `pnpm -F @odysseythink/integration-tests typecheck` 与 `test:parity` 收尾。
- [ ] 6. Test-the-risk：
  - tool-call-id：覆盖冲突 id 重命名、64 字符截断、pipe 分割；
  - request-auth：覆盖空 apiKey、default 优先级、header 合并/覆盖；
  - capability-registry：覆盖 OpenAI/Anthropic/Google 家族前缀与 reasoning 模型；
  - catalog：覆盖 embedding 过滤、/v1 剥离、未知 provider；
  - L1 golden：逐 fixture 用 `toStrictEqual(sortKeys(...))` 比较 TS/Rust 输出。
- [ ] 7. Type consistency：`ProviderType`、`Catalog*`、`Message`、`ToolCall` 的字段名与 serde 设置在 `core.md` 与 `parity.md` 中保持一致；binary 与 TS harness 的 operation 名称一一对应。
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

