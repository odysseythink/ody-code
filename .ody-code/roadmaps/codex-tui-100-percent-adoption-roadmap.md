# 100% 采用 codex-rust TUI 路线图（Codex TUI Full Adoption Roadmap）

**Document Type**: Migration / Integration Roadmap
**Last Updated**: 2026-06-29
**Status**: DRAFT（待评审）
**目标**：让 ody-code 的终端用户界面 **100% 使用** `~/Downloads/codex-rust-v0.142.3` 的 Rust TUI（`codex-tui`），由 ody 自己的智能体后端（`rust-ody/agent-rs` + `kosong-rs` + `kaos-rs`）驱动。
**前置依赖**：Phase 4 Rust Host 迁移（`backend-architecture-evolution-phase4-rust-host-migration-roadmap.md`）—— 复用其中 `agent-rs` 的智能体核心（`TurnFlow`、`agent_loop`、compaction、cron、background、records）。
**Pinned 版本**：codex-rust **v0.142.3**（Apache-2.0，许可允许构建/二次分发，需保留 NOTICE/署名）。

---

## 📋 执行摘要

### 核心洞察：seam 在哪里

codex TUI 与其后端之间存在一条**干净的协议接缝**。TUI 通过 `codex_app_server_client::AppServerClient` 连接后端，有两种传输：

1. **Embedded（in-process）**：在 TUI 进程内启动 codex-core（`InProcessAppServerClient::start` → `codex_app_server::in_process::start`），静态链接整套 codex 运行时。**不可用于 ody**（会把 codex 大脑塞回来）。
2. **Remote**：纯 JSON-RPC（`app-server-protocol`），以 **WebSocket text 帧**承载，跑在 **TCP（`ws://`/`wss://`）或本地 Unix socket（`unix://PATH`）** 上。握手 URI 固定 `ws://localhost/rpc`，先做 `Initialize/initialized`，随后双向收发 `JSONRPCMessage`（`app-server-client/src/remote.rs`）。

> 结论：**只要有一个进程在 Unix socket 上说 codex 的 `app-server-protocol`，就能用 `codex --remote unix://PATH` 让 TUI 100% 原样连上**，无需改 TUI 一行代码。这正是"100% 使用 codex TUI"的最低成本路径。

### 当前差距（gap）

| 维度 | codex TUI 期望 | ody 现状 |
|------|----------------|----------|
| 协议 | `app-server-protocol`（JSON-RPC：`thread/start`、`turn/start`、`item/*` 事件…）约 **90 客户端请求 / 11 服务端审批请求 / 75 通知** | `CoreAPI/SDKAPI` 自定义 `{method,args}` 封装，**形状完全不同** |
| 传输 | WebSocket 帧 over Unix/TCP，tokio-tungstenite | `ody-host` 有 stdio/uds/tcp 但是裸 JSON `{ok,value,error}`，非 JSON-RPC、非 WS |
| 后端 | 由 `thread/start`→`turn/start` 驱动一次 agent turn，流式回 `item/*` | `agent-rs` 的 `TurnFlow` 已实现，但**只被 `src/bin/*_l3.rs` 测试驱动**调用，未接任何 server |
| 前端 | codex Rust TUI（`@ ~/Downloads/...`） | ody 自己的 TS pi-tui（`apps/ody-code/src/tui/`） |

### 推荐方案：**Adapter（ody-app-server）**

新建一个 Rust 后端进程 **`ody-app-server`**（rust-ody 新 crate），它：
1. 在 Unix socket 上实现 codex `app-server-protocol` 的**服务端**（接受 WS 升级 → JSON-RPC）；
2. 实现 TUI 启动握手所需的**最小请求子集**（先用 stub 让 TUI 能起来）；
3. 收到 `thread/start` + `turn/start` 时，驱动 `agent-rs::TurnFlow`（接 `kosong-rs` LLM、`kaos-rs` 执行、tools）；
4. 把 `agent-rs` 的事件流**翻译**成 codex `ServerNotification`（`item/*`、`turn/*`、`thread/tokenUsage/updated`）；
5. 用 codex `ServerRequest`（`item/commandExecution/requestApproval` 等）对接 `agent-rs` 的 permission 系统。

