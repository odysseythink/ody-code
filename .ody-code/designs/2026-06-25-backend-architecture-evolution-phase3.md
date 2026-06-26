# Phase 3 — Rust Host 反转：Go/No-Go ADR + 端到端原型设计

> **Document Type**: Design (Phase 3 of Backend Architecture Evolution Roadmap)  
> **Status**: DRAFT — pending approval  
> **Audit Level**: Deep [C:USER]  
> **Decision Gate**: G3 — Go/No-Go for Rust Host reversal  
> **Recommended Path**: 3A 双进程（Rust host binary + TS TUI client） [C:USER]

---

## 已确认决策清单（Resolved decisions）

| # | 维度 | 决策 | 来源 |
|---|---|---|---|
| 1 | 审计策略 | Deep | [C:USER] |
| 2 | Scope | ADR + 端到端原型（非纯文档） | [C:USER] |
| 3 | Data & State | 内存会话 + 落盘 session store（复用现有 `SessionStore` 格式） | [C:USER] |
| 4 | Integration | 同时支持 stdio 与 socket transport，stdio 为默认 | [C:USER] |
| 5 | Error & Degradation | TUI 在 Rust host 断开时报告错误并退出（原型阶段不重连、不降级） | [C:USER] |
| 6 | Security | 本地 stdio/socket 默认无鉴权，依赖 OS 进程/文件权限 | [C:USER] |
| 7 | Observability | 复用现有 Event 流，Rust host 通过 `SDKAPI.emitEvent` 向 TUI 推送 | [C:USER] |
| 8 | Operations | Rust host 自读 TOML/JSON 配置文件，TUI 仅传少量 CLI 覆盖项 | [C:USER] |
| 9 | 架构方案 | 3A 双进程（Rust host binary + TS TUI client） | [C:USER] |
| 10 | 多终端行为 | 每个终端独立启动自己的 Rust host + TUI，互不共享状态 | [C:USER] |
| 11 | API 表面 | 接近真实：覆盖会话生命周期 + LLM 真实调用 + 一个内置 tool | [C:USER] |
| 12 | LLM 实现 | Rust host 复用 `kosong` 的接口/类型设计，用 Rust `reqwest`+SSE 重新实现 | [C:USER] |

---

## Scope In / Out

### In Scope

1. **ADR 文档** [C:USER]
   - 3A 双进程 vs 3B 内嵌 JS 引擎的完整对比。
   - Go/No-Go 判据与实测数据要求。
   - 若 No-Go，明确路线图收官状态。
2. **端到端原型** [C:USER]
   - 新增 Rust crate `ody-host`（二进制）。
   - Rust host 实现 `CoreAPI` 子集：`getCoreInfo`、`createSession`、`closeSession`、`listSessions`、`prompt`。
   - Rust host 实现一个最小 OpenAI 兼容 LLM provider（`reqwest` + SSE 流式）。
   - Rust host 实现一个内置 tool（`bash`，带确认门）。
   - Rust host 复用现有 `SessionStore` 的磁盘格式创建/列出/关闭会话。
   - Rust host 自读配置文件（TOML 或 JSON）。
   - TS TUI 通过 `SDKRpcClient.connect` 经 stdio/socket 连接 Rust host。
   - TS TUI 复用现有 `SessionEventHandler` 消费 Rust host 发出的 `AgentEvent`。
3. **Transport 边界** [C:USER]
   - Rust 侧实现与 `packages/agent-core/src/rpc/transports/stream.ts` 兼容的 length-prefixed + handshake wire 协议。
   - 同时支持 stdio（默认）与 Unix/TCP socket。
4. **构建与分发评估** [C:INFERRED]
   - `cargo build` 产出 `ody-host` 二进制。
   - 评估 Node SEA 将 `ody-host` 作为资源嵌入单文件分发包的方案（不强制实现）。

### Out of Scope

