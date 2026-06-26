# Phase 3 — Rust Host 反转实现计划

**Goal:** 在 `rust-ody` 工作区新建 `ody-host` crate，实现一个可通过 stdio/socket 与现有 TS TUI 通信的最小 Rust host 原型，覆盖会话生命周期、单次 OpenAI 兼容 LLM 流式调用和带确认的 bash 工具，并完成 Go/No-Go ADR。

**Architecture:** 采用 3A 双进程架构：Rust `ody-host` 作为 CoreAPI/SDKAPI 的宿主进程，负责会话持久化、LLM 调用和工具执行；TS TUI 通过 `SDKRpcClient.connect` 经 length-prefixed wire 协议连接 host，复用现有 `SessionEventHandler` 消费事件流。Rust host 按现有 `SessionStore` 目录结构与 `state.json` 字段落盘，确保与 TS 侧会话目录互操作。

**Tech Stack:** Rust (tokio, reqwest, serde_json, tracing, clap, toml), TypeScript/Node (apps/ody-code TUI, packages/node-sdk RPC client), Vitest, cargo。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

```
rust-ody/
├── Cargo.toml                              # 新增 crates/ody-host workspace member
└── crates/
    └── ody-host/
        ├── Cargo.toml
        └── src/
            ├── main.rs                     # CLI 解析、runtime 启动、signal 处理
            ├── lib.rs                      # crate root re-exports
            ├── config.rs                   # HostConfig / ProviderConfig / CLI / TOML/JSON 加载
            ├── error.rs                    # HostError / SessionError / LlmError / ToolError / TransportError / RpcError
            ├── host.rs                     # CoreHost 聚合根 + dispatch
            ├── session/
            │   ├── mod.rs                  # SessionId, SessionState, Session
            │   ├── manager.rs              # SessionManager
            │   └── store.rs                # SessionStoreAdapter (目录/key/state.json/index)
            ├── llm/
            │   ├── mod.rs                  # LlmProvider trait, ChatRequest, Message, ChatDelta, FinishReason
            │   └── openai.rs               # OpenAiProvider (reqwest + SSE)
            ├── tools/
            │   ├── mod.rs                  # Tool trait, ToolRegistry, ToolResult
            │   └── bash.rs                 # BashTool + approval 反向 RPC
            ├── events.rs                   # AgentEvent Rust 侧类型 + EventSink trait
            └── transport/
                ├── mod.rs                  # TransportServer trait, build_transport
                ├── wire.rs                 # WireMessage, HandshakeMessage, Framing, codec
                ├── connection.rs           # StreamConnection (reader/writer/dispatch)
                ├── stdio.rs                # StdioTransportServer
                ├── socket.rs               # Unix/Tcp socket servers
                └── rpc.rs                  # RpcRouter + EventSink 实现

apps/ody-code/src/
├── main.ts                                 # 新增 --host=rust 分支
├── cli/
│   ├── commands.ts                         # 新增 --host* 选项
│   ├── options.ts                          # 新增 rust host 相关 option 类型/校验
│   └── run-shell.ts                        # 新增 runShellWithRustHost 路径
├── native/
│   └── native-assets.ts                    # 评估新增 odyHost asset entry
└── tui/
    ├── rust-host-connector.ts              # RustHostConnector 实现
    └── __tests__/
        └── rust-host-connector.test.ts     # connector 单元测试

packages/node-sdk/src/
└── __tests__/
    └── rust-host-connect.test.ts           # 跨语言 RPC 连接测试

.ody-code/
├── designs/2026-06-25-backend-architecture-evolution-phase3/   # 已批准的设计文档
└── plans/2026-06-25-backend-architecture-evolution-phase3/     # 本计划分片
    ├── core.md
    ├── transport.md
    ├── tui.md
    └── packaging.md

docs/
└── designs/
    └── rust-host-reversal-adr.md           # Go/No-Go ADR（或 .ody-code/designs/ 下）
```

---

## Dependency Overview

工作分 4 个串行 Phase；每个 Phase 产出一个可独立构建/测试的子系统。跨 Phase 依赖只通过明确的接口契约传递（`CoreHost::dispatch`、`TransportServer::serve`、`SDKRpcClient.connect`）。

```
Phase A: Rust Host Core
  ├── Task A1: workspace + crate scaffold
  ├── Task A2: error types + config parsing
  ├── Task A3: SessionStoreAdapter (work dir key, state.json, index)
  ├── Task A4: SessionManager (create/list/get/close)
  ├── Task A5: LLM provider trait + OpenAI SSE provider
  ├── Task A6: Tool registry + BashTool + approval
  └── Task A7: CoreHost dispatch + EventSink integration
         │
         ▼
Phase B: Rust Transport Server
  ├── Task B1: wire types + framing codec
  ├── Task B2: handshake
  ├── Task B3: StreamConnection
  ├── Task B4: StdioTransportServer
  ├── Task B5: Unix/Tcp socket servers
  ├── Task B6: RpcRouter + EventSink transport impl
  └── Task B7: cross-language framing tests
         │
         ▼
Phase C: TS TUI Adaptation
  ├── Task C1: RustHostConnector
  ├── Task C2: CLI options (--host=rust, --host-stdio, etc.)
  ├── Task C3: main.ts / run-shell integration
  ├── Task C4: OdyTUI adapter for SDKRpcClient
  └── Task C5: connector tests
         │
         ▼
Phase D: Build, Packaging & Done Criteria
  ├── Task D1: ody-host Cargo.toml + root package.json scripts
  ├── Task D2: Go/No-Go ADR 文档
  ├── Task D3: native asset manifest evaluation (SEA)
  ├── Task D4: CI smoke job
  └── Task D5: end-to-end done-criteria verification
```