启动方式：`ody` 包装命令先拉起 `ody-app-server`（监听 socket），再以 `codex-tui --remote unix://<socket>` 连上。**TUI 全程原样使用。**

> 备选方案见 §7。简言之：Approach B（fork TUI 改连 ody RPC）破坏"100% 原样使用"且要继承 codex-core 耦合，否决；Approach C（让 `agent-rs` 原生实现 app-server 协议）把 codex 协议类型耦合进核心库，作为 A 的长线收敛形态可选。

### Non-Goals

- 不实现 codex 协议的全部 ~90 请求面；只实现 TUI 实际调用的子集（Phase 1 实测确定）。
- 不支持 codex 的云/远程 workspace、realtime 语音、windows sandbox、plugin marketplace、external-agent 迁移等高级面（启动期用空响应 stub 化）。
- 不改 codex TUI 源码（除非 Phase 0 验证发现 `--remote` 路径有阻断性本地依赖，才允许最小 patch，见风险 R1）。
- 不替换 ody 现有 TS TUI 的发布通道；本路线图是**新增**一条"codex TUI 前端"，二者可并存。

---

## 🗺️ 阶段总览

| Phase | 名称 | 目标 | 产出 | 依赖 |
|-------|------|------|------|------|
| **P0** | 可行性验证（spike） | 证明 TUI 能连上一个"哑"socket 并完成握手 | 一次性 spike + 决策备忘 | 能构建 codex-tui |
| **P1** | 协议抓取与最小子集 | 实测 TUI 启动/一次 turn 实际发的请求与期望事件 | 抓包日志 + 最小子集清单 | P0 |
| **P2** | 传输层（WS-over-UDS + JSON-RPC 服务端） | `ody-app-server` 能握手、路由请求/通知/服务端请求 | `ody-app-server` crate 骨架 | P1 |
| **P3** | 启动握手子集（stub） | TUI 能完整启动到空会话界面 | Initialize/config/model/account/thread 列表等 stub | P2 |
| **P4** | 核心 turn 闭环 | 输入一句话 → 跑通 agent turn → 流式渲染回 TUI | `thread/start`+`turn/start` → `agent-rs::TurnFlow` → `item/*` 事件 | P3 + agent-rs 可独立运行 |
| **P5** | 审批与权限 | 命令执行/改文件的审批弹窗在 TUI 里工作 | `ServerRequest` 审批 ↔ agent-rs permission | P4 |
| **P6** | 事件/item 映射保真 | agent message / reasoning / exec / patch / plan / MCP 全量正确渲染 | 事件映射表 + 渲染保真 | P4 |
| **P7** | 会话持久化与恢复 | `thread/list`/`thread/read`/`thread/resume` 接 agent-rs records | 历史列表、resume、fork | P4 + records |
| **P8** | 配置与模型面 | 设置/模型切换 UI 反映 ody 真实配置 | `config/read`+`model/list` 映射 ody 配置 | P3 |
| **P9** | 一致性与回归测试 | 协议级 golden + 真 TUI 无头集成测试 | conformance 套件接入 parity 纪律 | P2–P8 |
| **P10** | 打包与启动编排 | `ody` 一条命令拉起 TUI+后端；CODEX_HOME 供给；版本钉死 | 启动器 + 构建脚本 + 文档 | 全部 |

> 关键路径：**P0 → P1 → P2 → P3 → P4** 是 MVP（"能用 codex TUI 和 ody 对话一次"）。P5–P8 把体验补齐，P9/P10 工程化。

---

## 🔬 Phase 0 — 可行性验证（spike，~2–3 天）