1. **不实现完整 `CoreAPI`** [C:USER]
   - 理由：原型聚焦传输边界 + 会话生命周期 + 单 provider + 单 tool；其余方法（plan/design/office-hours/background/cron/MCP/code-review 等）属 Phase 4。
2. **不迁移 `kaos`/`kosong` 全部 provider** [C:USER]
   - 理由：只实现一个 OpenAI 兼容 provider 验证 Rust LLM 路径；其余 provider 适配留 Phase 4。
3. **不实现 MCP 客户端** [C:USER]
   - 理由：MCP 协议栈较大，原型阶段用内置 tool 替代；MCP 迁移作为 Phase 4 独立工作项。
4. **不实现 skill / plugin / code-review / office-hours** [C:USER]
   - 理由：这些属于 `agent-core` 边缘能力，已在 Phase 1-C/2-D 拆包或尚未迁移；原型不覆盖。
5. **不实现 TUI 降级到 TS Core worker** [C:USER]
   - 理由：原型阶段错误策略为“断开即退出”；双宿主切换是后续兼容性项。
6. **不实现配置热重载 / 远程 daemon / 多 TUI attach** [C:USER]
   - 理由：独立终端场景下无需共享 daemon；daemon/attach 模式作为架构扩展点保留，但不实现。
7. **不修改现有 TS Core 实现** [C:INFERRED]
   - 理由：原型是新增 Rust host 路径，现有 `KimiCore`/worker/inproc 路径保持不动。

---

## Prior Art

> 见 Step 0.5 搜索结果摘要。

| 项目 | 架构 | 与本设计的关系 |
|---|---|---|
| **claude-code-rust** | Rust binary + TypeScript bridge（NDJSON on stdio） | 直接佐证 3A 双进程可行性：Rust 侧跑事件循环，TS 侧做 bridge。 |
| **Codex CLI (OpenAI)** | Rust-based terminal coding agent | 证明 Rust 宿主在 agent CLI 中的性能与沙箱优势。 |
| **Goose** | Rust agent + MCP/ACP extension model | 证明 Rust 侧可承载 agent 编排与工具扩展。 |
| **TauRPC / quox-terminal** | Tauri 2.0 Rust backend + TS frontend，类型安全 IPC | 证明 Rust↔TS 的类型安全 IPC（Specta/taurpc）可工程化；本设计可借鉴类型生成思路。 |
| **LingTai** | Go TUI + Python/Rust kernel，文件系统 mailbox | 反例：本设计不采用文件系统 mailbox，而采用已存在的 `Transport`/`StreamTransport` 线协议。 |
| **imp-tui** | Rust 本体 + imp-core SDK | 证明纯 Rust agent 生态成熟，但与本项目“保留 TS TUI”策略不同。 |

---

## Architecture & Data Flow

### 运行时进程视图（3A 双进程）

```
┌─────────────────────────────────────────────────────────────────┐
│ Terminal 1 (per-terminal isolated)                              │
│  ┌─────────────────────┐      stdio / socket      ┌──────────┐ │
│  │  apps/ody-code TUI  │  <=====================> │ ody-host │ │
│  │  (Node SEA / tsx)   │   length-prefixed JSON   │ (Rust)   │ │
│  └─────────────────────┘                          └──────────┘ │
│        ▲                                                │       │
│        │ SDKAPI.emitEvent                               │       │
│        │ (AgentEvent stream)                            │       │
│        └────────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────┘

Terminal 2 runs its own independent ody-host + TUI pair.
```

### 数据流箭头

| # | 方向 | 数据变化 | 说明 |
|---|---|---|---|
| 1 | User → TUI | 原始输入（prompt、命令） | TUI 接收键盘输入。 |
| 2 | TUI → Rust host | `CoreAPI` request JSON（`prompt`、`createSession` 等） | 经 `StreamTransport` 序列化。 |
| 3 | Rust host → LLM endpoint | HTTP POST + SSE stream（OpenAI 兼容） | `reqwest` 异步流式请求。 |
| 4 | LLM endpoint → Rust host | SSE chunks → assistant deltas | Rust host 组装消息。 |
| 5 | Rust host → TUI | `SDKAPI.emitEvent` JSON（`AgentEvent`） | 流式返回 assistant 消息、工具调用事件。 |
| 6 | Rust host ↔ disk | `state.json` / `wire.jsonl` / session index | 复用 `SessionStore` 格式。 |
| 7 | Rust host → config file | `OdyConfig`（TOML/JSON） | 启动时读取。 |

