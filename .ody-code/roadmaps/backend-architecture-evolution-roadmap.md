# 后端架构演进总路线图（Backend Architecture & Rust Evolution Roadmap）

> **本文合并自两份路线图**:`architecture-refactor-roadmap`(传输边界 + 解耦)与
> `rust-backend-migration-roadmap`(后端 Rust 化)。二者**无硬冲突**,但在 `rpc` 边界上**深度重合**:
> 架构改造的「传输边界激活」恰是 Rust 化终局「Rust Host 反转」赖以站立的**同一块地基**。
> 故合并为「**多条独立轨道 + 一个收敛枢纽(`rpc`)**」的单一主控路线,去重了重复的决策门、纪律与资产判断。
>
> **Document Type**: Master Evolution Roadmap ·  **Last Updated**: 2026-06-24 ·  **Status**: DRAFT(awaiting approval)

---

## 📋 执行摘要

把后端"演进到位"——既补齐架构形态(真 headless Core + 薄客户端),又在该用 Rust 的地方用 Rust——本质是**围绕一个枢纽展开的几条独立工作流**,而不是两套互不相干的计划。

**两块已经押对的地基(作为依赖,不在 TODO 内):**
1. **Provider 抽象层 `kosong` 是真·解耦**——8 家 provider 归一化为 `ThinkingEffort`/`FinishReason`/`StreamedMessage`,catalog 元数据驱动。加 provider = 一文件 + 注册表一行。
2. **`rpc` 双向契约已切开**——`CoreAPI`(客户端→核心)/`SDKAPI`(核心→客户端,含 `emitEvent`/`requestQuestion` 反向推送)是**完整、可序列化的双向消息模型**。
3. **Rust→Wasm→TS 链路已验证**——`rust-ody/` PoC:`estimateTokens` 经 `wasm32-unknown-unknown` 复刻,双轨加载器 + 基准框架跑通(正确性逐字节一致,大输入 ~1.5× 提速)。

**两个待补的问题:**
- **缺口 G(架构):RPC 有契约、无传输。** `createRPC`(`packages/agent-core/src/rpc/client.ts:32`)用**共享闭包**把两端绑死同进程,`simulateNetwork` 只是 `setTimeout(0)+JSON`。已付"可序列化"的税,却没领崩溃隔离 / headless / 远程 / 跨语言客户端的利。
- **隐患 M(架构):`agent-core` 52.5K LOC 上帝包。** agent/tools/mcp/session/profile/skill/rpc/compaction/code-review/office-hours/e2e-testing/cron 全塞一个包。
- **机会 R(性能/形态,Rust):** 计算热点可 Wasm 化提速;而 I/O 核心若要真 Rust 化,唯一彻底路径是**反转宿主**——这与缺口 G 的传输改造**是同一条缝**。

**贯穿全局的硬约束(核心洞察):**
> **「全后端 Wasm 化」物理上不可能。** `wasm32-unknown-unknown` 没有 syscall——不能 spawn/fs/socket/ssh。后端价值最高的核心(`kaos`/`kosong`/`rpc`/大半 `tools`)恰是 **I/O 密集**。所以"尽量 Rust 化"不是一条路,而是**按模块性质三选一**(见下表);而 I/O 核的 Rust 化 = 架构反转 = 依赖传输边界先就位。

**Non-Goals:**
- 不重写 TUI(`apps/ody-code` 34.1K,Ink/React,Rust 化生态倒退)。
- 不为"纯度"把 I/O 叶子硬塞 Wasm(PoC tiny-input 5.3× 变慢已证伪)。
- 不为"远程/微服务"而过度建设——MessagePort 终态已交付绝大部分价值,网络层严格门控。
- 不追求 100% Rust;追求"**该 Rust 的是 Rust,且整体可维护**"。

---

## 🧭 策略分类法（统一 W / N / H 与传输/解耦）