**目的**：在投入前，证明"用 `--remote unix://` 连一个我们自己的 socket"这条路真的通，并测出 TUI 在 remote 模式下对本地 `~/.codex` 的硬依赖边界。

**任务**
- [ ] P0.1 在 v0.142.3 源码树构建出可运行的 `codex-tui` 二进制：`cargo build -p codex-tui --bin codex-tui`（评估构建耗时与依赖体量；TUI 直接/间接依赖 ~40 个 codex crate）。
- [ ] P0.2 写一个 30 行的 Rust spike server：在 Unix socket 上 `accept` → tokio-tungstenite `accept_async` 完成 WS 升级 → 收到 `Initialize` 回一个 `InitializeResponse` → 收到 `initialized` 通知 → 之后对任何请求回最小空响应。
- [ ] P0.3 `codex-tui --remote unix://<socket>` 连接，观察：能否进入空会话界面？还是因为缺 `config/read`/`model/list` 等响应而崩/卡？记录第一个阻断点。
- [ ] P0.4 **关键风险测量**：TUI 即使 remote 也会调用 `legacy_core::config::load_config_toml_with_layer_stack` 和 `find_codex_home`（`tui/src/lib.rs:11,63,349`）。测一个**空/最小 CODEX_HOME** 能否让 TUI 启动；记录它本地读哪些文件（config.toml、auth、模型 catalog…）。`uses_remote_workspace()` 会关掉一部分本地配置加载（`lib.rs:756,773,777`），实测其覆盖范围。

**验收**：能给出一句话结论——"remote 路径 + 最小 CODEX_HOME 下，TUI 至少能完成握手并进入界面，阻断点是 X"。若发现必须改 TUI 源码才能跑，则在此决定是否接受"最小 patch + 维护 fork"（风险 R1）。

---

## 🔍 Phase 1 — 协议抓取与最小子集（~3–4 天）

**目的**：不靠猜，**实测**codex TUI 在「启动 → 输入一句话 → 收到回复 → 一次命令审批」全流程里实际发出的请求序列与它期望的事件，得到必须实现的**最小协议子集**清单。

**任务**
- [ ] P1.1 把 P0 的 spike server 升级为**录制代理**：要么 (a) 让它把收到的每条 `JSONRPCMessage` 落盘；要么 (b) 用真实 codex embedded 后端 + 中间人 socket 代理，录制 TUI↔真后端的完整双向报文。后者能同时拿到"正确的响应/事件形状"作为黄金样本。
- [ ] P1.2 归类抓到的**客户端请求**（预期含：`Initialize`、`config/read`、`model/list`、`account/read`、`account/rateLimits/read`、`experimentalFeature/list`、`permissionProfile/list`、`collaborationMode/list`、`skills/list`、`plugin/list`/`plugin/installed`、`thread/list`、`thread/loaded/list`，turn 期：`thread/start`、`turn/start`、`turn/steer`、`turn/interrupt`）。以实测为准。
- [ ] P1.3 归类**服务端→客户端请求（审批）**：`item/commandExecution/requestApproval`、`item/fileChange/requestApproval`、`item/permissions/requestApproval`、`item/tool/requestUserInput`、`currentTime/read` 等——实测哪些是必答。
- [ ] P1.4 归类**通知/事件**：`thread/started`、`turn/{started,completed}`、`item/{started,completed}`、`item/agentMessage/delta`、`item/reasoning/*`、`item/commandExecution/outputDelta`、`item/fileChange/*`、`turn/plan/updated`、`thread/tokenUsage/updated`、`error`/`warning`。
- [ ] P1.5 产出 `docs/codex-protocol-subset.md`：三张表（必须实现 / stub 即可 / 完全忽略），每条标注实测来源报文。**这是 P3–P6 的合同。**

**验收**：一份带真实报文样本的最小子集清单，明确"MVP 必须答 N 个请求、发 M 类事件"。

---

## 🔌 Phase 2 — 传输层：WS-over-UDS + JSON-RPC 服务端（~1 周）