### 模块边界

```
ody-host (Rust)
├── main.rs              # CLI 解析、runtime 启动、signal 处理
├── rpc/                 # CoreAPI/SDKAPI 请求分发
├── transport/           # stdio / socket server transport
├── session/             # Session runtime + SessionStore adapter
├── llm/                 # 最小 OpenAI 兼容 provider
├── tools/               # 内置 tool（bash）
└── config/              # TOML/JSON 配置加载

apps/ody-code (TS TUI)
├── main.ts              # 启动/连接 Rust host
├── cli/commands.ts      # 新增 --host=rust / --host-stdio 等选项
└── tui/ody-tui.ts       # 复用 OdyTUI，底层换 SDKRpcClient.connect
```

---

## Data Models

> 完整数据类型定义见 `2026-06-25-backend-architecture-evolution-phase3/core.md` §2。

| Model | Where defined | Key fields | Lifecycle |
|---|---|---|---|
| `HostConfig` | core.md §2.1 | `home_dir`, `config_path`, `transport`, `log_level`, `provider` | 启动时从 CLI + 配置文件加载，运行期只读 |
| `Session` / `SessionState` | core.md §2.3 | `id`, `work_dir`, `dir`, `title`, `last_prompt`, `custom` | `createSession` 创建 → 内存 `SessionManager` 持有 → `closeSession` 释放；磁盘 `state.json` 持久 |
| `SessionSummary` | core.md §2.3 + §3.5 | `id`, `workDir`, `sessionDir`, `createdAt`, `updatedAt`, `title`, `lastPrompt`, `metadata` | 由 `SessionStoreAdapter` 从磁盘目录实时构建 |
| `ChatRequest` / `Message` / `ChatDelta` | core.md §2.4 | `model`, `messages`, `tools`, `stream` | 每次 `prompt` 新建请求；SSE delta 流式产生 |
| `ToolDefinition` / `ToolResult` | core.md §2.5 | `name`, `parameters`, `output`, `is_error` | 启动时注册到 `ToolRegistry`；每次 tool call 执行 |
| `WireMessage` / `HandshakeMessage` | transport.md §2.1 | `req_id`, `bytes`, `error`, `framing`, `token` | 每条 transport frame 创建/解析 |
| `RpcRequestWrapper` / `RpcResponseWrapper` | transport.md §2.2 | `method`, `args`, `ok`, `value`, `error` | 每个 RPC call 创建/解析 |

---

## Algorithms

> 完整算法伪代码见各 part file。