| 策略 | 适用 | 产物 | 保留 SEA 单二进制? | 关系 |
|---|---|---|:---:|---|
| **W — Wasm 模块** | 纯计算叶子(无 I/O) | 一份 `.wasm`,全平台通用 | ✅ 最契合 | 独立于传输改造,可并行 |
| **N — Native Rust(napi-rs)** | I/O 密集但性能敏感(crypto 等) | 每平台一个 `.node` | ⚠️ 每平台编译嵌入 | 独立;主要为量化 SEA 代价 |
| **H — Rust Host(架构反转)** | 整个 I/O 核心 | Rust 二进制做宿主,TS 仅剩 TUI 客户端 | 🔄 二进制故事反转 | **= 传输边界(架构 T1)+ Core 端换 Rust** |
| **传输边界激活** | `rpc` 层本身 | `Transport` 接口 + InProc/MsgPort/Socket | ✅(InProc/MsgPort)🔄(Socket) | **H 的使能地基** |
| **包解耦** | `agent-core` 单体 | 4 个 peer 包 | ✅ 不影响 | 独立,纯工程卫生 |

**收敛关系(本路线图的灵魂)**:
> **H 策略 ≠ 一个新东西。** 「Rust Host」就是把**传输边界激活到 Socket/stdio 那一档之后,Core 进程从 Node worker 换成 Rust 进程**。所以原两份路线里"arch 造 Socket transport"与"rust 做 3A 双进程"是**同一件工程的两面**——传输是管线,Rust 核是接在管线另一端的引擎。合并后只造一次。

---

## 🗺️ 模块盘点与策略归类

> 基线(2026-06-24 实测 LOC,排除测试):agent-core 52.5K / kosong 7.4K / oauth 3.3K / node-sdk 2.3K / kaos 2.3K / telemetry 1.0K;apps/ody-code TUI 34.1K。

| 模块 | LOC | 性质 | 策略 | 说明 |
|---|---:|---|:---:|---|
| `agent-core/utils/tokens` | 1.0K | 纯计算 | **W** | PoC 已证;落地要换**真 BPE**(tiktoken-rs)才有大收益 |
| `code-review/diff` 算法部分 | — | 纯计算 | **W** | similar crate;当前是 `spawn git`,先抽纯算部分 |
| `tools/support` 匹配(glob/rule/path) | ~0.5K | 纯计算 | **W** | Rust glob crate 复刻 picomatch/pathe |
| `session` 序列化/校验 | 5.8K | 计算为主 | **W**(部分) | 编解码/压缩/hash 可 Wasm;落盘留 TS |
| `agent/compaction`+`context` | ~3K | 计算为主 | **W**(部分) | 预算/裁剪/打分纯算;收益依赖分词器 |
| `oauth` 加密/签名 | 3.3K | 计算+少 I/O | **N** | crypto(PKCE/JWT)适合 Rust;网络回调留宿主 |
| `code-review`(整子系统) | — | 混合 | **拆包(T2)** | 剥成 `@odysseythink/code-review` peer 包 |
| `office-hours` | — | 混合 | **拆包(T2)** | 剥成 peer 包(配合 mode 统一) |
| `e2e-testing` | — | 混合 | **拆包(T2)** | 剥成 peer 包 |
| `mcp` 客户端 | 2.8K | I/O 密集 | **拆包(T2)→H** | 先剥 `@odysseythink/mcp-host`;反转后随宿主 |
| `kaos` 执行环境 | 2.3K | **I/O 密集** | **N→H** | spawn/fs/ssh,Wasm 不可行;反转后 Rust 优势最大 |
| `kosong` LLM 抽象 | 7.4K | **I/O 密集** | **H** | reqwest+tokio 流式 SSE,属反转范畴 |
| `rpc` 传输/协议 | 2.2K | **I/O 密集** | **传输 T1 + H** | 先抽 Transport;反转后成 Rust↔TS 边界 |
| `agent` 编排核心 | 13.2K | 编排逻辑 | **H** | 状态机/turn;反转后才谈 Rust 化 |
| `tools/builtin` | 13.5K | I/O 为主 | **N/H** | I/O 封装随宿主;个别解析可拆 W |
| `telemetry`/`node-sdk` | 3.3K | I/O+SDK | 留 TS | 收益低 |
| `apps/ody-code` TUI | 34.1K | 终端渲染 | 留 TS | Non-Goal |

---

## 🛤️ 工作轨道总览（独立可并行 + 一个收敛点）