**目的**：`ody-app-server` 能在 Unix socket 上正确地说 codex 的传输协议——这是纯机械但必须 100% 对齐的一层。

**任务**
- [ ] P2.1 新建 crate `rust-ody/crates/ody-app-server`（或在 `ody-host` 下新增 transport 变体；推荐独立 crate 以隔离 codex 协议类型）。
- [ ] P2.2 **协议类型来源决策**：（a）`vendor` codex 的 `app-server-protocol` crate（Apache-2.0，钉 v0.142.3）以保证 100% 形状一致；或（b）手写 serde 结构对齐线格式。**推荐 (a)**——直接依赖 vendored `codex-app-server-protocol`，零形状漂移风险。在 `docs/codex-protocol-subset.md` 记录 vendoring commit。
- [ ] P2.3 实现 server 端传输：Unix socket `listen/accept` → tokio-tungstenite `accept_async`（对端会以 `ws://localhost/rpc` 升级）→ 读 `Message::Text` 解析 `JSONRPCMessage`，分派 Request/Notification/Response/Error；写回同构。复用 `app-server-client/src/remote.rs:198-440` 的对端逻辑作镜像参考。
- [ ] P2.4 实现 `Initialize`→`InitializeResponse` 握手 + 等待 `initialized` 通知后才进入就绪态。把 server 能力（capabilities）按 TUI 期望填好（实测自 P1）。
- [ ] P2.5 实现**服务端→客户端请求**通道（带 `RequestId` 关联、超时、`serverRequest/resolved`）——审批要用。
- [ ] P2.6 单元测试：用 vendored 的 `app-server-client`（RemoteAppServerClient）做测试客户端，连本 server，跑通 initialize + 一个 echo 请求 + 一条通知（**自洽 round-trip 测试**，不依赖真 TUI）。

**验收**：`app-server-client` 能连上 `ody-app-server` 完成握手并双向收发；round-trip 测试绿。

---

## 🚪 Phase 3 — 启动握手子集（stub）（~1 周）

**目的**：让真 codex TUI **完整启动到一个空会话界面**，所有启动期请求都有合规响应（先用 stub/空值，不接真实数据）。

**任务**
- [ ] P3.1 按 P1.5 清单，为每个启动期请求实现 handler，返回**合规但最小**的响应：
  - `config/read` → ody 默认配置投影成 codex config 形状（最小字段）
  - `model/list` → 返回 ody 当前可用模型（至少一个，含 id/显示名/能力）
  - `account/read` / `account/rateLimits/read` → 已登录/无限额的最小账户对象（绕过 codex 登录 UI）
  - `experimentalFeature/list`、`permissionProfile/list`、`collaborationMode/list`、`skills/list`、`plugin/list`/`installed` → 空数组或 ody 最小集
  - `thread/list`、`thread/loaded/list` → 空列表（P7 再接真历史）
- [ ] P3.2 处理 TUI 启动期发的**无关请求**：一律给"成功但空"的合规响应，避免 TUI 报错弹窗。
- [ ] P3.3 准备**最小 CODEX_HOME 模板**（P0.4 的结论落地）：一个 `config.toml` + 伪 auth，让 TUI 本地配置层加载通过。由启动器在临时目录生成（P10）。
- [ ] P3.4 端到端手测：`codex-tui --remote unix://...` 起到空界面，无报错、无登录拦截、能看到模型名。

**验收**：真 codex TUI 稳定进入空会话界面，光标可输入，不崩不卡。**这是第一个"看得见"的里程碑。**

---

## 🔁 Phase 4 — 核心 turn 闭环（MVP，~1.5–2 周）

**目的**：在 TUI 里输入一句话 → 驱动 `agent-rs` 跑一次 turn → 流式把回复渲染回 TUI。**这是整条路线的 MVP 终点。**