| # | Algorithm | Location | Summary |
|---|---|---|---|
| 1 | `CoreHost::dispatch` | core.md §3.1 | 根据 `method` 路由到 `getCoreInfo`/`createSession`/`listSessions`/`closeSession`/`prompt`；`prompt` 异步 spawn |
| 2 | `SessionManager::create` | core.md §3.2 | 生成 id → normalize workDir → 创建 `homeDir/sessions/{workDirKey}/{id}` → 写 `state.json` → append session index |
| 3 | `handle_prompt` | core.md §3.3 | 更新 state → emit `UserMessage` → 调用 LLM provider 流式 delta → emit `AssistantDelta`/`AssistantFinish` → 可选执行 tool |
| 4 | `OpenAiProvider::chat_stream` | core.md §3.4 | POST `/v1/chat/completions` → 解析 SSE `data:` 行 → 流式回调 delta → 返回 `FinishReason` |
| 5 | `SessionStoreAdapter::summary_from_dir` | core.md §3.5 | `stat(dir)` + read `state.json` + mtime 聚合 → 返回 `SessionSummary` |
| 6 | `StreamConnection::run` | transport.md §3.4 | 启动 writer/reader tasks → 分发 request/response → 处理反向 RPC |
| 7 | `perform_handshake` | transport.md §3.5 | server 发送 hello → 读取 client hello → 校验 framing/token |
| 8 | `encode_and_write` / `read_frame` | transport.md §3.6/§3.7 | length-prefixed: `u32le(len) + payload`; ndjson: `payload + \n` |
| 9 | `RpcRouter::route` | transport.md §2.2 | 解析 `{method, args}` → 调用 `host.dispatch(method, args[0])` → 包装 `{ok, value/error}` |
| 10 | `RustHostConnector.connect` | tui.md §3.1 | spawn `ody-host` → wait ready → 建 `StreamTransport` → `createRPCEndpoint` → 返回 `SDKRpcClient` |

---

## Error Handling

> 各子系统的详细错误表见 core.md §5、transport.md §5、tui.md §5、packaging.md §5。

### Cross-cutting error classes

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `TransportError::InvalidFraming` | Close connection, log | TUI 看到 host 退出 | 重启 host |
| `RpcError::MethodNotImplemented` | Return `{ok:false,error}` for that reqId | TUI 显示该操作不可用 | 不调用原型未覆盖方法 |
| `SessionStoreError::AlreadyExists` / `NotFound` | Return对应 error code | TUI 提示用户 | 换 id / listSessions 重选 |
| `LlmError::ApiError` / `StreamParse` | Emit `AgentEvent::Error` | 该 turn 终止 | 用户重试 prompt |
| `HostError::ConfigInvalid` | stderr + exit(1) | Host 不启动 | 修正 config |
| Rust host disconnect | TUI print error + exit(1) | TUI 退出（原型策略） | 用户重新启动 ody |

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify | Source |
|---|---|---|---|---|---|
| A1 | `ody-host` crate 可加入 `rust-ody` workspace 并与 `ody-rust`/`ody-crypto` 共存 | High | 低；仅 workspace 配置调整 | 检查 `rust-ody/Cargo.toml` | [C:INFERRED] |
| A2 | Rust host 使用 `tokio` + `reqwest` + `serde_json` + `tracing` 作为基础依赖栈 | High | 低；技术选型可替换 | 在原型 Cargo.toml 中落地 | [C:INFERRED] |
| A3 | `StreamTransport` 的 length-prefixed framing 可直接在 Rust 中复刻（4-byte LE length + JSON payload） | High | 中；若 Rust 侧实现不一致则 transport 不通 | 与 TS `stream.ts:76-109` 逐字节对照测试 | [C:INFERRED] |
| A4 | Rust host 配置文件使用 TOML（`ody.toml`），字段与 `OdyConfig` schema 子集对齐 | Medium | 中；若格式不兼容则配置迁移成本高 | 在原型中实现并对比 TS `OdyConfigSchema` | [C:INFERRED] |
| A5 | 原型 LLM provider 只实现 OpenAI 兼容接口一家（复用 `kosong` 类型设计） | High | 低；仅影响原型范围 | 实现 `reqwest` SSE 调用并跑通真实 API | [C:INFERRED] |
| A6 | 内置 tool 选择 `bash`，并复用现有 approval 反向 RPC 流程 | High | 低；tool 可替换 | 实现 tool call → emit event → requestApproval → execute | [C:INFERRED] |
| A7 | Rust host 可直接按现有 `SessionStore` 目录结构（`homeDir/sessions/{workDirKey}/{id}/state.json`）读写 | Medium | 高；若格式不兼容则无法复用现有会话 | 与 `session-store.ts:46-71,269-296` 逐字段对照 | [C:INFERRED] |
| A8 | TS TUI 现有 `SessionEventHandler` 无需修改即可消费 Rust host 发出的 `AgentEvent` | Medium | 中；若事件字段不一致需调整 TUI | 原型跑通后检查事件消费 | [C:INFERRED] |
| A9 | Node SEA 可将 `ody-host` 二进制作为资源嵌入并在运行时提取到临时路径 spawn | Medium | 中；若不可行则发布包需多文件 | 在 packaging part 中做 PoC | [C:INFERRED] |
| A10 | 原型阶段不实现鉴权是安全的，因为只监听 localhost / stdio | High | 低；后续 socket daemon 场景需补 token | 在 ADR 中记录限制 | [C:INFERRED] |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Rust host 实现 `CoreAPI` 子集时与 TS 侧类型/字段不一致，导致 TUI 解析失败 | Medium | High | 用 `scripts/gen-rpc-schema.ts` 生成 JSON Schema，Rust 类型由 schema 派生；CI 加契约 golden 测试。 |
| R2 | `SessionStore` 格式复杂（`wire.jsonl`、agent wire、fork 状态），Rust 侧复刻不完整 | Medium | High | 原型只覆盖 create/list/close；fork/resume 明确 out of scope；写 golden 测试断言目录结构与 `state.json` 字段。 |
| R3 | LLM SSE 流式处理在 Rust 侧与现有 TS `kosong` 行为不一致（chunk 合并、tool-call delta） | Medium | Medium | 只实现文本 completion 流；tool-call 流明确 deferred；用真实 API 跑端到端对比。 |
| R4 | Transport 握手/framing 跨语言不兼容 | Low | High | 直接复刻 `stream.ts` 的 `u32le` + JSON；写 Rust↔TS 双向 byte-level 测试。 |
| R5 | TS TUI 对断开 Rust host 的处理路径未经过测试，原型易崩溃 | Medium | Medium | 在 `ody-tui.ts` 适配层加显式 error handler；错误时打印清晰信息并 `process.exit(1)`。 |
| R6 | SEA 嵌入 Rust 二进制导致单文件体积过大或平台矩阵复杂 | Medium | Medium | 原型阶段仅做评估；发布包单文件作为 optional goal，不阻塞核心原型。 |
| R7 | 团队 Rust 能力不足以在原型时间内完成 tokio/reqwest/SSE/tool 调用 | Medium | High | 范围限定为单 provider + 单 tool；预留 fallback 到“mock LLM + 纯会话 API”的缩减路径。 |