| 轨道 | 解决 | 核心动作 | 风险 | 与其它轨道关系 |
|---|---|---|---|---|
| **A. Wasm 计算热点(策略 W)** | 机会 R | 真分词器 / diff / glob 搬进 Wasm | 低 | 完全独立 |
| **B. 传输边界激活(旗舰)** | 缺口 G | `Transport` 接口 → InProc → MessagePort → Socket | 中 | **C/D 的 Rust 化 = 接在 B 末端** |
| **C. `agent-core` 解体** | 隐患 M | 边缘能力剥成 peer 包 | 中(量大) | 独立;为 H 减小反转面 |
| **D. mode 概念统一** | 次要混淆 | 厘清 `SessionModeKind` vs `profile` | 低 | 独立;office-hours 拆包前置 |
| **E. Native 模块(策略 N)** | 机会 R | oauth crypto napi-rs + SEA 代价量化 | 中 | 独立;为 H/N 决策供数据 |
| **★ 收敛:Rust Host(策略 H)** | 机会 R 终局 | Core 进程从 Node worker 换 Rust | 高 | **依赖 B 到 Socket 档 + C 已瘦身 + E 的数据** |

A/B/C/D/E **解耦可并行**;★ 是它们的汇流口,只有数据支撑下才启动。

---

## 🚦 统一分阶段路线（含合并后的 Go/No-Go 门）

### Phase 0 — 双地基锁定（前置,~0.5–1 周）

| 编号 | 轨道 | 条目 | 集成路径 |
|---|---|---|---|
| **0.1** | B | 抽 `Transport` 接口(不改实现) | 新增 `rpc/transport.ts`:`{ send(msg); onMessage(cb); close?() }` |
| **0.2** | B | `createRPC` 的 `simulateNetwork` 改为经 transport 收发,默认注入 `InProcessTransport`(逐字节同今日) | `rpc/client.ts` 共享闭包 → 一对内存 transport 互联 |
| **0.3** | B | RPC 契约 golden:录真实会话消息流,断言重构前后序列一致 | `rpc/__tests__/transport-parity.test.ts` |
| **0.4** | A | `rust-ody/` 框架已就绪(✅ 已完成) | `./rust-ody/build.sh` 可复现 |

**门 G0**:`InProcessTransport` 跑通全部测试 + golden parity。契约行为漂移即停修接缝。
> 接缝本质:`createRPC` 现用 `left`/`right` 两个 `createControlledPromise` 在同闭包直接交换 `bindAllFunctions(self)`。改造 = 把"交换函数引用"换成"经 `Transport` 交换序列化消息"。InProc transport 就是两端 `send` 直连对端 `onMessage`(保留 `setTimeout(0)` 维持异步语义)——**零行为变化的安全垫**。

---

### Phase 1 — 并行铺开:Wasm 热点 ∥ MessagePort 边界 ∥ 拆包启动

三条轨道在此并行,互不阻塞。

**1-A｜Wasm 高确定性热点(策略 W,低风险)** — 不改架构、不破 SEA:
1. **真 BPE 分词器(最高价值)**:`tiktoken-rs` 编 Wasm 替换启发式,套 PoC 基准框架。**这是验证"Wasm 值不值"的决定性实验。**
2. **diff/patch**:先把 `code-review/diff.ts` 的 `spawn git` 与纯 diff 解耦,后者用 Rust `similar`。
3. **glob/路径匹配**:`tools/support/{path-glob-match,rule-match}` 用 Rust glob crate。
- **门 G1-A**:真分词器端到端收益 < 阈值(对总延迟贡献 <2%)→ Phase 1-A 收敛到分词器一项即止,不为边际收益铺开。
- 交付:`rust-ody` 多个 Wasm 导出,各走双轨 + 黄金测试;《W 收益基准报告》。