**前置**
- [ ] P4.0 让 `agent-rs` 能在 server 进程内真实运行一次 turn（目前只被 `*_l3.rs` 测试驱动调用）：接 `kosong-rs` 真 LLM provider（OpenAI/Kimi）、`kaos-rs` 执行、注册基础 tools。复用 Phase 4 Rust Host 迁移成果；若 `ody-host` 已能跑真 turn，则把 `ody-app-server` 建在其上。

**任务**
- [ ] P4.1 `thread/start` → 在 `agent-rs` 建一个 session/thread，回 `thread/started` 通知（带 threadId）。
- [ ] P4.2 `turn/start`（携带用户消息）→ 启动 `agent-rs::TurnFlow`（`turn/turn_flow.rs:42`），回 `turn/started`。
- [ ] P4.3 **事件桥**：订阅 `TurnFlow`/`agent_loop` 的事件流，逐条翻译为 codex `ServerNotification`：
  - 助手文本增量 → `item/started`(agentMessage) + `item/agentMessage/delta`(多条) + `item/completed`
  - 推理/thinking → `item/reasoning/textDelta` / `summaryTextDelta`
  - turn 结束 → `turn/completed`
  - token 统计 → `thread/tokenUsage/updated`（接 agent-rs usage 模块）
- [ ] P4.4 `turn/interrupt` → 触发 `TurnFlow` 的 abort signal；`turn/steer` → 灌入 steer buffer（`TurnFlow` 已有 steer/abort 能力）。
- [ ] P4.5 错误路径：agent 错误 → `error` 通知，TUI 正确显示。
- [ ] P4.6 端到端手测：输入"你好"，看到流式回复；输入到一半 Esc 打断生效。

**验收**：在 codex TUI 里与 ody 后端完成**一次完整对话**（含流式与打断）。MVP 达成。

---

## 🛡️ Phase 5 — 审批与权限（~1 周）

**目的**：当 agent 要执行命令或改文件时，codex TUI 弹出审批，用户的批准/拒绝正确回传 agent-rs。

**任务**
- [ ] P5.1 命令执行：agent-rs 请求执行 → 发 `item/commandExecution/requestApproval`（ServerRequest）→ 等 TUI 用户响应 → 映射回 agent-rs permission 决策（允许/拒绝/记住）。
- [ ] P5.2 改文件：`item/fileChange/requestApproval` 同上；patch 内容映射成 codex 期望的 diff 形状（参考 `apply-patch`/`diff_render`）。
- [ ] P5.3 升级权限：`item/permissions/requestApproval` ↔ agent-rs permission 等级。
- [ ] P5.4 执行流式输出：命令 stdout/stderr → `item/commandExecution/outputDelta`，完成 → `item/completed`。
- [ ] P5.5 对接 agent-rs 的 permission 模块（`permission/`）的"记住选择/自动批准"语义，确保与 codex auto-approval 体验一致。

**验收**：在 TUI 里跑一条会触发审批的指令（如写文件 / 运行命令），弹窗→批准→执行→输出流式回显全链路正确。

---

## 🎨 Phase 6 — 事件/item 映射保真（~1.5 周）

**目的**：把 agent-rs 的所有事件类型完整、正确地映射到 codex item 模型，使 TUI 渲染与"原生 codex"无差别。

**任务**
- [ ] P6.1 建立**事件映射表** `docs/event-mapping.md`：agent-rs 事件 ↔ codex `ServerNotification`，逐项对齐（参考 codex 自身的 `app-server-protocol/src/protocol/event_mapping.rs` 作为目标形状权威）。
- [ ] P6.2 覆盖：agent message（含 markdown）、reasoning summary、plan 更新（`turn/plan/updated` / `item/plan/delta`）、命令执行、文件改动 patch、MCP 工具调用进度（`item/mcpToolCall/progress`）、diff 更新（`turn/diff/updated`）。
- [ ] P6.3 处理 item 生命周期一致性：每个 `item/started` 必须有对应 `item/completed`；id 稳定且单调，避免 TUI 渲染错乱。
- [ ] P6.4 多语言/markdown 渲染抽样核对（中文、代码块、表格）。
- [ ] P6.5 缺口标注：agent-rs 有但 codex 协议无对应概念的事件（如某些 ody 专有遥测）→ 决定丢弃或塞进 `warning`/自定义字段。