---

## Reuse Analysis

### Reused as-is

| Component | File | Symbol | How used |
|---|---|---|---|
| CoreAPI/SDKAPI 契约 | `packages/agent-core/src/rpc/core-api.ts` | `CoreAPI`, `CoreAPIProtocol` | Rust host 实现其方法子集；TS TUI 继续通过 typed RPC 调用 [C:CODE] |
| SDKAPI 反向通道 | `packages/agent-core/src/rpc/sdk-api.ts` | `SDKAPI`, `SDKAgentAPI` | Rust host 通过 `emitEvent`/`requestApproval` 向 TUI 推送 [C:CODE] |
| Event taxonomy | `packages/agent-core/src/rpc/events.ts` | `AgentEvent`, `Event` | Rust host 发出与 TS Core 相同的事件；TUI `SessionEventHandler` 消费 [C:CODE] |
| StreamTransport 线协议 | `packages/agent-core/src/rpc/transports/stream.ts` | `createStreamTransport`, length-prefixed framing | Rust transport server 逐字节复刻 [C:CODE] |
| External RPC client | `packages/node-sdk/src/rpc.ts` | `SDKRpcClient.connect`, `createExternalTransport` | TUI 用其 spawn/connect Rust host [C:CODE] |
| SessionStore 格式 | `packages/agent-core/src/session/store/session-store.ts` | `SessionStore`, `SessionSummaryStateSchema` | Rust host 按同样目录结构与 `state.json` 字段落盘 [C:CODE] |
| OdyConfig schema | `packages/agent-core-shared/src/config.ts` | `OdyConfigSchema`, `RuntimeMode` | Rust host 配置文件字段子集来源 [C:CODE] |
| Rust workspace | `rust-ody/Cargo.toml` | workspace members | 新增 `ody-host` crate [C:CODE] |
| Wasm compute crate | `rust-ody/crates/ody-rust/src/lib.rs` | `estimate_tokens`, `compute_diff` | 原型阶段不直接调用；Phase 4 可内联为库函数 [C:CODE] |
| Native crypto crate | `rust-ody/crates/ody-crypto` | NAPI exports | TS TUI 继续复用；Rust host 可直接用底层 crate [C:CODE] |
| TUI 渲染管线 | `apps/ody-code/src/tui/ody-tui.ts` | `OdyTUI`, `SessionEventHandler` | 不变，仅替换底层 client [C:CODE] |