**并行性说明**：Phase A/B/C/D 必须按顺序执行，因为后一 Phase 依赖前一 Phase 产出的二进制或接口。Phase 内部任务基本串行，但 A3/A4 与 A5 可独立开发后再在 A7 集成；计划为减少上下文切换仍按顺序列出。

---

## Risks & Open Questions

| # | Risk / Open Question | 处理策略 |
|---|---|---|
| R1 | `SessionStore` 字段复杂（agents、forkedFrom、custom），Rust 侧写 `state.json` 字段与 TS 不一致 | A3 任务严格复刻 `SessionSummaryStateSchema` 字段；A7 用 golden 测试断言兼容性 |
| R2 | `workDirKey` slug+hash 算法跨语言不一致导致目录对不上 | A3 任务逐行复刻 `workdir-key.ts:9-17` + `workdir-slug.ts:3-10`，并加 roundtrip 测试 |
| R3 | length-prefixed framing 握手/字节序不一致 | B1/B7 与 TS `stream.ts:76-109` 逐字节对照测试 |
| R4 | OpenAI SSE tool-call delta 流式解析与 TS `kosong` 行为差异 | A5 仅实现文本流；tool call 一次性 buffer，A6 用简单 JSON 参数 |
| R5 | TS TUI 深度依赖 `KimiHarness` 接口，换成 `SDKRpcClient` 改动面大 | C4 采用“保持 OdyTUI 构造函数签名，内部增加 `SDKRpcClient` 分支”的最小侵入策略 |
| R6 | Node SEA 嵌入 Rust 二进制体积大或平台矩阵复杂 | D3 为 optional evaluation，不阻塞原型 |
| R7 | Rust host 断开后 TUI 行为未测试 | C1/C5 显式测试 `onDisconnect` 回调；C3 在 main 分支注册后打印错误并 `process.exit(1)` |

---

## Spec-Coverage Table

| 设计章节 | 需求 | 覆盖任务 | 状态 |
|---|---|---|---|
| index §In Scope 1 | ADR 文档 | D2 | covered |
| index §In Scope 2.1 | `ody-host` crate + `CoreAPI` 子集 | A1, A7 | covered |
| index §In Scope 2.2 | 最小 OpenAI 兼容 LLM provider | A5 | covered |
| index §In Scope 2.3 | 内置 bash tool + 确认门 | A6 | covered |
| index §In Scope 2.4 | 复用 `SessionStore` 格式 | A3, A4 | covered |
| index §In Scope 2.5 | host 自读配置文件 | A2 | covered |
| index §In Scope 2.6 | TS TUI 通过 `SDKRpcClient.connect` 连接 host | C1, C3 | covered |
| index §In Scope 2.7 | 复用 `SessionEventHandler` | C4 | covered |
| index §In Scope 3 | stdio + socket transport，length-prefixed wire 协议 | B1-B7 | covered |
| index §In Scope 4 | cargo build 产出 + SEA 评估 | D1, D3 | covered |
| index §Out of Scope 1-7 | 不迁移完整 CoreAPI/kaos/kosong/MCP/skill 等 | — | no-op（本计划不覆盖） |
| core.md §2.1 | `HostConfig` | A2 | covered |
| core.md §2.2 | `CoreHost` | A7 | covered |
| core.md §2.3 | `SessionManager`/`Session`/`SessionState` | A3, A4 | covered |
| core.md §2.4 | `LlmProvider`/`ChatRequest`/`OpenAiProvider` | A5 | covered |
| core.md §2.5 | `ToolRegistry`/`BashTool` | A6 | covered |
| core.md §2.6 | `EventSink` | A7, B6 | covered |
| core.md §3.1-3.5 | 核心算法 | A3-A7 | covered |
| transport.md §2.1-2.4 | Wire/TransportServer/RpcRouter/EventSink | B1-B6 | covered |
| transport.md §3.1-3.8 | transport 算法 | B1-B6 | covered |
| tui.md §2.1-2.3 | `RustHostLaunchOptions`/`RustHostConnector`/`ClientAPI` | C1, C4 | covered |
| tui.md §3.1-3.4 | TUI 连接/启动/事件消费 | C1-C4 | covered |
| packaging.md §2.1-2.3 | 构建产物/SEA manifest/resolver | D1, D3 | covered |
| packaging.md §3.1-3.4 | 构建流程/CLI/SEA/CI | D1-D4 | covered |
| packaging.md §6 | Done criteria | D5 | covered |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-25-backend-architecture-evolution-phase3/core.md` | Rust host core：config、session runtime/persistence、LLM provider、tool | done |
| 2 | `2026-06-25-backend-architecture-evolution-phase3/transport.md` | Rust transport server、wire protocol、stdio/socket lifecycle | done |
| 3 | `2026-06-25-backend-architecture-evolution-phase3/tui.md` | TS TUI adaptation：connector、CLI options、main integration、event consumption | done |
| 4 | `2026-06-25-backend-architecture-evolution-phase3/packaging.md` | Build scripts、SEA evaluation、ADR、CI、done criteria | done |