**验收**：一组代表性会话（纯对话 / 带命令 / 带改文件 / 带 plan / 带 MCP）在 TUI 中渲染与预期逐项一致。

---

## 💾 Phase 7 — 会话持久化与恢复（~1 周）

**目的**：历史会话列表、打开、resume、fork 在 TUI 中可用，接 agent-rs 的 records/persist。

**任务**
- [ ] P7.1 `thread/list`/`thread/search` → 查询 agent-rs records 存储（`records/`、`persist/`），投影成 codex thread 摘要。
- [ ] P7.2 `thread/read` → 取某 thread 的历史 item，按 codex item 形状回放（注意与 P6 映射一致）。
- [ ] P7.3 `thread/resume`/`thread/fork` → 在 agent-rs 重建会话上下文继续。
- [ ] P7.4 `thread/{archive,unarchive,delete}`、`thread/name/set` → 接 records 元数据。
- [ ] P7.5 与 ody 现有 TS 会话存储的**互通性决策**：codex TUI 写的会话能否被 TS 端读，反之亦然？（推荐：MVP 阶段两套存储各自独立，标注为已知限制）。

**验收**：重启 TUI 后能看到并打开上次会话，resume 后上下文连续。

---

## ⚙️ Phase 8 — 配置与模型面（~3–5 天）

**目的**：TUI 的设置面板、模型切换器反映 ody 的真实配置而非 stub。

**任务**
- [ ] P8.1 `config/read`/`config/value/write`/`config/batchWrite` → 读写 ody 配置（双向）。
- [ ] P8.2 `model/list` + `modelProvider/capabilities/read` → 映射 ody/kosong 的真实模型与能力（上下文窗口、推理、视觉等）。
- [ ] P8.3 turn 内模型/thinking 切换 → 传给 `TurnFlow`（agent-rs 已有 setModel/setThinking 语义，参考 `ody-host` dispatch）。
- [ ] P8.4 `account/usage/read` / `account/rateLimits/read` → 接 agent-rs usage（或合理 stub）。

**验收**：在 TUI 里切换模型、改配置生效并持久化。

---

## ✅ Phase 9 — 一致性与回归测试（贯穿，集中收口 ~1 周）

**目的**：把"协议正确性"纳入项目既有的 **TS↔Rust parity 纪律**（见 phase4 roadmap §parity），防止 codex 升级或 agent-rs 改动悄悄破坏协议。

**任务**
- [ ] P9.1 **协议 golden 测试**（类 L3）：给定一段确定性脚本（输入→事件），断言 `ody-app-server` 产出的 `JSONRPCMessage` 序列与黄金样本逐字段相等（归一化非确定性：id、时间戳、socket 路径）。复用项目现有 `packages/integration-tests/test/parity/` 的 normalize/driver 模式。
- [ ] P9.2 **conformance 测试**：用 vendored `app-server-client` 作为程序化客户端，跑 initialize→turn→approval 全流程，断言响应/事件 schema 合规。
- [ ] P9.3 **真 TUI 无头集成测试**：在 CI 里用 pty（如 `expect`/`portable-pty`）跑真 `codex-tui --remote`，发一条消息，断言渲染输出含预期文本。门控为 nightly/可选（重）。
- [ ] P9.4 **codex 版本漂移守护**：记录 v0.142.3 的协议 schema 指纹；vendored 协议 crate 升级时跑 diff，破坏性变更显式评审。
- [ ] P9.5 接入 CI（`.github/workflows/rust-host.yml` 已有 Rust 流水线，扩展之）。

**验收**：协议 golden + conformance 进 CI 必跑；TUI 无头冒烟进 nightly。