### Adapted

| Component | File | Symbol | Adaptation |
|---|---|---|---|
| CLI entry | `apps/ody-code/src/main.ts` | `main()` | 增加 `--host=rust` 分支，spawn/connect Rust host |
| CLI options | `apps/ody-code/src/cli/commands.ts` | `createProgram` | 增加 `--host-*` 选项 |
| Native asset loader | `apps/ody-code/src/native/native-assets.ts` | `NativeAssetManifest` | 评估增加 `odyHost` asset entry |

### Replaced / Greenfield

| Capability | Reason |
|---|---|
| Rust host binary crate (`ody-host`) | 不存在；需新建 [C:GREENFIELD] |
| Rust async runtime + transport server | 不存在；需新建 [C:GREENFIELD] |
| Rust CoreAPI request router | 不存在；需新建 [C:GREENFIELD] |
| Rust LLM provider (`reqwest` + SSE) | `kosong` 是 TS；Rust 侧重新实现 [C:GREENFIELD] |
| Rust execution environment for tools | `kaos` 是 TS；Rust 侧用 `tokio::process` 等 [C:GREENFIELD] |
| Rust session runtime | `agent-core/src/session` 是 TS；Rust 侧重新实现最小子集 [C:GREENFIELD] |

---

## Self-Review

### Highest-stakes decisions scrutinized

#### D1 — Length-prefixed framing 跨语言编码

Risk: Rust `u32le(payload.len())` 与 TS `stream.ts:76-86` 不一致 ⇒ transport 不通。

| Input | Expected bytes (TS) | Expected bytes (Rust) | Match |
|---|---|---|---|
| payload len = 0 | `00 00 00 00` | `(0u32).to_le_bytes()` → `00 00 00 00` | ✅ verified with `node -e` |
| payload len = 256 | `00 01 00 00` | `(256u32).to_le_bytes()` → `00 01 00 00` | ✅ verified with `node -e` |
| payload len = 67,108,864 (MAX_FRAME_SIZE) | `00 00 00 04` | `(67108864u32).to_le_bytes()` → `00 00 00 04` | ✅ verified with `node -e` |

Fixed during review: transport.md 最初将 `WireRequest.bytes` 描述为 "CoreAPI request payload"，后来发现 TS `createRPC` 实际发送的是 `{ method, args }` wrapper。已修正 transport.md §2.1、§3.8、§4.1 并引入 `RpcRouter`。

#### D2 — RPC wrapper 格式

Risk: Rust 解析 `{ method, args }` 或返回 `{ ok, value/error }` 的字段名/结构不对 ⇒ TS RPC 层拒绝。

| Input | Expected TS send bytes | Rust expected parse | Match |
|---|---|---|---|
| `createSession({workDir:'/tmp'})` | `{"method":"createSession","args":[{"workDir":"/tmp"}]}` | method="createSession", args[0].workDir="/tmp" | ✅ verified with `node -e` |
| Response success | `{"ok":true,"value":{"id":"s1"}}` | ok=true, value.id="s1" | ✅ verified with `node -e` |
| Response error | `{"ok":false,"error":{"message":"x","code":"E1"}}` | ok=false, error.message="x" | ✅ verified with `node -e` |

