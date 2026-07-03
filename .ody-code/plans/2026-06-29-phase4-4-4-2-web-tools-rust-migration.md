# Phase 4.4.2 Web Tools Rust 迁移实施计划

**Goal:** 将 TypeScript `tools/builtin/web` 下的 `FetchURL` 与 `WebSearch` 工具及 host provider 骨架完整迁移到 Rust (`ody-host`)，并通过 L1/L3 对照测试。

**Architecture:** 在 `ody-host` 层新增 `UrlFetcher`/`WebSearchProvider` trait，由 `FetchURLTool`/`WebSearchTool` 实现现有 `Tool` trait；host 通过可选注入的 provider 决定是否注册这两个工具。`agent-rs::ToolManager` 的 builtin 工具元数据同步增加 `FetchURL`/`WebSearch`，并通过 `AgentBuilder` 标志控制是否暴露给 LLM。所有变更以 L1 golden fixture 与 L3 agent turn fixture 钉死等价性。

**Tech Stack:** Rust (tokio/async-trait/serde_json), `ody-host`, `agent-rs`, `tools-rs` golden harness, TypeScript parity runner (`packages/integration-tests/src/parity`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## 1. 范围与文件结构

本次迁移覆盖路线图中 **4.4.2 Web tools** 的全部条目：

| 路线图条目 | 内容 | 主要落地位置 |
|---|---|---|
| 4.4.2.1 | `FetchURLTool` | `rust-ody/crates/ody-host/src/tools/fetch_url.rs` |
| 4.4.2.2 | `WebSearchTool` | `rust-ody/crates/ody-host/src/tools/web_search.rs` |
| 4.4.2.3 | host provider 骨架（local fetch + search registry/noop） | `rust-ody/crates/ody-host/src/tools/providers/` |
| 4.4.2.4 | L1 + L3 对照 | `packages/integration-tests/src/parity/fixtures/tools-rs/web-tools.json`、`rust-ody/crates/*/tests/` |

新增/修改的核心文件清单：

- `rust-ody/crates/ody-host/src/tools/web.rs` — 共享 trait 与错误类型
- `rust-ody/crates/ody-host/src/tools/fetch_url.rs` — FetchURL 工具实现
- `rust-ody/crates/ody-host/src/tools/web_search.rs` — WebSearch 工具实现
- `rust-ody/crates/ody-host/src/tools/providers/local_fetch_url.rs` — 本地 URL fetch provider
- `rust-ody/crates/ody-host/src/tools/providers/web_search.rs` — WebSearch provider trait 与 noop 实现
- `rust-ody/crates/ody-host/src/tools/mod.rs` — 注册 web 工具
- `rust-ody/crates/ody-host/src/host.rs` — `CoreHost::new` 注入 provider
- `rust-ody/crates/ody-host/src/main.rs` — 适配 `CoreHost::new` 新签名
- `rust-ody/crates/ody-host/Cargo.toml` — 新增 `tools-rs`、`readability` 依赖
- `rust-ody/crates/agent-rs/src/tool/manager.rs` — builtin 元数据增加 FetchURL/WebSearch
- `rust-ody/crates/agent-rs/src/agent.rs` — `AgentBuilder` 增加 web 工具可用性标志
- `rust-ody/crates/agent-rs/src/config/state.rs` — `AgentConfigContext::initialize_builtin_tools` 透传标志
- `rust-ody/crates/agent-rs/src/bin/generate_tool_fixture.rs` — 更新 fixture
- `rust-ody/crates/agent-rs/fixtures/tools-rust.json` — 更新预期工具列表
- `rust-ody/crates/tools-rs/src/golden.rs` — 扩展 async web tool op
- `packages/integration-tests/src/parity/fixtures/tools-rs/web-tools.json` — L1 fixture
- `packages/integration-tests/src/parity/tools-rs-golden.ts` — TS 端 web tool golden runner
- `packages/integration-tests/src/parity/scenarios/web-search-l3.ts` — L3 scenario

---

## 2. 依赖关系与执行阶段

```
Phase A: 契约与基础设施（可独立验收）
  ├── Task 1: 定义 UrlFetcher/WebSearchProvider trait 与错误类型
  ├── Task 2: 扩展 tools-rs golden.rs 支持 async web tool op
  └── Task 3: 实现 mock fetcher/searcher 用于测试

Phase B: 工具实现与 host 注册（依赖 A）
  ├── Task 4: 实现 FetchURLTool
  ├── Task 5: 实现 WebSearchTool
  ├── Task 6: 实现 LocalFetchURLProvider 与 noop WebSearch provider
  └── Task 7: CoreHost 条件注册 web 工具

Phase C: agent-rs 集成（依赖 A/B）
  ├── Task 8: ToolManager 增加 FetchURL/WebSearch 元数据
  └── Task 9: AgentBuilder/AgentContext 透传 web 工具可用性

Phase D: 对照测试（依赖 B/C）
  ├── Task 10: L1 golden fixture 与 TS/Rust runner
  ├── Task 11: L3 agent turn scenario
  └── Task 12: 运行 parity 套件并修复差异
```

Phase A/B/C 可部分并行：C 依赖 A 的 trait 类型，但不依赖 B 的具体实现；D 依赖 B/C 完成。

---

## 3. 风险与待确认事项

| 风险 | 应对 |
|---|---|
| HTML 提取库 `readability` 与 TS `@mozilla/readability` 输出不完全一致 | L1 fixture 只使用 `text/plain`/`text/markdown` passthrough 与简单 HTML fallback，复杂 HTML 提取登记为 known gap |
| `CoreHost::new` 签名变更影响所有调用方 | 同一任务内更新 `main.rs` 与全部测试，并以 `cargo test -p ody-host` 验证 |
| agent-rs `initialize_builtin_tools` 行为变更 | 保持签名不变，通过 `ToolManager` 实例标志控制，避免扇出 |
| search provider 12 家全部迁移工作爆炸 | 本子阶段只迁移 trait + registry 骨架 + noop/static provider，真实 provider 按 4.5.0 triage 决策 |

---

## 4. 路线图需求覆盖表

| 路线图需求 | 覆盖任务 | 状态 |
|---|---|---|
| 4.4.2.1 FetchURLTool | Task 4 | covered |
| 4.4.2.2 WebSearchTool | Task 5 | covered |
| 4.4.2.3 host provider 实现（local fetch + search registry/noop） | Task 3, 6 | covered |
| 4.4.2.4 L1 + L3 对照 | Task 10, 11, 12 | covered |
| 工具描述文件 | Task 4, 5 | covered |
| 条件注册（无 provider 时不暴露） | Task 7, 9 | covered |
| agent-rs builtin 元数据同步 | Task 8 | covered |

---

## 5. Parts 清单

| # | 文件 | 范围 | 状态 |
|---|---|---|---|
| 1 | `2026-06-29-phase4-4-4-2-web-tools-rust-migration/contracts.md` | trait 契约 + golden async 扩展 + mock | done |
| 2 | `2026-06-29-phase4-4-4-2-web-tools-rust-migration/tools.md` | FetchURL/WebSearch 工具 + host provider + CoreHost 注册 | pending |
| 3 | `2026-06-29-phase4-4-4-2-web-tools-rust-migration/agent-integration.md` | agent-rs ToolManager + AgentBuilder 标志 | pending |
| 4 | `2026-06-29-phase4-4-4-2-web-tools-rust-migration/parity.md` | L1/L3 对照测试 + 最终 parity 运行 | pending |