---

## 📦 Phase 10 — 打包与启动编排（~1 周）

**目的**：用户一条 `ody`（或 `ody --tui=codex`）命令，无感地拉起后端 + codex TUI。

**任务**
- [ ] P10.1 **codex-tui 二进制来源决策**：(a) 从 vendored v0.142.3 源码树构建并随 ody 分发；或 (b) 要求用户本地装 codex。**推荐 (a)**：钉版本、可控、离线可用；代价是构建/分发体量（评估 P0.1 的构建成本）。
- [ ] P10.2 **启动器**：一个 ody 子命令/脚本：生成临时 socket 路径 → 拉起 `ody-app-server` → 等 socket 就绪 → 生成最小 CODEX_HOME（P3.3）→ `exec codex-tui --remote unix://<socket>` → TUI 退出时清理后端与临时文件。
- [ ] P10.3 生命周期/健壮性：后端崩溃 → TUI 收到连接断开并友好退出；信号转发（Ctrl-C）；僵尸进程清理。
- [ ] P10.4 与现有 `apps/ody-code` 启动路径集成：新增"codex 前端"开关，不破坏现有 TS pi-tui 路径（二者并存）。
- [ ] P10.5 许可与署名：随分发包含 codex 的 Apache-2.0 LICENSE/NOTICE；在 ody 文档标注内嵌 codex-rust v0.142.3。
- [ ] P10.6 用户文档：如何启用、已知限制、支持/不支持的功能矩阵。

**验收**：干净环境下 `ody <codex-tui-cmd>` 一键进入由 ody 驱动的 codex TUI，退出干净无残留。

---

## ⚠️ 风险与缓解

| ID | 风险 | 影响 | 缓解 |
|----|------|------|------|
| **R1** | 即使 remote，TUI 仍调用 `legacy_core::config` / `find_codex_home`，可能对本地 codex 环境有硬依赖（`tui/src/lib.rs:11,63,349`） | 高：可能无法做到"零改动 TUI" | P0.4 优先实测；准备最小 CODEX_HOME 模板兜底；万不得已接受最小 patch + 维护 fork（破坏"100% 原样"，需用户拍板） |
| **R2** | 协议面大（~90 请求 / 75 事件），全实现成本高 | 中 | 只实现 P1 实测子集；其余 stub；明确 Non-Goals |
| **R3** | codex 协议随版本演进，钉死 v0.142.3 后升级困难 | 中 | vendoring + 版本指纹守护（P9.4）；升级走显式评审 |
| **R4** | `agent-rs` 事件模型与 codex item 模型语义不完全对得上 | 中 | P6 建映射表，缺口显式标注/降级；以 codex `event_mapping.rs` 为目标权威 |
| **R5** | `agent-rs` 当前未接入任何 server，只被测试驱动调用 | 高（P4 前置） | P4.0 复用 Phase 4 Rust Host 迁移；若 `ody-host` 已能跑真 turn，则 `ody-app-server` 复用之 |
| **R6** | 构建整个 codex-tui（~40 crate）体量大、CI 慢 | 中 | P0.1 量化；考虑只构建 tui bin、缓存 cargo、预编译产物分发 |
| **R7** | 两套会话存储（codex/ody）不互通 | 低 | MVP 标为已知限制（P7.5）；后续可做导入桥 |
| **R8** | codex TUI 的高级面（云 workspace/realtime/marketplace）在 ody 无对应后端 | 低 | 启动期 stub 空响应；UI 入口存在但功能降级，文档说明 |

---

## 🧭 备选方案（为何选 Adapter）