**1-B｜MessagePort 边界(策略 B 核心)** — Core 搬进 `worker_thread`:
1. `MessagePortTransport`:`rpc/transports/message-port.ts`。
2. Core worker 宿主:`node-sdk/src/core-worker.ts`;`SDKRpcClient` 加 `transport: 'inproc'|'worker'`(默认 inproc)。
3. 崩溃语义:worker 异常退出 → 客户端收结构化错误,UI 存活,可重启降级。
4. 反向通道:`emitEvent`/`requestQuestion`/审批流在 MessagePort 等价工作。
5. **不可序列化 payload 审计**:扫 `CoreAPI`/`SDKAPI` 是否有函数/`AbortSignal`/流穿越边界;`AbortSignal` 改 transport 层 `cancel(callId)` 消息。
- **门 G1-B**:worker 边界下全测试 + golden 通过;杀 worker 后 UI 不崩可重连;📊 量化 MessagePort 往返开销(目标 P95 增量 < 单次 LLM 首字节延迟 1%)。达标 → Go Socket;否则 worker 即终态,B 轨在此收官(已拿崩溃隔离这一最大单项收益)。

**1-C｜`agent-core` 拆包启动(策略 C)** — 对外依赖最少者先剥,逐包独立 PR:
- `code-review/` → `@odysseythink/code-review`
- `e2e-testing/` → `@odysseythink/e2e-testing`
- `mcp/` → `@odysseythink/mcp-host`
- **门 G1-C**:每包剥出后全测试绿 + 无循环依赖(`madge`/`dpdm` 校验)。引入环即回滚记债。**只移文件 + 调 import,不重写逻辑。**

---

### Phase 2 — 数据收集层:Socket transport ∥ 首个 Native ∥ mode 统一

**2-B｜网络 transport(策略 B,门控,仅 G1-B=Go)**:
1. `StreamTransport`(stdio/socket,newline-delimited 或 length-prefixed)。
2. headless `ody serve`:`apps/ody-code/src/cli/` 新增子命令,启纯 Core 进程监听 socket/TCP/WS。
3. 鉴权:本地 socket 走 OS 权限;TCP/WS 需 token。
4. 线协议 schema:`scripts/gen-rpc-schema.ts`(`CoreAPI`/`SDKAPI` 类型 → JSON Schema),为跨语言/Rust 客户端铺路。
- **门 G2-B**:最小外部客户端(curl/Python)能跑通"建会话→发 prompt→收事件流";鉴权/连接复杂度超预算 → 退回 MessagePort 终态,网络层留 backlog。

**2-E｜首个 Native 模块(策略 N)**:
- `oauth` crypto(PKCE/JWT/签名)用 napi-rs。**核心目的:实测 SEA 里嵌 per-platform `.node` 的构建矩阵成本**,与 Wasm"一份通用"成文对比。
- **门 G2-E**:对比 W vs N 的"性能增益 ÷ 工程复杂度",产出"后续 I/O 模块走 N 增量还是攒着等 H"的结论。

**2-D｜mode 概念统一(策略 D)**:
- 厘清 `SessionModeKind`(plan/design/office-hours/game-design)与 agent `profile` 的层次:前者是"交互阶段",后者是"角色/工具集/system prompt"。
- 文档化 `docs/architecture/modes-vs-profiles.md`;正交则在类型层收敛耦合点(`SystemPromptContext.sessionMode`),重叠则择一为上层。
- **门 G2-D**:新人能从一页文档说清"加新模式 / 加新角色该动哪个"。
- 完成后解锁 `office-hours` 拆包(1-C 的延后项)。

---

### Phase 3 — ★ 收敛枢纽:是否反转为 Rust Host（策略 H,最重决策门）

> **这是整条路线最重要的决策,是产品方向而非技术细节。** 在此之前一切都是"Node 宿主 + Rust 模块 + 真传输边界",随时可停可回滚;在此之后是不可逆的架构演进。

**前置(必须全部就位)**:
- ✅ B 轨已到 Socket transport(Phase 2-B Go)——**这是 H 的物理地基**;
- ✅ C 轨已瘦身 `agent-core`,缩小反转面;
- ✅ E 轨给出 N vs H 的成本数据(Phase 2-E)。

**问题**:I/O 核心(`kaos`/`kosong`/`rpc`/`agent`)无法 Wasm 化。要让它们"用 Rust",唯一彻底路是反转宿主:
- **现状**:Node 是宿主,TS 跑一切,Wasm/native 是被调库。
- **反转后**:**Rust 二进制做宿主**(tokio + reqwest + 原生进程/fs/ssh),TS 退化为**纯 TUI 客户端**,经 Socket transport 与 Rust 核通信。