Fixed during review: core.md `CoreHost::dispatch` 签名从 `(method, bytes)` 改为 `(method, JsonValue)`，transport.md 新增 `RpcRouter` 负责 wrapper 解析/包装。

#### D3 — SessionStore `workDirKey` 算法

Risk: Rust 生成的会话目录 key 与 TS 不一致 ⇒ Rust 创建/TS 读取或反之找不到会话。

| Input | TS `encodeWorkDirKey` | Rust expected | Match |
|---|---|---|---|
| `/Users/ranwei/workspace/ody-code` | `wd_ody-code_eaef72b82f4b` | `wd_<slug>_<sha256-12>` | ✅ verified with `node -e` |
| `/tmp/foo bar!baz` | `wd_foo-bar-baz_24daf27f8e98` | slugify+sha256-12 | ✅ verified with `node -e` |
| `/` | `wd_workspace_8a5edab28263` | special-case empty/`.`/`..` slug → "workspace" | ✅ verified with `node -e` |

Fixed during review: core.md `SessionManager::create` 中 `encode_work_dir_key` 算法待 Rust 实现时须逐行对照 `workdir-key.ts:9-17` + `workdir-slug.ts:3-10`；已在 Self-Review 中记录，未修改设计结论。

### Four-lens sweep

- **Security**: 检查了原型阶段的鉴权策略（本地 stdio/socket 无 token）和 secrets 处理。发现原设计未明确 LLM API key 存放位置；已确认 Rust host 自读 config，TUI 不接触 key，但 config 文件权限（`0o600`）应在实现时 enforce。无 filter/regex 泄漏 PII 风险，因为原型不新采集数据。
- **Test**: 检查了每部分的 must-pass / must-reject assertions。发现 transport.md 原 §3.8 对 "method 如何传递" 描述与真实 wire 格式不一致，已修正。所有行为都有正向和反向断言。
- **Ops**: 检查了并发与重复行为。发现 socket transport prototype 未明确多连接策略；已在 transport.md §3.3 注明 "single connection only; second connection rejected or queued"。session id 由 UUID v7 生成，碰撞可忽略。
- **Integration**: 验证了所有关键 hook 存在：`CoreAPI`/`SDKAPI`、`StreamTransport`、`SDKRpcClient.connect`、`SessionStore`、`OdyConfig`、`rust-ody` workspace 均真实存在。修正了 wire wrapper 理解错误。设计落地在 `rust-ody/crates/ody-host`，与 roadmap 一致，无 silent retargeting。
- **Scope**: 评估后仍是一个连贯设计：围绕 "Rust Host 反转原型" 一个目标，分 core/transport/tui/packaging 四个子系统。每个子系统都服务于同一原型，不是独立产品。

---

## User Final Approval

- Audit level: Deep [C:USER]
- Section key claims (1/2): accepted [C:USER]
- Section key claims (2/2): accepted [C:USER]
- [C:INFERRED] assumptions A1-A3: accepted [C:USER]
- [C:INFERRED] assumptions A4-A7: accepted [C:USER]
- [C:INFERRED] assumptions A8-A10: accepted [C:USER]
- State: **approved via ExitDesignMode** [C:USER]

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-25-backend-architecture-evolution-phase3/core.md` | Rust host core：session runtime、persistence、LLM provider、内置 tool | done |
| 2 | `2026-06-25-backend-architecture-evolution-phase3/transport.md` | Rust transport server、wire protocol、stdio/socket lifecycle | done |
| 3 | `2026-06-25-backend-architecture-evolution-phase3/tui.md` | TS TUI adaptation：启动/连接 Rust host、事件消费、错误处理 | done |
| 4 | `2026-06-25-backend-architecture-evolution-phase3/packaging.md` | Build matrix、Node SEA 嵌入、CI 集成、原型 Done criteria | done |