| 方案 | 做法 | 是否"100% 用 codex TUI" | 评价 |
|------|------|------------------------|------|
| **A. Adapter（推荐）** | 新建 `ody-app-server` 说 codex 协议；TUI 用 `--remote unix://` 原样连 | ✅ TUI 零改动 | 接缝干净、风险可控、可增量；唯一不确定是 R1（本地配置层依赖），P0 先验 |
| **B. Fork TUI** | 把 TUI 里的 `app-server-client` 调用换成 ody 的 CoreAPI RPC | ❌ 改了 TUI，且要继承 codex-core 的 config/login/models/skills/plugins 耦合 | 工程量大、维护 fork 重、违背"原样使用"，**否决** |
| **C. agent-rs 原生协议** | 让 `agent-rs` 直接实现 app-server 协议服务端（不经 adapter 翻译） | ✅ TUI 零改动 | 把 codex 协议类型耦合进核心库，污染 parity 纪律；可作为 A 成熟后的收敛形态，**非首选** |

> 选 A 的核心理由：codex 在 `app-server-client` 里**已经把 Remote 传输做成与 in-process 等价的一等公民**（`remote.rs` 注释明示 callers 如 TUI 可在两种传输间无缝切换），意味着官方已支持"换后端"。我们顺着这条官方接缝走，成本最低、最稳。

---

## 📅 指示性时间线（单人，粗估）

- **里程碑 1（MVP，~4–5 周）**：P0→P1→P2→P3→P4 —— 能在 codex TUI 里与 ody 对话一次（流式 + 打断）。
- **里程碑 2（可用，~+3–4 周）**：P5→P6→P7→P8 —— 审批、保真渲染、历史、配置/模型齐活。
- **里程碑 3（工程化，~+2 周）**：P9→P10 —— 测试纪律 + 一键启动 + 分发。
- 合计约 **9–11 工程周**（指示性，受 R1/R5/R6 影响波动较大；P0 后应重估）。

---

## ✅ 总验收标准（Definition of Done）

1. 用户运行一条 ody 命令即进入 **codex-rust v0.142.3 原版 TUI**（二进制零源码改动，或仅最小且记录在案的 patch）。
2. 该 TUI 的对话、流式、打断、命令/文件审批、plan、历史会话、模型切换，**全部由 ody 的 `agent-rs` 后端驱动**，无任何 codex-core 大脑参与。
3. 协议 golden + conformance 测试进 CI 必跑，纳入项目 parity 纪律。
4. 退出后无残留进程/临时文件；许可署名合规。
5. 不支持项在文档中以功能矩阵明确列出。

---

## 🔗 关键代码参照（实现时的锚点）

**codex 侧（~/Downloads/codex-rust-v0.142.3/codex-rs/）**
- 连接/传输：`app-server-client/src/remote.rs`（WS-over-UDS、握手、JSON-RPC 路由——服务端镜像参考）
- TUI 连接分派：`tui/src/lib.rs:260-478`（AppServerTarget / start_app_server）
- 本地配置依赖：`tui/src/lib.rs:11,63,349,756,773,777`（R1 重点）
- 协议合同：`app-server-protocol/src/protocol/common.rs`（请求/通知/服务端请求宏定义：`:466`、`:1445`、`:1600`）
- 事件形状权威：`app-server-protocol/src/protocol/event_mapping.rs`

**ody 侧（rust-ody/）**
- 智能体核心：`crates/agent-rs/src/turn/turn_flow.rs:42`（TurnFlow，含 steer/abort）、`crates/agent-rs/src/agent_loop/`
- 测试驱动样例（事件流形态参考）：`crates/agent-rs/src/bin/background_cron_l3.rs`、`*_l3.rs`
- 现有 headless 后端：`crates/ody-host/src/{host.rs,transport/}`（dispatch ~25 CoreAPI 方法——P4.0 复用基座）
- 持久化：`crates/agent-rs/src/{records,persist}/`
- parity 测试范式：`packages/integration-tests/test/parity/`

---

*本路线图聚焦"让 codex TUI 100% 跑在 ody 后端上"。它与 Phase 4 Rust Host 迁移互补：Phase 4 把 ody 大脑搬进 Rust，本路线图给这颗 Rust 大脑换上 codex 的脸。*