**落地形态(本阶段只选型不实现)**:
- **3A 双进程**:Rust 核 + Node TUI,走 Phase 2-B 已造好的 Socket/stdio transport。改动最小、隔离最好。**强烈优先**——它直接复用 B 轨产物,不需要新机制。
- **3B 内嵌 JS 引擎**:Rust 宿主内嵌 deno_core/rusty_v8 跑 TUI。真单二进制,但复杂度/体积显著上升。

**门 G3 — Go/No-Go(合并了原 arch-G2 与 rust-G3)**:产出 ADR,基于实测回答:
- 反转收益(常驻内存 / 冷启动 / 并发 / I/O 核类型安全)是否 > 重写 `kaos`+`kosong`+`rpc` 桥接成本?
- 现有 `rpc` 边界(+ Phase 0 已抽出的 Transport + Phase 2 的线协议 schema)是否足够干净直接当 Rust↔TS 契约?(初判:**是**——core/client 分层 + Transport 抽象 + schema 化,这是反转的最大利好。)
- **若 No-Go:本路线图在此合法收官**,稳定停在"Node 宿主(可 worker 隔离)+ Wasm 计算模块 + 可选 native + 瘦身后的 agent-core"——**这本身就是一个合理、完整的终态**。

---

### Phase 4 — Rust Host 增量迁移（仅当 G3 = Go）

沿 `rpc`/Socket 边界,自底向上迁核,每块保持双实现可切换:
1. **kaos**(执行环境)→ Rust:进程/fs/ssh 用 tokio+std,反转后即时见效、Rust 优势最大(沙箱/资源控制)。
2. **kosong**(LLM 层)→ Rust:reqwest + 流式 SSE,provider 适配逐个搬。
3. **agent** 编排核心 → Rust:状态机/turn/compaction 调度。
4. **tools/builtin** 逐个搬:纯 I/O 直接 Rust,含计算的复用 Phase 1-A 的 Wasm。
5. TUI 保持 TS,仅维护 Socket transport 客户端契约。
- **终态**:后端核心 Rust;TUI TypeScript;二者经 Socket transport 解耦。这就是"尽量换成 Rust"的现实最优解。
- **执行模式标注**:Phase 4 每个子阶段在子路线图标 `[normal]`/`[plan]`/`[design]`,统一判定标准见 `backend-architecture-evolution-phase4-rust-host-migration-roadmap.md` §3.0 Rubric。核心三条:① **模式由「决策不确定性」决定,不由风险等级**(高风险但有成熟前置层的机械迁移仍是 normal);② 奠基子阶段(`.0`)与跨模块集成门一律 **plan**;③ 难回滚的契约/数据模型(records WAL schema、CoreAPI/SDKAPI 冻结)走 **plan + design-lite 决策门**。

---

## 🛡️ 贯穿全程的工程纪律（两份去重合并）

1. **契约冻结**:`CoreAPI`/`SDKAPI` 方法签名与 payload 形状是合同,改造只发生在 `Transport` 之下。改契约须单独提案、单独评审,不与传输/拆包/Rust 改动混 PR。
2. **双轨 + 安全回退**:每个 Rust 模块提供"加载失败回退原 TS"开关(见 `rust-ody/ts/wasm-tokens.ts`);每个 transport 保留 `inproc` 默认值。迁移期生产永远可降级。
3. **黄金测试 / golden parity 优先**:Rust 输出与原 TS **逐字节/逐值一致**;每种 transport / 每个剥离包先让既有测试 + RPC 消息流逐字节同结果,再谈优化。
4. **先 profile 再迁移**:每个候选先量化端到端占比,**禁止凭感觉**。G1/G2/G3 全是数据门。
5. **基准即交付物**:复用 `rust-ody/ts/bench.ts` 方法论(warmup + `hrtime.bigint` + 多尺寸),每个 Rust 迁移项 / 每种 transport 附短/中/长三档对比,显式记录边界税。
6. **可回滚粒度 = 单单元**:一个 transport / 一个剥离包 / 一个 Rust 模块 = 一个独立 PR,出问题只回滚该单元(改一行 import 切回 TS)。
7. **依赖图守护**:拆包后 CI 加包间依赖环检测,防"拆了又粘回去"。
8. **稳定即删旧实现**:迁移项一旦稳定即删 TS 双份,不长期养双实现。

---

## 📊 成功度量

| 维度 | 现状 | Phase 1 后 | Phase 2 后 | Phase 4 后(若 Go) |
|---|---|---|---|---|
| 崩溃隔离 | Core 崩=整 CLI 崩 | worker 崩可恢复,UI 存活 | + 远程 Core 断连可重连 | Rust 核独立崩溃域 |
| Headless | 无(绑死 TUI) | Core 可独立 worker 运行 | `ody serve` 网络 headless | Rust headless server |
| 客户端多样性 | 仅内置 TUI | TS 可换壳 | 跨语言/远程可接入 | Rust 核 + 任意客户端 |
| `agent-core` LOC | 52.5K 单体 | 边缘进 peer 包 | 核心 < 35K | 核心迁 Rust |
| W 收益 | — | 真分词器端到端占比(G1-A ≥2% 才续) | — | — |
| N 代价 | — | — | `.node` SEA 构建矩阵增量(G2-E 成文) | — |
| H 收益 | — | — | — | 常驻内存/冷启动/空闲 CPU(G3 ADR 基线) |
| 正确性 | — | 黄金测试 100%(硬门) | 同左 | 同左 |
| 稳定性 | — | 迁移项生产回退率 ≈0 | 同左 | 同左 |

---

## ⚠️ 风险与应对（合并去重）

| 编号 | 风险 | 应对 |
|---|---|---|
| **R1 收益证伪** | agent 网络瓶颈主导,纯计算 Rust 化端到端无感 | G1-A 提前止损,不铺开 W |
| **R2 不可序列化 payload** | 函数/`AbortSignal`/流在 InProc"碰巧能用",换 transport 即炸 | Phase 1-B.5 专项审计;`AbortSignal`→`cancel(callId)`;CI 加 JSON-safe 断言 |
| **R3 传输延迟** | MessagePort/Socket 往返拖慢交互 | G1-B 用数据卡阈值;LLM 首字节 >> RPC 往返,预期可忽略但须证伪 |
| **R4 SEA × native 张力** | N 破坏"一份产物全平台",构建复杂度升 | G2-E 量化,优先 W;N 仅用于确有刚需处 |
| **R5 拆包牵出循环依赖** | T2 工程量爆炸 | 按对外依赖最少排序,逐包独立 PR;依赖图工具前置探测;卡住可单独搁置不阻塞 B |
| **R6 架构反转不可逆** | Phase 3/4 大工程 | 干净的 `rpc`+Transport+schema 做契约;3A 双进程优先;G3 允许直接 No-Go |
| **R7 过度建设(YAGNI)** | 为"远程/微服务"过早铺网络层 | Phase 2-B 严格门控:G1-B 不 Go 就不做;MessagePort 终态已交付 80% 价值 |
| **R8 双实现维护负担** | 迁移期 TS+Rust 并存 | 稳定即删 TS;不长期养双份 |
| **R9 团队 Rust 能力** | 全栈 Rust 需储备 | W/N 阶段兼作能力爬坡,H 启动前确保人手 |

---

## 🧭 一句话总览

> **地基押对了**(provider 抽象 + 双向 RPC 契约 + 已验证的 Rust→Wasm 链路),演进不是推倒重来,而是**几条独立轨道汇向 `rpc` 一个枢纽**:
> Phase 0 抽 `Transport` 接口(零行为安全垫)→ Phase 1 并行干三件事(真分词器验证 Wasm 值不值 ∥ MessagePort 拿崩溃隔离与 headless ∥ 拆 agent-core)→ Phase 2 按数据决定是否上 Socket、量化 native 的 SEA 代价、统一 mode → Phase 3 在数据支撑下做最重决策"是否反转为 Rust Host"(传输边界正是它的地基)→ Phase 4 仅当 Go 才沿 Socket 自底向上迁核。
> **每个箭头都是带数据门的 Go/No-Go,可在 MessagePort 终态或 G3 No-Go 处合法收官**——不为远程而远程,不为 Rust 而 Rust,不为微服务而拆包。
