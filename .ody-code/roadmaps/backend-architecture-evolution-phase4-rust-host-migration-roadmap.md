# Phase 4 详细路线图 — Rust Host 增量迁移与 TS↔Rust 对照测试

> **Document Type**: Phase 4 Detailed Execution Roadmap
> **Parent**: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md`(Phase 4 节)
> **Predecessor**: `.ody-code/roadmaps/backend-architecture-evolution-phase3-fixup-roadmap.md`
> **Status**: DRAFT(awaiting approval) · **Last Updated**: 2026-06-29(after 4.3.x full-implementation audit + 4.4.0–4.4.3 completion)
> **Precondition**: **仅当 G3 = Go** 才执行本路线图。No-Go 则停在「Node 宿主 + Wasm 模块 + 瘦身 agent-core」终态。

---

## 0. 本路线图与母路线图的关系

母路线图把 Phase 4 压缩成 5 行(kaos→kosong→agent→tools→TUI 保留)。本文把这 5 行展开为**可执行、带数据门、可回滚到单模块粒度**的详细计划,并把母路线图反复强调但从未具体化的一条纪律——

> **「Rust 输出与原 TS 逐字节/逐值一致」**

——升级为本阶段的**第一公民:TS↔Rust 对照测试(differential / parity testing)框架**。母路线图所有 Go/No-Go 都是数据门;Phase 4 的数据门**必须**由对照测试供数。

**核心架构利好(为什么对照测试在 Phase 4 几乎零成本)**:
经 Phase 0–3,TS Core 与 Rust Host **说同一套 RPC 线协议**(`CoreAPI`/`SDKAPI`),且都经同一个 `SDKRpcClient` 驱动:
- TS 后端入口:`packages/node-sdk/src/core-server.ts:23` `createCoreServer(...)`
- Rust 后端入口:`rust-ody/target/release/ody-host`(已在 `packages/node-sdk/test/rust-host-connect.test.ts` 跑通 stdio/uds/tcp)

> **这意味着「对照测试」= 把同一段确定性脚本用同一个客户端分别打到两个后端,逐字段比对响应与事件流。** 两个后端是协议级可互换的黑盒,这是本阶段最大的杠杆。

---

## 1. 当前已迁移面盘点(经 2026-06-29 源码审计)

| 维度 | TS 后端(基线) | Rust Host(`ody-host`,~3.5K LOC + `agent-rs` ~23K LOC + `tools-rs` ~9K LOC) |
|---|---|---|
| 传输 | InProc / MessagePort / Socket(stdio/uds/tcp) | stdio / uds / tcp(`transport/`) |
| Session 生命周期 | `agent-core` session(5.8K) | `session/{store,manager}.rs`(~450 行,内存+落盘) |
| CoreAPI 方法 | 全量(`rpc/core-api.ts:437`) | ~25 个(`host.rs:63-86`):session CRUD / chat / prompt / steer / setModel / setThinking / setPermission / getStatus 组件 / config |
| LLM provider | `kosong` 8 家(7.4K) | mock + openai(`llm/{mock,openai}.rs`,~240 行) |
| 工具 | `tools/builtin` 全量(13.3K) | tools-rs: Read/Write/Edit/Glob/Grep/ReadMedia/Bash/FetchURL/WebSearch + TaskList/TaskOutput/TaskStop/CronCreate/CronList/CronDelete (L1 golden ✓); tool_registry in ody-host |
| agent 核心(4.3) | `agent-core/src/agent/*` (~15.7K) | **全部 10 子阶段已实现** — records/context/config/usage/tool/skill/permission/agent_loop/turn/compaction/session_mode/injection/replay/background/cron + Agent 组装 + CoreHost 集成 |
| 执行环境(kaos) | `kaos` 全量(2.3K,含 ssh 915 行) | 内联在 bash 工具里,无独立抽象 |
| 对照测试 | — | `rust-host-connect.test.ts`(仅 session 生命周期 + mock provider 基本面,**非逐字段对照**) |

**结论**:Phase 3 证明了「协议级可互换」,4.3 agent 核心已全部 Rust 化。4.4 工具已迁移 4.4.0–4.4.3(基础+文件+Web+后台管理)。**当前阶段已具备切换到 Rust 后端的条件,剩余 4.4.4–4.4.8 是增量完善。**

---

## 2. ★ TS↔Rust 对照测试框架(本阶段第一公民,先建后迁)

> **纪律:任何模块的 Rust 实现合入前,必须先有它的对照测试,且对照测试必须先在「TS vs TS」自比对下绿(证明 harness 与归一化本身无 bug),再切「TS vs Rust」。**

### 2.1 四层对照(由内而外,成本递增,覆盖递增)

| 层级 | 名称 | 比对对象 | 输入源 | 用于哪个子阶段的门 |
|---|---|---|---|---|
| **L1** | 模块golden对照 | 纯函数/单操作输出 | 录制 fixture(SSE 流、目录树、文件字节) | 4.1 / 4.2 |
| **L2** | RPC 响应对照 | 单个 `CoreAPI` 方法的 JSON 响应 | 确定性脚本 | 全程 |
| **L3** | 事件流对照 | `SDKAPI.emitEvent` 的**有序**事件序列 | 确定性 prompt + mock provider | 4.3 / 4.4 |
| **L4** | 端到端场景重放 | 完整会话的响应+事件+落盘记录 | 录制的真实会话脚本 | 收官门 G4-final |

### 2.2 dual-driver harness(L2/L3/L4 共用)

新增 `packages/integration-tests/src/parity/`:

```
parity/
  driver.ts          # runScenario(backend, scenario) -> { responses[], events[], records[] }
  backends.ts        # makeTsBackend()  -> createCoreServer 经 InProc transport
                     # makeRustBackend()-> spawn ody-host 经 stdio transport
  normalize.ts       # 抹平非确定性字段(见 2.3)
  scenarios/         # 确定性脚本(JSON/TS),mock provider 驱动
  assert-parity.ts   # 归一化后逐字段深比对,diff 友好报错
```

核心断言:
```ts
const ts   = normalize(await runScenario(makeTsBackend(),   scenario));
const rust = normalize(await runScenario(makeRustBackend(), scenario));
expect(rust).toEqual(ts);          // 逐字段;失败时打印结构化 diff + scenario 名
```

**关键:两个 backend 用同一个 `SDKRpcClient` 类型与同一套 scenario 脚本驱动**——harness 对后端语言无知,只认 RPC 协议。

### 2.3 非确定性归一化(归一化清单 = 评审过的合同)

对照前必须抹平,且**抹平规则本身要在 PR 里显式列出、单独评审**(防止「为了绿而过度抹平」):

| 字段类 | 处理 |
|---|---|
| 时间戳 / `duration` / `hrtime` | 置零或替换为 `<ts>` 占位 |
| UUID / sessionId(未固定时) | 固定种子或替换为序号占位 `<id:0>` |
| 绝对路径 / `homeDir` / tmpdir | 替换为 `<HOME>` / `<TMP>` 相对锚点 |
| 流式分片边界(token-by-token 的切分点) | 先 `join` 再比对**最终拼接结果**;另设独立用例比对**分片数量量级**(非逐片) |
| 进程 pid / 端口 | 替换为占位 |
| 平台行尾 / 路径分隔符 | 经 kaos 的 `pathClass()` 归一(见 4.1) |
| 错误对象 | 比对 `{ code, kind, messageShape }`,**不比对** stack/绝对路径 |

> **反模式守门**:归一化清单每新增一项,需在 PR 描述写明「为什么这个字段无法/不应对齐」。清单膨胀本身就是 No-Go 信号(意味着两后端语义在漂移)。

### 2.4 确定性供给

- **LLM**:复用 Rust 侧 `--mock-provider`(`config.rs:39`)与 TS 侧等价 mock provider。两侧 mock **必须产出逐字节相同的 assistant 流**——这是 L3 的前提,故 mock provider 本身先做一次 L1 对照。
- **录制 fixture**:`kosong` 的真实 provider 用**录制的 SSE 流**(VCR 模式)喂给 TS 与 Rust 两套解析器,比对 `StreamedMessage` 序列(见 4.2)。
- **文件系统**:每个场景在干净 tmpdir 内构造已知目录树,kaos 操作后比对 tree 快照 + 操作返回值。

### 2.5 对照测试的 CI 形态

- 新增 job `parity` 于 `.github/workflows/rust-host.yml`(扩展现有 workflow)。
- 矩阵:`{ transport: [stdio, uds], layer: [L1, L2, L3] }`;L4 单独 nightly。
- **硬门**:`rust ≠ ts` 即红,失败上传两侧原始 + 归一化产物作为 artifact 便于 diff。
- 平台矩阵:至少 `darwin-arm64` + `linux-x64`(行尾/路径差异最易在此暴露)。

---

## 3. 迁移总览(自底向上,每块双实现可切换)

### 3.0 执行模式判定 Rubric(本节标注的统一标准)

> 本路线图每个子阶段都带 `[normal]`/`[plan]`/`[design]` 执行模式标注。**模式由「决策不确定性」决定,不由风险等级决定。** 新增或调整子阶段时一律按下表与三条规则套用,以保证跨节标注一致。

| 模式 | 何时归此 | 软件机制依据 |
|---|---|---|
| **normal** | 机械迁移;有成熟前置/共享层可照搬;不引入新的共享签名或架构决策 | normal 模式可直接改代码,无规划开销 |
| **plan** | 多步骤且有真实依赖;改共享 trait/签名引发调用方扇出;值得 task-by-task TDD | plan 模式禁止改计划外文件,强制依赖图 + test-first |
| **design** | 架构 / 数据模型 / 对外契约 / 迁移语义存在真实未知,定错难回滚 | design 模式 HARD-GATE,方案获批前禁止实现 |

**判定规则(消除跨节标注漂移):**

1. **风险 ≠ 模式**:业务风险「高」不代表需要 plan/design。当存在成熟共享前置层、迁移本质是照搬时,即使风险高也应是 **normal**——故 `4.2.3–4.2.6`(provider,各有 4.2.2 共享解析层作前置)、`4.3.6`(compaction,分词器对齐作前置)标 normal 是正确的,不是漏标。
2. **奠基子阶段 + 集成门 = plan**:每个模块的 `.0` 子阶段(定义 trait/接口:`4.1.0/4.2.0/4.3.0/4.4.0`)与每个跨模块集成门子阶段(`4.1.4/4.2.7/4.3.9/4.4.8`)一律 **plan**,因其改动共享契约且调用方扇出大。
3. **契约/数据模型定义 → plan + design-lite 决策门**:当一个 plan 子阶段的产物是**定错难回滚的终态契约或持久化格式**(`4.3.0` records WAL schema、`4.5.5` CoreAPI/SDKAPI/TUI 契约冻结),进入时第一步必须先做一次**轻量终态决策**(枚举候选格式/契约、记录取舍),再进入 TDD。不必整段升 design,但这一步不可跳过。

| 子阶段 | 模块 | TS LOC | Rust 目标 crate | 主要对照层 | 风险 | 门 | 执行模式 |
|---|---|---:|---|---|---|---|---|
| **4.0** | 对照框架 + 双后端可切换开关 | — | `parity/` harness | — | 低 | G4-0 | normal |
| **4.1** | `kaos` 执行环境（拆为 5 子阶段，见 §4.1） | 2.3K | `ody-host/src/env/`(新)+ `kaos-rs` crate | L1 + L2 | 中 | G4-1 | — |
| 4.1.0 | kaos crate 骨架 + 路径/环境操作 | — | `kaos-rs` | L1 | 低 | G4-1-0 | **plan** |
| 4.1.1 | 目录操作（stat/iterdir/glob/mkdir） | — | `kaos-rs` | L1 | 中 | G4-1-1 | normal |
| 4.1.2 | 文件读写操作（含编码错误模式） | — | `kaos-rs` | L1 | 中 | G4-1-2 | normal |
| 4.1.3 | 进程执行（exec / KaosProcess / kill） | — | `kaos-rs` | L1 | 中 | G4-1-3 | **plan ✅** |
| 4.1.4 | CoreHost 集成 / RPC 暴露 / bash 迁移 | — | `ody-host` | L2 | 中 | G4-1 | **plan** |
| **4.2** | `kosong` LLM 层（拆为 8 子阶段，见 §4.2） | 7.4K | `ody-host/src/llm/`(扩)+ `kosong-rs` crate | L1(SSE 重放)+ L3 | 高 | G4-2 | — |
| 4.2.0 | kosong 共享数据模型 + generate 循环 | 1.8K | `kosong-rs` | L1 | 高 | G4-2-0 | **plan** |
| 4.2.1 | 通用工具层（tool-call-id / auth / capability / catalog） | 1.0K | `kosong-rs` | L1 | 中 | G4-2-1 | normal |
| 4.2.2 | OpenAI Chat Completions 共享解析 + OpenAI Legacy | 1.5K | `kosong-rs` | L1 SSE | 高 | G4-2-2 | **plan** |
| 4.2.3 | OpenAI Responses provider | 1.0K | `kosong-rs` | L1 SSE | 高 | G4-2-3 | normal |
| 4.2.4 | Anthropic provider | 1.1K | `kosong-rs` | L1 SSE | 高 | G4-2-4 | normal |
| 4.2.5 | Chat-Completions 兼容三兄弟（Kimi / DeepSeek / GLM） | 1.3K | `kosong-rs` | L1 SSE | 高 | G4-2-5 | normal |
| 4.2.6 | Google GenAI provider | 0.9K | `kosong-rs` | L1 SSE | 高 | G4-2-6 | normal |
| 4.2.7 | CoreHost provider factory + L2/L3 门 | 0.8K | `ody-host` | L2 + L3 | 高 | G4-2 | **plan** |
| **4.3** | `agent` 编排核心（拆为 10 子阶段，见 §4.3） | ~15.7K | `agent-rs` crate | L3 + L4 | 最高 | G4-3 | ✅ 全 10 子阶段已实现 (2026-06-29 审计, ~27.5K LOC + 50 test files) |
| 4.3.0 | Records & persistence foundation | 2,764 | `agent-rs/src/records/` (6 files) | L4 | 中 | G4-3-0 | ✅ done — `AgentRecords`/`FileSystemPersistence`/`BlobStore`/`WireMigration` |
| 4.3.1 | Context & projection(依赖 4.3.0 + injection 生命周期接口) | 842 | `agent-rs/src/context/` (6 files) | L1 + L3 | 中 | G4-3-1 | ✅ done — `ContextMemory`/`project()`/token count/notification-xml |
| 4.3.2 | Config / usage / tool & skill registry(Skill prompt 路径依赖 4.3.5) | 1,403 | `agent-rs/src/{config,usage,tool,skill}/` | L2 | 中 | G4-3-2 | ✅ done — `ConfigState`/`UsageRecorder`/`ToolManager`/`SkillManager` |
| 4.3.3 | Permission system(依赖 4.3.0 + 4.3.2 tool 信息) | 2,020 | `agent-rs/src/permission/` (15 policies) | L3 | 中 | G4-3-3 | ✅ done — `PermissionManager` + 全部 policy |
| 4.3.4 | Stateless loop engine(与 4.3.1/4.3.2/4.3.3 可并行) | 2,498 | `agent-rs/src/agent_loop/` (10 files) | L3 | 高 | G4-3-4 | ✅ done — `runTurn`/`executeLoopStep`/`ToolScheduler`/`LoopHooks` |
| 4.3.5 | Turn flow & LLM adapter(依赖 4.3.1+4.3.2+4.3.3+4.3.4) | 5,266 | `agent-rs/src/turn/` (11 files) | L3 | 最高 | G4-3-5 | ✅ done — `TurnFlow`/`KosongLLM`/`RemoteKosongLLM`/`ToolCallDeduplicator` |
| 4.3.6 | Compaction strategies(依赖 4.3.1+4.3.2+4.3.5) | 1,948 | `agent-rs/src/compaction/` (10 files) | L1 + L3 | 高 | G4-3-6 | ✅ done — `FullCompaction`/`MicroCompaction`/`SplitPlanCheckpoint`/budget |
| 4.3.7 | Session modes & prompt injection(依赖 4.3.1+4.3.2+4.3.5) | 2,346 | `agent-rs/src/{session_mode,injection,replay}/` | L3 | 高 | G4-3-7 | ✅ done — `SessionModeManager`(4 behaviors)/`InjectionManager`(9 injectors)/`ReplayBuilder` |
| 4.3.8 | Background tasks & cron(依赖 4.3.5) | 3,683 | `agent-rs/src/{background,cron}/` (13 files) | L3 | 中 | G4-3-8 | ✅ done — `BackgroundManager`(real)/`CronManager`(real)/persistence/scheduler/jitter |
| 4.3.9 | Agent orchestrator & CoreHost integration(依赖 4.3.0–4.3.8) | ~3,700 | `agent-rs/src/agent.rs` + `ody-host/src/{host,agent_bridge}.rs` | L2 + L3 + L4 | 最高 | G4-3-9 | ✅ done — `Agent`/`AgentBuilder`/`CoreHost`/`SessionManager`/RPC routing |
| **4.4** | `tools/builtin` 拆分 9 子阶段（见 §4.4） | ~7.3K(工具体,不含 support/providers/cron) | `tools-rs` crate | L1 + L3 | 中 | G4-4 | — |
| 4.4.0 | Tool infrastructure & shared support | — | `tools-rs` | L1 | 低 | G4-4-0 | ✅ done |
| 4.4.1 | File & shell core tools | — | `tools-rs` | L1 + L3 | 中 | G4-4-1 | ✅ done |
| 4.4.2 | Web tools | — | `tools-rs` | L1 + L3 | 低 | G4-4-2 | ✅ done |
| 4.4.3 | Background & cron management tools | — | `tools-rs/src/builtin/{background,cron}/` | L1(已覆盖) + L3 | 中 | G4-4-3 | ✅ done(6 tools, L1 golden parity 绿) |
| 4.4.4 | Collaboration tools | — | `tools-rs` | L3 | 高 | G4-4-4 | ⬜ pending — 依赖 4.3.5/4.3.7/4.3.8 ✅ 就绪 |
| 4.4.5 | Session-mode workflow tools | — | `tools-rs` | L3 | 高 | G4-4-5 | ⬜ pending — 依赖 4.3.7 ✅ 就绪 |
| 4.4.6 | Goal & state tools | — | `tools-rs` | L2 + L3 | 中 | G4-4-6 | ⬜ pending — 依赖 4.3.2 ✅ 就绪 + SessionGoalStore 缺口需补齐 |
| 4.4.7 | Quality & specialized tools | — | `tools-rs` | L1 + L3 | 中 | G4-4-7 | ⬜ pending — 叶子工具，依赖 4.4.1/4.3.7 等 |
| 4.4.8 | Tool registration integration & L2/L3 gate | — | `ody-host` | L2 + L3 | 中 | G4-4 | ⬜ pending — 依赖 4.3.9 ✅ + 4.4.4–4.4.7 ⬜ |
| **4.5** | 收官与终态固化（拆为 8 子阶段，见 §4.5） | — | — | L4 全量 | 中 | G4-final | — |
| 4.5.0 | Final gap inventory & triage | — | — | — | 高 | G4-final-0 | **design** |
| 4.5.1 | Migrate deferred kaos gaps | — | `kaos-rs` | L1 | 高 | G4-final-1 | **plan** |
| 4.5.2 | Migrate deferred kosong/provider gaps | — | `kosong-rs` | L1 SSE | 中 | G4-final-2 | normal |
| 4.5.3 | Migrate deferred agent/tool gaps | — | `agent-rs`/`tools-rs` | L1 + L3 | 中 | G4-final-3 | **plan** |
| 4.5.4 | Delete TS dual implementations | — | — | L4 | 中 | G4-final-4 | normal |
| 4.5.5 | Freeze CoreAPI/SDKAPI & TUI transport contract | — | — | L2 + L4 | 高 | G4-final-5 | **plan** |
| 4.5.6 | L4 regression gate & golden archive | — | `parity/` | L4 | 中 | G4-final-6 | normal |
| 4.5.7 | ADR update & success metrics | — | `docs/` | — | 低 | G4-final-7 | normal |

**强约束:严格自底向上。** kaos 是所有 I/O 的地基;kosong 依赖 kaos(网络/落盘);agent 依赖 kosong+kaos;tools 依赖 kaos+agent 上下文。倒序迁移会让对照测试无法隔离故障源。

---

## 4. 分子阶段详解

### 4.0 — 对照框架就位 + 双后端运行时开关(前置,~3–5 天)

**目标**:在迁移任何真实逻辑前,先让「同一脚本打两个后端并逐字段对照」这件事跑通且自证可信。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.0.1 | 实现 `parity/{driver,backends,normalize,assert-parity}.ts` | 见 §2.2 |
| 4.0.2 | 移植 3 个现有场景为 scenario:session 生命周期 / setModel / mock prompt | 复用 `rust-host-connect.test.ts` 已有断言点 |
| 4.0.3 | **TS-vs-TS 自比对**:两个 `createCoreServer` 实例跑同脚本必须 `toEqual` | 证明 harness + 归一化无 bug |
| 4.0.4 | 切 TS-vs-Rust,容许已知差异登记在 `parity/known-gaps.md` | Rust 侧未实现项显式标 `skip(reason)` |
| 4.0.5 | 运行时开关:`ODY_BACKEND=ts|rust` + per-module `ODY_RUST_MODULES=kaos,kosong,...` | 为 4.1+ 的「单模块切 Rust、其余留 TS」铺路(混合后端) |

> **4.0.5 的混合后端是 Phase 4 可回滚到单模块粒度的关键**:Rust host 需支持「kaos 走 Rust 但 LLM 仍回调 TS」这类组合,或至少支持「整后端 ts/rust 二选一 + 模块级 golden 对照」。二选一更简单,优先;若 agent 迁移期需要更细回滚,再上模块级桥接。

**门 G4-0**:TS-vs-TS 自比对绿;TS-vs-Rust 在已实现方法上绿(未实现项显式 skip);CI `parity` job 上线。**harness 不可信则全程无意义,此门最硬。**

---

### 4.1 — `kaos` 执行环境 → Rust(中风险,拆为 5 个子阶段)

**为什么先迁**:进程/fs/ssh 是一切 I/O 的地基,反转后 Rust 优势最大(沙箱/资源控制),且接口面清晰(`kaos.ts` 的 `Kaos` interface,§见 `packages/kaos/src/kaos.ts:12`)。

**接口面**(对照测试的契约):
- 路径(sync):`pathClass / normpath / gethome / getcwd`
- 目录(async):`chdir / withCwd / stat / iterdir / glob / mkdir`
- 文件(async):`readBytes / readText(strict|replace|ignore)/ readLines / writeBytes / writeText(w|a)`
- 进程:`exec / execWithEnv` → `KaosProcess`(stdout/stderr/exitCode)
- **SSH(`ssh.ts` 915 行):本子阶段 Non-Goal**,登记在 `known-gaps.md`,留 TS 经混合后端回调(或 defer 到 4.5 后单列)。

**Rust 落地**:新建 `kaos-rs` crate(tokio::fs + tokio::process + std::path),`ody-host` 通过依赖 `kaos-rs` 接入。`Kaos` struct 携带实例级 `cwd`,`with_cwd` 返回新实例(与 `LocalKaos` 一致,不污染全局 `std::env::current_dir`)。

**拆分原则与依赖关系**:
- 按操作族拆分:纯路径计算 / 目录元数据 / 文件内容 / 进程执行 / 集成与 RPC。
- **4.1.0 是硬前置**:4.1.1/4.1.2/4.1.3 都依赖 `Kaos` struct、实例级 `cwd`、错误类型以及 `internal.ts` 中的共享纯函数(`decodeTextWithErrors` / `globPatternToRegex` / `BufferedReadable`)。这些共享helper必须先落地,否则并行开发会产生冲突。
- **4.1.1 / 4.1.2 / 4.1.3 彼此无依赖**,在 4.1.0 完成后可并行开发;建议仍按「目录 → 文件 → 进程」顺序合入,因为 agent/tools 最早需要的是目录/文件能力,且进程子阶段的平台安全评审最耗时。
- 每子阶段先建 L1 golden 对照,绿后再开启下一子阶段;避免「全部实现完才发现 readText ignore 模式对不齐」。
- 进程执行独立成子阶段:跨平台(POSIX 进程组 vs Windows `taskkill`)和安全性最高,需要单独评审。
- 最终集成子阶段(4.1.4)负责把 `kaos-rs` 接入 `CoreHost`、提供内部 `env.*` RPC(或等价的测试面),并把 `BashTool` 从内置 `tokio::process::Command` 切到 kaos;**4.1.4 依赖 4.1.1/4.1.2/4.1.3 全部完成**。

**建议执行模式**（见 §3 表格列）:
- **4.1.0 / 4.1.3 / 4.1.4 → plan 模式**：新建 crate、进程安全、CoreHost 集成均涉及接口/边界/回滚策略，需要先产出可执行计划再编码。其余子阶段接口已由 TS 锁定，适合 normal 模式直接实现。
- **4.1 无 design 模式子任务**：目标、边界、接口和「SSH defer」策略在路线图中已锁定，不存在需要头脑风暴/规格探索的开放性问题；若执行时发现 4.1.0 的 crate 边界或 4.1.4 的 RPC 形态与现有架构冲突，则升级为 design 模式重新决策。

#### 4.1.0 — kaos crate 骨架 + 路径/环境操作 `[plan]`(低风险,~2–3 天)

**目标**:先把 kaos 的 crate 边界和纯路径/环境语义钉死,不碰 async I/O。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.1.0.1 | 新建 `rust-ody/crates/kaos-rs` crate,加入 workspace | 更新 `rust-ody/Cargo.toml` workspace members |
| 4.1.0.2 | 定义 `Kaos` struct 与实例级 `cwd` | 对齐 `LocalKaos`:`with_cwd` 克隆,`chdir` 仅改内部状态 |
| 4.1.0.3 | 实现 `pathClass / normpath / gethome / getcwd` | `std::path` / `dirs::home_dir`;win32 vs posix 分支 |
| 4.1.0.4 | 实现环境探测 | 对齐 `detectEnvironmentFromNode`:平台/架构/shell 路径;Windows Git Bash 定位 |
| 4.1.0.5 | 迁移共享 helper(纯函数/无 I/O) | 对齐 `internal.ts`:`decodeTextWithErrors`(UTF-8/UTF-16LE strict/replace/ignore)、`globPatternToRegex`、`BufferedReadable` 缓冲语义 |
| 4.1.0.6 | L1 golden 路径 fixture | `parity/fixtures/kaos/path/`:输入路径 → 预期 `normpath` / `pathClass`;含非法 UTF-8 decode 用例 |

**对照测试设计(L1)**:
- fixture 为 JSON:`{ "cwd": "...", "inputs": [...], "expected": [...] }`
- TS 侧直接调用 `LocalKaos`;Rust 侧由 `kaos-rs` 的 test binary / 临时 golden binary 解析同一 fixture。
- 重点:`normpath` 对 `.`/`..`/`//`、盘符、反斜杠的处理与 `pathe.normalize` 逐字符一致。

**门 G4-1-0**:`kaos-rs` 编译通过;路径/环境 golden fixture TS vs Rust 100% 绿;`with_cwd` 实例隔离语义与 TS 一致;`decodeTextWithErrors` / `globPatternToRegex` 的 L1 fixture 绿。**此门不绿,禁止进入 4.1.1/4.1.2/4.1.3。**

#### 4.1.1 — 目录操作 `[normal]`(中风险,~3–4 天)

**目标**:迁移 stat/iterdir/glob/mkdir。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.1.1.1 | 实现 `stat` | 对齐 `StatResult` 字段;`followSymlinks` 开关;mtime/ctime Windows 生日行为 |
| 4.1.1.2 | 实现 `iterdir` | 返回规范化路径,根目录 trailing slash 处理 |
| 4.1.1.3 | 实现 `glob` | 复刻 `globPatternToRegex` + `_globWalk` 算法;`**`、字符类 `[!a]`、大小写敏感、隐藏文件、循环检测 |
| 4.1.1.4 | 实现 `mkdir` | `parents` + `existOk` 语义;目录已存在抛 `KaosFileExistsError` |
| 4.1.1.5 | L1 golden 目录 fixture | 构造已知 tmpdir 树,比对返回列表/字段 |

**对照测试设计(L1)**:

| 用例族 | 输入 | 比对点 | 易错点(必测) |
|---|---|---|---|
| stat | 文件/目录/符号链接/不存在 | `StatResult` 全字段(size/mode/isDir/mtime 归一) | followSymlinks 开关 |
| glob | 已知目录树 + 多 pattern | **排序后**的匹配列表 | 大小写敏感、`**` 递归、隐藏文件、symlink 循环检测 |
| mkdir | 嵌套/已存在/冲突文件 | 返回值 + 落盘树 | `parents`/`existOk` 组合 |

**门 G4-1-1**:目录操作 L1 对照 100% 绿,含 glob symlink 循环安全用例。

#### 4.1.2 — 文件读写操作 `[normal]`(中风险,~3–4 天)

**目标**:迁移 read/write 族;难点是 `readText` 的 `errors` 模式与 TS `decodeTextWithErrors` 逐字节一致。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.1.2.1 | 实现 `readBytes / writeBytes` | 字节透传;`readBytes(n)` 最多读 n 字节 |
| 4.1.2.2 | 实现 `readText / readLines` | `encoding` + `errors`(strict/replace/ignore);UTF-8 / UTF-16LE |
| 4.1.2.3 | 实现 `writeText` | `mode=w\|a`;返回字符数 |
| 4.1.2.4 | L1 golden 文件 fixture | 含非法 UTF-8、混合 CRLF、有效 U+FFFD 等 |

**对照测试设计(L1)**:

| 用例族 | 输入 | 比对点 | 易错点(必测) |
|---|---|---|---|
| readText 错误模式 | 含非法字节的文件 | strict 抛错形状 / replace 的 **U+FFFD** 位置 / ignore 丢弃结果 | 这是 TS 特有语义,Rust 须逐字节复刻 |
| writeText w/a | 写+追加 | 返回的字符数 + 落盘字节 | 编码、行尾 |
| LF/CRLF 保留 | 文本写入再读字节 | `readBytes` 与原始 Buffer 一致 | 不自动转换行尾 |
| readBytes(n) | 部分读取 | 返回长度 ≤ n | EOF 边界 |

**门 G4-1-2**:文件读写 L1 对照 100% 绿,含 readText 三种错误模式逐字节一致。

#### 4.1.3 — 进程执行 `[plan]`(中风险,~4–5 天)

**目标**:迁移 `exec / execWithEnv` 与 `KaosProcess` 生命周期。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.1.3.1 | 实现 `exec / execWithEnv` | `tokio::process::Command`;继承 kaos 实例 `cwd`;环境变量隔离 |
| 4.1.3.2 | 实现 `KaosProcess` | stdout/stderr 流式 + 缓冲;`pid` / `exitCode` / `wait` / `kill` |
| 4.1.3.3 | POSIX 进程组 kill | `detached` + `kill(-pgid, signal)`;ESRCH/EPERM 容错 |
| 4.1.3.4 | Windows 进程树 kill | `taskkill /T` fallback |
| 4.1.3.5 | L1 golden 进程 fixture | echo / 退出码 / stderr / 大输出 / 未找到命令 |

**对照测试设计(L1)**:

| 用例族 | 输入 | 比对点 | 易错点(必测) |
|---|---|---|---|
| exec | echo / 退出码 / stderr / 大输出 | stdout/stderr 字节 + exitCode | 缓冲、流式、环境变量隔离 |
| wait-before-read | 先 wait 再读流 | 缓冲不丢数据 | 流已关闭后仍可读取 |
| kill | 长运行进程 | 进程树终止 | POSIX 进程组 / Windows taskkill |
| spawn failure | 不存在的命令 | 拒绝方式 | 不能信号 process group -1 |

**门 G4-1-3**:exec 在 darwin+linux 两平台 L1 对照绿;kill 进程树用例绿。

#### 4.1.4 — CoreHost 集成 / RPC 暴露 / bash 工具迁移 `[plan]`(中风险,~3–4 天)

**目标**:让 kaos 真正被 parity harness 通过 RPC 驱动,并让现有 `BashTool` 走 kaos。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.1.4.1 | `ody-host` 依赖 `kaos-rs` | `ody-host/Cargo.toml` |
| 4.1.4.2 | `CoreHost` 持有 `Arc<Kaos>` | host 级实例;未来按 session workDir 派生 |
| 4.1.4.3 | 内部 RPC 方法暴露 kaos 操作 | `env.stat` / `env.glob` / `env.readText` / `env.writeText` / `env.exec` 等(内部/测试用,不进入 `CoreAPI` 公共契约) |
| 4.1.4.4 | 迁移 `BashTool` 到 kaos | `tools/bash.rs` 不再直接 `tokio::process::Command`,改为 `kaos.exec('bash', '-c', command)` |
| 4.1.4.5 | L2 parity scenario | 通过 `SDKRpcClient` 调用内部 env 方法,比对 TS vs Rust 响应 |
| 4.1.4.6 | 📊 基准 | `rust-ody/ts/bench.ts` 方法论,对 stat/glob/read 大目录测短/中/长三档,记录 Rust vs TS 增益 |

**对照测试设计(L2)**:
- 复用 4.0 的 `ParityDriver` / `backends.ts`,新增 kaos 专用 scenario(不依赖 LLM,直接调内部 env 方法)。
- TS 后端:在 parity TS backend 中直接调 `LocalKaos`,返回与 Rust `env.*` 同形的 JSON。
- Rust 后端:`CoreHost.dispatch` 识别 `env.*` 方法并路由到 `kaos-rs`。

实现方式:fixture 构造已知 tmpdir 树 → 同一组操作分别经 TS `LocalKaos` 与 Rust `kaos-rs`(经 host 的内部 `env.*` RPC 或专用 golden 二进制)→ 归一化后 `toEqual`。

**门 G4-1**:
- kaos 全接口面(除 ssh)L1 对照 100% 绿,含 readText 三种错误模式逐字节一致;
- `env.*` 内部 RPC L2 对照绿;
- `BashTool` 经 kaos 后在现有 parity scenario(如 `file-edit` / `multi-turn-tool`)中绿;
- exec 在 darwin+linux 两平台对照绿;
- 📊 基准:`rust-ody/ts/bench.ts` 方法论,对 stat/glob/read 大目录测短/中/长三档,记录 Rust vs TS 增益。
- **No-Go 信号**:readText 错误模式无法对齐(Rust 的 UTF-8 处理与 Node Buffer 语义有不可调和差异)→ 该方法保留 TS 回调,登记 gap,不阻塞其余。

---

### 4.2 — `kosong` LLM 层 → Rust(高风险,拆为 8 子阶段)

**为什么高风险**:8 家 provider(`packages/kosong/src/providers/`)各有 SSE 流格式、tool-call 协议、thinking 语义差异;归一化抽象(`ThinkingEffort`/`FinishReason`/`StreamedMessage`,`provider.ts:15-55`)是产品正确性的命脉。**任何流解析漂移都会静默改变模型行为。**

**为什么必须拆**:Rust 侧当前 `llm/mod.rs` 只有最简 `LlmProvider` trait(`chat_stream` callback + 粗糙 `FinishReason`),与 TS `ChatProvider` 在 `StreamedMessage`/`GenerateOptions`/`withThinking`/`getCapability`/`tool-call 并行路由` 等维度存在**结构性差距**。若不分拆,直接按 provider 迁移会导致每家都重复补齐这些公共语义,且无法做 provider 无关的 L1 golden 对照。拆分后:
- 先钉死**公共数据模型与 generate 循环**(4.2.0);
- 再钉死**通用工具层**(4.2.1,依赖 4.2.0 类型);
- 然后按**协议族**迁移:OpenAI Chat Completions 共享层(4.2.2)→ OpenAI Responses(4.2.3,**复用 openai-common**)/Anthropic(4.2.4)/Chat-Completions 兼容三兄弟(4.2.5,**复用 chat-completions-stream**)/Google GenAI(4.2.6);
- 最后补齐 **CoreHost provider factory + L2/L3 门**(4.2.7)。

**依赖关系**:4.2.0 → 4.2.1 → {4.2.2, 4.2.4, 4.2.6};4.2.2 → {4.2.3, 4.2.5};所有 provider → 4.2.7(provider factory)。

**Rust 落地**:新建 `kosong-rs` crate,`ody-host` 依赖它并保留 provider factory。当前 `ody-host/src/llm/mod.rs` 的简化 trait 需逐步演化为与 TS `ChatProvider` 同形(含 `StreamedMessage`、`GenerateOptions`、`withThinking`、`getCapability`)。

**建议执行模式**（见 §3 表格列）:
- **4.2.0 / 4.2.2 / 4.2.7 → plan 模式**：公共数据模型与 `generate` 循环是所有 provider 的契约面；OpenAI Chat Completions 共享解析层决定 4.2.3/4.2.5 的复用结构；CoreHost provider factory 是集成面。这三处先做 plan 可避免后续大面积返工。
- **4.2.1 / 4.2.3 / 4.2.4 / 4.2.5 / 4.2.6 → normal 模式**：4.2.1 是纯函数横切逻辑，各 provider 是协议实现；接口与输入输出已由 TS 和公共层锁定，适合直接对照实现。
- **4.2 无 design 模式子任务**：「建 `kosong-rs`、按协议族复用、provider factory 路由」在路线图中已锁定；不确定的是 Rust 侧类型映射与解析器拆分，属于 plan 模式范畴，不需要 design 模式的规格探索。若发现某 provider 协议无法与 TS 逐值对齐，则该 provider defer 到 gap 清单，不升级为 design。

#### 4.2.0 — kosong 共享数据模型 + generate 循环 `[plan]`(高风险,~4–5 天)

**目标**:先把 provider 无关的公共协议钉死,这是后续所有 provider 的契约面。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.0.1 | 新建 `rust-ody/crates/kosong-rs` crate,加入 workspace | 更新 `rust-ody/Cargo.toml` workspace members |
| 4.2.0.2 | 迁移归一化类型 | `Message`/`ContentPart`/`ToolCall`/`StreamedMessagePart`/`ThinkingEffort`/`FinishReason`/`TokenUsage`/`GenerateResult`/`GenerateOptions`/`ProviderType` |
| 4.2.0.3 | 实现 `generate()` 循环 | 对齐 `packages/kosong/src/generate.ts`:abort 检查、`onMessagePart`/`onToolCall` 回调、tool-call 并行路由(`toolCallIndexMap`)、`mergeInPlace`、空响应/think-only 拒绝 |
| 4.2.0.4 | 定义 `ChatProvider` trait | `name`/`modelName`/`thinkingEffort`/`generate`/`withThinking`/`withMaxCompletionTokens`/`getCapability` |
| 4.2.0.5 | 错误分类 | 对齐 `errors.ts`:`ChatProviderError`/`APIConnectionError`/`APITimeoutError`/`APIStatusError`/`APIContextOverflowError`/`APIEmptyResponseError` |
| 4.2.0.6 | L1 generate 循环 golden fixture | mock part 序列 → TS/Rust generate 输出逐值 |

**对照测试设计(L1)**:
- fixture 为 JSON:`{ "parts": [...], "expected": { "message": ..., "finishReason": ..., "usage": ... } }`
- TS 侧调用 `generate()` with mock provider;Rust 侧调用 `kosong-rs` 的 generate with 同样的 mock part 迭代器。
- 必测:连续 text 合并、think 合并、单/并行 tool-call 参数路由、abort 中途取消、空响应抛 `APIEmptyResponseError`、think-only 拒绝。

**门 G4-2-0**:`kosong-rs` 编译通过;generate 循环 L1 golden fixture TS vs Rust 100% 绿;错误对象 `{ name, messageShape }` 对齐。

---

#### 4.2.1 — 通用工具层 `[normal]`(中风险,~3–4 天)

**目标**:迁移被所有 provider 复用的横切逻辑,避免每家重复实现。**依赖 4.2.0 的 `Message`/`ToolCall`/`ProviderRequestAuth`/`ModelCapability`/`ProviderType` 类型**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.1.1 | 实现 `tool-call-id` 语义 | `sanitizeToolCallId`/`sanitizeOpenAIResponsesCallId`/`normalizeToolCallIdsForProvider`/`makeUniqueToolCallId`,含 64 字符截断与冲突重命名 |
| 4.2.1.2 | 实现 `request-auth` 语义 | `requireProviderApiKey`/`mergeRequestHeaders`/`resolveAuthBackedClient` 的 precedence(constructor env < per-request auth) |
| 4.2.1.3 | 实现 `capability-registry` | 按模型名前缀匹配 `ModelCapability`;OpenAI/Anthropic/Google/Kimi 家族映射 |
| 4.2.1.4 | 实现 `catalog` 解析 | `inferWireType`/`catalogBaseUrl`/`catalogModelToCapability`,对齐 `packages/kosong/src/catalog.ts` |
| 4.2.1.5 | L1 golden fixture | tool-call-id 冲突重命名表、capability 查询表、catalog JSON 解析表 |

**对照测试设计(L1)**:
- tool-call-id:输入一组可能冲突的 raw id → 比对 TS/Rust 归一化结果。
- capability:输入模型名 → 比对 `ModelCapability` 各字段。
- catalog:输入 catalog JSON → 比对 inferred wire type + capability。

**门 G4-2-1**:工具层 L1 对照 100% 绿,尤其 tool-call-id 在 64 字符截断与重名后缀场景下与 TS 一致。

---

#### 4.2.2 — OpenAI Chat Completions 共享解析 + OpenAI Legacy `[plan]`(高风险,~4–5 天)

**目标**:建立 OpenAI Chat Completions 协议族的共享解析层,并完成 OpenAI Legacy provider。**这是 DeepSeek/GLM/Kimi 的公共地基**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.2.1 | 实现 `openai-common` | `convertContentPart`/`toolToOpenAI`/`extractUsage`/`normalizeOpenAIFinishReason`/`convertOpenAIError`/`thinkingEffort↔reasoning_effort` 映射 |
| 4.2.2.2 | 实现 `chat-completions-stream` | `convertChatCompletionStreamToolCall` + `BufferedChatCompletionToolCall`,支持参数先 buffering 再 emit header、`_streamIndex`/`index` 路由 |
| 4.2.2.3 | 实现 `OpenAILegacyChatProvider` | message 转换(含 `reasoningKey` round-trip、`toolMessageConversion`)、request 构造、stream/non-stream 解析 |
| 4.2.2.4 | L1 SSE fixture | 纯文本/thinking/单 tool-call/并行 tool-calls/截断/错误/usage |

**对照测试设计(L1 SSE 重放)**:
- 同一份 `.sse` 字节分别喂给 TS `OpenAILegacyChatProvider` 与 Rust provider,比对 `StreamedMessagePart` 序列(归一化 id/usage)。
- 必测:`finish_reason` 映射、`reasoning_content` 扫描多 key、`tool_calls` 索引路由、usage 中 `cached_tokens` 解析。

**门 G4-2-2**:OpenAI Legacy 全部 SSE fixture L1 绿;Chat Completions 共享解析器被后续 provider 复用而无重复实现。

---

#### 4.2.3 — OpenAI Responses provider `[normal]`(高风险,~4–5 天)

**目标**:迁移 `openai-responses.ts`。**协议与 Chat Completions 完全不同**(事件类型驱动、`output_item`、developer role、reasoning summary),**但复用 4.2.2 的 `openai-common` 进行错误转换与 reasoning_effort 映射,故依赖 4.2.2**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.3.1 | 实现 `OpenAIResponsesChatProvider` | message → `input` items(含 reasoning/`function_call_output`)、request 构造、developer role 判定 |
| 4.2.3.2 | 实现 stream 事件解析 | `response.output_text.delta`/`output_item.added|done`/`function_call_arguments.delta|done`/`reasoning_summary_*`/`response.completed|incomplete|failed`/`error` |
| 4.2.3.3 | 实现 non-stream 解析 | `output` items → text/function_call/reasoning |
| 4.2.3.4 | tool-call-id 策略 | `sanitizeOpenAIResponsesCallId(id, 64)`,处理 `call_id` 中的 `\|` 分隔符 |
| 4.2.3.5 | L1 SSE + non-stream fixture | 覆盖 reasoning summary、并行 function_call、`incomplete` 状态、error 事件 |

**对照测试设计(L1)**:
- SSE 重放:TS `OpenAIResponsesStreamedMessage` vs Rust `OpenAIResponsesStreamedMessage`。
- 重点:`response.id` 只在 `response.created/in_progress/completed` 捕获(不被 `item.id` 覆盖)、`item_id`/`output_index` 作为 tool-call 路由 key、reasoning summary 拼接、final arguments suffix 与 streamed deltas 一致性校验。

**门 G4-2-3**:Responses provider 全部 fixture L1 绿;`function_call_arguments.delta` 与 `.done` 的 suffix 校验逻辑对齐。

---

#### 4.2.4 — Anthropic provider `[normal]`(高风险,~4–5 天)

**目标**:迁移 `anthropic.ts`。**唯一使用 Messages API + SSE 事件类型(`message_start`/`content_block_start`/`content_block_delta`/`message_delta`)的 provider**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.4.1 | 实现 `AnthropicChatProvider` | system prompt → `system` param、message 转换(think block、tool_result 合并、cache_control)、工具转换、max_tokens ceiling 解析 |
| 4.2.4.2 | 实现 stream 事件解析 | `message_start`/`content_block_start`/`content_block_delta`/`message_delta`/thinking/tool_use/input_json_delta/signature_delta |
| 4.2.4.3 | 实现 non-stream 解析 | `content` blocks → text/thinking/redacted_thinking/tool_use |
| 4.2.4.4 | thinking 配置 | `budget_tokens`/`adaptive`/`output_config.effort` 映射到 `ThinkingEffort`;Claude 版本 ceiling 表 |
| 4.2.4.5 | L1 SSE + non-stream fixture | 覆盖 thinking、并行 tool_use、cache usage、stop_reason 映射 |

**对照测试设计(L1)**:
- SSE 重放:TS `AnthropicStreamedMessage` vs Rust。
- 重点:`content_block_start` 的 block index 作为 `_streamIndex`、`input_json_delta` 按 index 路由、thinking 段落(含 `signature_delta`)、tool_result-only message 合并、cache read/creation token 拆分。

**门 G4-2-4**:Anthropic 全部 fixture L1 绿;thinking effort 映射与 max_tokens ceiling 表与 TS 一致。

---

#### 4.2.5 — Chat-Completions 兼容三兄弟:Kimi / DeepSeek / GLM `[normal]`(高风险,~4–5 天)

**目标**:复用 4.2.2 的共享解析器,迁移三家 OpenAI-Compatible provider。**三家可并行开发,但建议按 Kimi → DeepSeek → GLM 顺序合入**(Kimi 最复杂,GLM 最简单)。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.5.1 | 实现 `KimiChatProvider` + `KimiFiles` + `kimi-schema.ts` | `reasoning_content` 读写、`kimi-schema.ts` 工具参数归一化、`extra_body.thinking`、`max_tokens→max_completion_tokens` 归一、`stream_options.include_usage`、视频上传 `ms://<file-id>` |
| 4.2.5.2 | 实现 `DeepSeekChatProvider` | 封装 `OpenAILegacyChatProvider`,`reasoningKey` 默认扫描、`deepseek-reasoner/chat/v4` capability |
| 4.2.5.3 | 实现 `GLMChatProvider` | 基本 Chat Completions、无 thinking、空文本过滤、capability 返回 `UNKNOWN` |
| 4.2.5.4 | L1 SSE fixture | 每家独立 fixture 目录:`sse/kimi/`、`sse/deepseek/`、`sse/glm/`;Kimi 上传做 L1 文件 fixture |

**对照测试设计(L1)**:
- 复用 4.2.2 的共享解析器,重点测 provider 特有的 message 转换差异:
  - Kimi:`reasoning_content` 往返、`kimi-schema` 参数清理、`extra_body` 合并。
  - DeepSeek:`reasoning_content`/`reasoning_details`/`reasoning` 多 key 扫描。
  - GLM:空字符串 text part 被过滤、多媒体内容抛错。

**门 G4-2-5**:Kimi/DeepSeek/GLM 各自全部 SSE fixture L1 绿;三家共享 `chat-completions-stream` 解析器无重复实现。

---

#### 4.2.6 — Google GenAI provider `[normal]`(高风险,~3–4 天)

**目标**:迁移 `google-genai.ts`。**使用 @google/genai SDK 而非 SSE,流对象是 SDK 的 async generator**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.6.1 | 实现 `GoogleGenAIChatProvider` | `contents`/`config` 构造、system_instruction、tool function_declarations、Vertex AI 分支 |
| 4.2.6.2 | 实现 response 解析 | `candidates[0].content.parts`:text/thought/function_call、function_response 排序、finishReason 映射 |
| 4.2.6.3 | AbortSignal 兼容 | Google SDK 不原生支持 abort,需手动 race `abortPromise` |
| 4.2.6.4 | thinking 配置 | `gemini-3` 用 `thinking_level`,其他用 `thinking_budget`;`off` 映射为 `MINIMAL + include_thoughts=false` |
| 4.2.6.5 | L1 fixture | 录制/构造 Google SDK 流 chunk(非 SSE,是 JSON chunk) → 比对 `StreamedMessagePart` 序列 |

**对照测试设计(L1)**:
- fixture 为 Google SDK 产出的 chunk JSON 数组(不是 SSE 字节),TS 与 Rust 用同一 fixture。
- 重点:tool-call id 构造 `{tool_name}_{id}`、`function_response` 按 assistant message 的 tool_calls 顺序排序、thought parts、`usageMetadata` token 拆分。

**门 G4-2-6**:Google GenAI 全部 fixture L1 绿;Vertex AI 配置分支至少通过构造参数对照(不发真请求)。

---

#### 4.2.7 — CoreHost provider factory + L2/L3 门 `[plan]`(高风险,~3–4 天)

**目标**:把全部 provider 接入 Rust Host 的 provider 路由,让 `setModel`/`getConfig` 能按模型别名选择 provider,并通过 L2/L3 对照。**依赖 4.2.0–4.2.6 全部 provider 实现**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.2.7.1 | `kosong-rs` provider factory | 根据 `provider_id`/`model`/`base_url`/`api_key` 构造对应 provider,对齐 `packages/kosong/src/providers/index.ts:createProvider` |
| 4.2.7.2 | `CoreHost` provider 可切换 | `config.provider` 从 `ProviderConfig` 扩展到支持 provider_id + modelAlias;`setModel` 路由到对应 provider |
| 4.2.7.3 | `getConfig`/`getOdyConfig` provider 信息 | 返回当前 provider id、model、capability |
| 4.2.7.4 | L2 对照 | 通过 `SDKRpcClient` 调用 `setModel`/`getConfig`,比对 TS vs Rust 返回值 |
| 4.2.7.5 | L3 对照 | mock provider 固定输出多轮 tool-call scenario,比对事件流顺序与 payload |
| 4.2.7.6 | 📊 基准 | 流式首字节延迟(TTFB)与解析吞吐(Rust reqwest+tokio vs TS),记录边界税 |

**对照测试设计(L2/L3)**:
- L2:fixture 含 provider 配置 → 两后端 `getConfig` 返回的 `provider.id`/`modelAlias`/`modelCapabilities`/`thinkingLevel` 逐字段一致。
- L3:mock provider scenario(如 `multi-turn-tool`)在 TS 与 Rust 后端跑,归一化后事件序列 `toEqual`。

**门 G4-2**:
- 4.2.0–4.2.6 每子阶段 L1 对照 100% 绿(逐个 Go);
- `StreamedMessage` 归一化序列逐值一致,**尤其 tool-call-id 与 usage**;
- `CoreHost` provider 切换 L2 绿;
- mock provider 多轮 tool-call L3 绿;
- 📊 流式首字节延迟与解析吞吐对照(Rust reqwest+tokio vs TS),记录边界税。
- **No-Go 信号**:某 provider 流语义在 Rust 无法逐值复刻(如 Google GenAI function_response 排序)→ 该 provider 保留 TS,host 经混合后端回调;**整层不因单 provider 卡死**。

---

### 4.3 — `agent` 编排核心 → Rust(最高风险,拆为 10 子阶段)

**为什么最高风险**:agent + 底层 loop 合计约 15.7K LOC,横跨 `packages/agent-core/src/agent/` 与 `packages/agent-core/src/loop/`。它是状态机、事件流、落盘格式、权限策略、多模式会话(prompt injection)、后台任务、定时任务、压缩策略的集合体。任何一处的语义漂移都会静默改变产品行为,且「事件流逐值一致」是 L3 对照的硬目标,所以必须把 4.3 拆成**可独立合入、独立回滚、独立对照**的子阶段。

**为什么必须拆成 10 个子阶段**:
- 当前文档把 4.3 压成 5 个「子单元」,但缺少**明确的子阶段边界、前置依赖、独立闸门**。AI 执行时容易把「迁 turn 状态机」和「迁 compaction」混为一谈,导致遗漏 records 互读、忘记权限策略、跳过 session-mode 注入器等。
- 真实依赖链是:**records → {context, config/usage/tool, permission, loop} → turn → {compaction, session-mode/injection, background/cron} → Agent 集成**。不按这个顺序合入,对照测试无法定位故障源。
- 每个子阶段都应产生一个**可运行的 Rust 模块 + 独立对照门**,而不是等全部 Agent 代码写完才统一测。

**拆分原则与依赖关系**(经源码核对 `packages/agent-core/src/agent/*`):
- 按数据/控制流拆分:持久层 → 内存状态层 → 配置/工具/权限 → 无状态循环 → Turn 编排 → 上层策略/后台任务。
- **4.3.0 是最硬前置**:records schema(`AgentRecordEvents`) 是所有 WAL 记录与跨版本互读的契约。虽然它的类型定义里引用了 context/config/permission/tool/usage/compaction/session-mode 的类型,但这些 import 是**纯类型**的;运行时 records 不依赖这些模块,因此 records 可以先落地,为其他模块提供 record 写入契约。
- **4.3.1 Context 依赖 4.3.0 + injection 生命周期接口**:ContextMemory 在 `clear`/`undo`/`applyCompaction` 中会调用 `agent.injection.onContextClear/onContextCompacted/onContextMessageRemoved`,所以 context 需要 injection 提供生命周期回调接口(但不需要注入器全部实现)。
- **4.3.2 Config / Usage / Tool 可与 4.3.1/4.3.3/4.3.4 并行,但 Skill 的 prompt 触发能力需等 4.3.5**:SkillManager.`recordActivation` 在 TS 中直接调用 `agent.turn.prompt()`,因此 Skill 的「用户 /slash 触发 prompt」能力必须等 TurnFlow 就绪;Skill 的注册/渲染/不可用提示可在 4.3.2 提前完成。
- **4.3.3 Permission 依赖 4.3.0 + 4.3.2 的 Tool 信息**:permission policies(如 `file-access-ask`、`plan-mode-tool-approve`)需要 `agent.tools` 的 active tool 列表和 tool 元数据。
- **4.3.4 Stateless loop engine 可与 4.3.1/4.3.2/4.3.3 并行**:loop 模块(`loop/types.ts`、`run-turn.ts`、`turn-step.ts`、`tool-call.ts`)只依赖 kosong 类型、`ExecutableTool` 抽象和 hooks,不直接依赖 context/records/permission。对照测试可完全用 mock LLM + mock tools 完成。
- **4.3.5 Turn flow 是核心硬门,依赖 4.3.1 + 4.3.2 + 4.3.3 + 4.3.4**:TurnFlow 在 `runOneTurn` 中直接调用 context/usage/config/tools/permission/injection/session-mode/compaction,并通过 `runTurn` 驱动 loop engine。
- **4.3.6 / 4.3.7 / 4.3.8 都依赖 4.3.5**:它们都在 turn 生命周期内被调用(compaction 的 `beforeStep`/`afterStep`、injection 的 `inject()`、session-mode 的 enter/exit、background 通过 turn.steer 注入、cron 通过 turn.steer 注入)。
- **4.3.6 还额外依赖 Phase 1-A 真分词器一致**:此点保持。
- **4.3.9 是 G4-3 真正硬门**:只有全部子模块组装进 `Agent` 并接入 `CoreHost` 后,L2/L3/L4 才绿。

**Rust 落地**(实际) `agent-rs` crate(23K LOC,与 `kaos-rs`/`kosong-rs` 并列) + 模块命名 `agent_loop/`(替代 `loop/`,Rust 关键字回避)。`ody-host` 依赖 `agent-rs` 并持有 `Agent` 实例。全部 10 子阶段已实现 — 路线图规划时预估 `ody-host/src/agent/`,实际独立为 `agent-rs` crate。

**建议执行模式**（见 §3 表格列）:
- **4.3.0 / 4.3.3 / 4.3.4 / 4.3.5 / 4.3.7 / 4.3.9 → plan 模式**：records schema 是全部子模块的契约；permission 是安全边界；loop engine 事件模型与 hooks 决定并发语义；turn flow 是核心集成面；session-mode + injection 是多模式工作流架构；4.3.9 是最终组装与 CoreHost 集成。这六处先做 plan 可避免结构性返工。
- **4.3.1 / 4.3.2 / 4.3.6 / 4.3.8 → normal 模式**：context projection、config/usage/tool/skill、compaction 算法、background/cron 的接口在 plan 阶段已划定，实现本身是 TS 逻辑的机械迁移，靠 L1/L3 测试钉死即可。
- **4.3 无 design 模式子任务**：agent 整体架构（`agent-rs` crate、模块边界 mirroring TS、`CoreHost` 持有 `Agent`）在路线图中已锁定；4.3 是约束下的迁移，不是开放设计。若执行时发现 Rust 所有权/并发模型与 TS 的共享可变状态存在根本性冲突，则升级为 design 模式重新决策；否则按 plan → normal 推进。

#### 4.3.0 — Records & persistence foundation `[plan]`(中风险,~3–4 天)

> **模式说明(Rubric §3.0 规则 3)**:本子阶段标 plan,但 `AgentRecordEvents` WAL schema 是**定错难回滚的持久化契约**。进入时第一步先做一次 design-lite 终态决策——枚举 wire 格式 / 事件 schema 候选与取舍(尤其 v1.1→v1.3 迁移兼容性),记录后再进入下方 TDD 条目。

**目标**:先把「会话的真相来源」钉死。records 是其余所有子阶段的地基:context、config、permission、tools 都通过 `logRecord()` 写 WAL;恢复时通过 `replay()` 重建内存状态。records 格式必须先稳定,否则后续子阶段的对照测试无从谈起。

**关键约束(源码核对)**:`AgentRecordEvents`(`records/types.ts`)的类型定义会引用 `LoopRecordedEvent`、`AgentConfigUpdateData`、`ContextMessage`、`PermissionMode`、`SessionModeKind`、`UserToolRegistration`、`UsageRecordScope`、`CompactionResult` 等下游类型,但这些 import 都是**纯类型**(`import type`)。运行时 `AgentRecords.logRecord`/`replay`/`restore` 不调用任何下游模块的业务逻辑(`restoreAgentRecord` 在 4.3.9 组装后才会把记录分发给各子模块),因此 4.3.0 可以先独立落地并为后续模块提供 record schema 契约。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.0.1 | 新建 `rust-ody/crates/agent-rs` crate,加入 workspace | 更新 `rust-ody/Cargo.toml` workspace members |
| 4.3.0.2 | 迁移 `AgentRecord` 类型与 `AgentRecordEvents` | 对齐 `packages/agent-core/src/agent/records/types.ts` 全部事件 schema |
| 4.3.0.3 | 实现 `InMemoryAgentRecordPersistence` + `FileSystemAgentRecordPersistence` | 对齐 `persistence.ts`:append/rewrite/flush/close、批量写入、目录 sync、截断尾行容忍 |
| 4.3.0.4 | 实现 `BlobStore` | 对齐 `blobref.ts`:data URI → blobref 卸载、blobref → data URI 再水化、LRU、去重、`blobref:<mime>;<hash>` 格式 |
| 4.3.0.5 | 实现 wire migration(v1.1/v1.2/v1.3) | 对齐 `records/migration/`,支持旧格式读、新格式写 |
| 4.3.0.6 | 实现 `AgentRecords`(log/replay/restore) | 对齐 `records/index.ts`:metadata 首行、`restoring` 标记、订阅者、rewrite-on-migration |
| 4.3.0.7 | L4 records 互读 fixture | TS 写 → Rust 读;Rust 写 → TS 读;含 blobref 的场景 |

**对照测试设计(L4)**:
- 构造同一份 record 序列,分别用 TS 与 Rust 持久化,再交叉读取,断言归一化后 `toEqual`。
- blobref 场景:TS 写含大 data URI 的 record → Rust 读回必须再水化为 data URI;反之亦然。
- 截断尾行:模拟崩溃后最后一行不完整,Rust 与 TS 须以同样方式忽略或报错。

**门 G4-3-0**:`agent-rs` 编译通过;records 序列化/反序列化 L1 100% 绿;TS↔Rust 双向互读 L4 绿;wire migration fixture 绿。**此门不绿,禁止进入任何后续 agent 子阶段。**

**No-Go 信号**:records 格式无法双向互读 → 整个 agent 迁移停止,Phase 4 停在「kaos+kosong Rust 化、agent 留 TS」的中间终态。

---

#### 4.3.1 — Context & projection `[normal]`(中风险,~3–4 天)

**目标**:迁移 `ContextMemory` 与 `projector`。context 是 turn 与 LLM 之间的桥梁,负责维护对话历史、token 计数、开放 step/tool-exchange、undo、compaction apply。

**依赖说明**:ContextMemory 在 `clear`/`undo`/`applyCompaction` 中会调用 `agent.injection.onContextClear/onContextCompacted/onContextMessageRemoved`,因此 4.3.1 需要 injection 提供**生命周期回调接口**,但不需要注入器完整实现。4.3.1 与 4.3.7 的开发可并行,只要先约定接口。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.1.1 | 迁移 `ContextMessage` / `PromptOrigin` / `AgentContextData` 类型 | 对齐 `agent/context/types.ts` |
| 4.3.1.2 | 实现 `ContextMemory` | 对齐 `agent/context/index.ts`:appendUserMessage/appendMessage/appendLoopEvent/appendSystemReminder/clear/undo/applyCompaction、开放 step 跟踪、deferred messages |
| 4.3.1.3 | 实现 `project()` 与 `dropOrphanToolResults()` | 对齐 `agent/context/projector.ts`:partial/empty assistant 过滤、相邻 user message 合并、孤儿 tool result 丢弃 |
| 4.3.1.4 | 实现 token 计数 | `tokenCount` / `tokenCountWithPending` 与 TS `estimateTokensForMessages` 一致 |
| 4.3.1.5 | 实现 `notification-xml` | 对齐 `agent/context/notification-xml.ts` |
| 4.3.1.6 | L1 + L3 fixture | 输入 record 序列 → 投影出的 message 数组;context 事件流 |

**对照测试设计(L1 + L3)**:
- L1:给定同一份 record 序列,TS `context.messages` 与 Rust `context.messages()` 归一化后逐字段一致(重点:相邻 user 合并、孤儿 tool result 丢弃、partial 过滤)。
- L3:通过同一组 `appendLoopEvent` 调用,比对 `context.append_message` / `context.append_loop_event` / `context.clear` / `context.undo` / `context.apply_compaction` 事件及落盘 record。

**门 G4-3-1**:context projection L1 100% 绿;context 事件流 L3 绿;undo 与 compaction apply 后的 token 计数与 TS 一致。

---

#### 4.3.2 — Config / usage / tool & skill registry `[normal]`(中风险,~3–4 天)

**目标**:迁移 Agent 的「配置面」和「工具面」。ConfigState 是所有 RPC 读配置的入口;ToolManager 决定模型能看到哪些工具;UsageRecorder 是计费/限流依据。

**依赖说明**:Config、Usage、ToolManager 本身只依赖 records 和 kosong 类型,可与 4.3.1/4.3.3/4.3.4 并行。**SkillManager 的 prompt 触发路径依赖 TurnFlow**:TS 中 `SkillManager.recordActivation` 在传入 `input` 时会调用 `agent.turn.prompt(input, origin)`,因此 4.3.2 可先完成 Skill 注册/渲染/不可用提示,但「用户 slash 触发 prompt」的完整端到端能力需等 4.3.5 就绪后再启用。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.2.1 | 实现 `ConfigState` + thinking 解析 | 对齐 `agent/config/index.ts` + `agent/config/thinking.ts`:cwd/modelAlias/profileName/thinkingLevel/systemPrompt 更新,`providerConfig` 懒解析 |
| 4.3.2.2 | 实现 `UsageRecorder` | 对齐 `agent/usage/index.ts`:beginTurn/endTurn/record、session/turn/goal 作用域 |
| 4.3.2.3 | 实现 `ToolManager` + tool types | 对齐 `agent/tool/index.ts`:builtin/user/MCP 工具注册、`setActiveTools`、mcpAccessPatterns、`loopTools` 排序、工具冲突处理 |
| 4.3.2.4 | 实现 `SkillManager` | 对齐 `agent/skill/index.ts`:skill 注册、激活、工具根目录扩展 |
| 4.3.2.5 | L2 fixture | setModel/setThinking/getConfig/getUsage/getTools/registerTool/unregisterTool/setActiveTools |

**对照测试设计(L2)**:
- 通过 `SDKRpcClient` 调用 `setModel`/`setThinking`/`getConfig`/`getUsage`/`getTools`/`registerTool`/`unregisterTool`/`setActiveTools`,比对 TS 与 Rust 返回值。
- 重点:`getConfig` 返回的 `provider`/`modelCapabilities`/`thinkingLevel` 与 TS 一致;`getTools` 的 active/source 字段一致。

**门 G4-3-2**:所有配置/工具/用量相关 RPC 的 L2 对照绿;`loopTools` 在启用/禁用/注册/注销组合后排序与 TS 一致。

---

#### 4.3.3 — Permission system `[plan]`(中风险,~3–4 天)

**目标**:迁移 `PermissionManager` 与全部决策策略。权限是安全边界,必须逐策略复刻(尤其是 file-access、plan-mode-guard、yolo/auto/manual 模式差异)。

**依赖说明**:PermissionManager 依赖 records 与 loop 类型(`PrepareToolExecutionResult`);policies(如 `file-access-ask`、`plan-mode-tool-approve`)在评估时需要 `agent.tools` 的 active tool 列表和 tool 元数据,因此**4.3.3 依赖 4.3.2 的 ToolManager 落地**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.3.1 | 迁移 `PermissionManager` | 对齐 `agent/permission/index.ts`:mode 继承、session approval rules、approval 请求/回调、telemetry |
| 4.3.3.2 | 迁移所有 policy | 对齐 `agent/permission/policies/*`:yolo-mode-approve、auto-mode-approve、file-access-ask、plan-mode-guard-deny、plan-mode-tool-approve、pre-tool-call-hook、session-approval-history、fallback-ask 等 |
| 4.3.3.3 | 迁移 `matches-rule` | 对齐 `agent/permission/matches-rule.ts` |
| 4.3.3.4 | L3 fixture | 不同 mode + 不同 tool + 不同规则 → 批准/拒绝/询问/阻塞结果 |

**对照测试设计(L3)**:
- scenario 覆盖:manual 模式下 file write 触发 requestApproval;yolo 模式下同工具直接通过;plan-mode 下非白名单工具被 deny;session approval 缓存命中后自动通过。
- 比对 `tool.call.started`/`tool.result` 事件及 `permission.record_approval_result` record。

**门 G4-3-3**:所有 policy 的 L3 fixture 绿;manual/yolo/auto/plan 四种模式对同一工具的决策与 TS 一致。

---

#### 4.3.4 — Stateless loop engine `[plan]`(高风险,~4–5 天)

**目标**:迁移 `packages/agent-core/src/loop/` 下整个无状态 step 引擎。它是 turn 的「心脏」,与 Agent 实例解耦,可独立对照。包含 `runTurn`/`executeLoopStep`/`tool-call`/`events`/`retry`/`tool-access`。

**依赖说明**:loop 模块在源码中**不直接 import context/records/permission/turn/config**。它只依赖 kosong 类型、`ExecutableTool` 抽象、`LLM` trait 和通过 `LoopHooks` 注入的回调。因此 4.3.4 可与 4.3.1/4.3.2/4.3.3 **完全并行**开发,对照测试用 mock LLM + mock tools 即可,无需等待上层 Agent 组装。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.4.1 | 迁移 loop 类型与事件 | 对齐 `loop/types.ts` + `loop/events.ts`:ExecutableTool/ToolExecution/LoopHooks/LoopEvent/LoopRecordedEvent/createLoopEventDispatcher |
| 4.3.4.2 | 实现 `executeLoopStep` | 对齐 `loop/turn-step.ts`:build messages → LLM chat → 流式 delta 分发 → tool-call 解析 → 工具执行 |
| 4.3.4.3 | 实现 `runTurn` | 对齐 `loop/run-turn.ts`:step 循环、max steps、abort 处理、retry、usage 累加、shouldContinueAfterStop hook |
| 4.3.4.4 | 实现 tool-call 调度 | 对齐 `loop/tool-call.ts`:串行/并行工具执行、参数校验、错误处理、abort、进度事件 |
| 4.3.4.5 | 实现 retry 与 tool-access | 对齐 `loop/retry.ts` + `loop/tool-access.ts` |
| 4.3.4.6 | L3 fixture | mock LLM 固定输出 text/tool-call/多 step → 比对事件序列 |

**对照测试设计(L3)**:
- 用同一个 mock LLM(已在 4.0 做 L1 对照)和同一组 ExecutableTool,分别驱动 TS `runTurn` 与 Rust `run_turn`,比对 `LoopEvent` 序列(归一化 stepUuid/time)。
- 必测:单 text 结束、单 tool-call 后 continue、并行 tool-calls、工具执行失败、max steps 中断、abort 中断、retry 触发。

**门 G4-3-4**:loop engine 全部 L3 fixture 绿;事件顺序与 payload 与 TS 一致;max steps / abort / retry 边界行为一致。**这是 4.3.5 的硬前置。**

---

#### 4.3.5 — Turn flow & LLM adapter `[plan]`(最高风险,~5–6 天)

**目标**:迁移 `TurnFlow` 及其直接依赖(`kosong-llm`/`remote-kosong-llm`/`tool-dedup`/`canonical-args`)。它是 Agent 级别的 turn 状态机,承接 loop engine,叠加 goal continuation、steer buffer、tool-call 去重、telemetry、错误分类。

**依赖说明(源码核对)**:TurnFlow(`agent/turn/index.ts`)在 `runOneTurn`/`runStepLoop` 中直接调用 `agent.context`、`agent.usage`、`agent.config`、`agent.tools`、`agent.permission`、`agent.injection`、`agent.sessionMode`、`agent.fullCompaction`、`agent.microCompaction`、`agent.splitPlanCheckpoint`、`agent.normalModeTaskCheckpoint`,并通过 `runTurn` 驱动 loop engine。因此**4.3.5 是 4.3.1/4.3.2/4.3.3/4.3.4 的硬后置集成点**,不可提前。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.5.1 | 实现 `TurnFlow` | 对齐 `agent/turn/index.ts`:prompt/steer/cancel/wait、turnWorker/runOneTurn/driveGoal、activeTurn 生命周期、steer buffer |
| 4.3.5.2 | 实现 `KosongLLM` | 对齐 `agent/turn/kosong-llm.ts`:把 kosong `generate()` 包装成 loop `LLM` trait,处理 auth、request log context |
| 4.3.5.3 | 实现 `RemoteKosongLLM` | 对齐 `agent/turn/remote-kosong-llm.ts`:worker 模式下的 LLM 代理 |
| 4.3.5.4 | 实现 `ToolCallDeduplicator` | 对齐 `agent/turn/tool-dedup.ts`:same_step / cross_step 检测、结果复用 |
| 4.3.5.5 | 实现 telemetry 归一化 | `canonical-args.ts` 的 tool input/output 规范化 |
| 4.3.5.6 | L3 fixture | 单 turn / 多 step / goal continuation / steer / cancel / tool-call 去重 |

**对照测试设计(L3)**:
- scenario 覆盖:`prompt` → 单 assistant 回复;`prompt` → tool-call → tool-result → turn end;steer 进入 active turn 的 buffer;cancel 中止 turn;goal active 时的 continuation prompt 注入。
- 比对 `turn.started` / `turn.step.*` / `assistant.delta` / `thinking.delta` / `tool.call.*` / `tool.result` / `turn.ended` 事件序列。

**门 G4-3-5**:TurnFlow 全部 L3 fixture 绿;goal continuation、steer buffer、tool-call 去重行为与 TS 一致。

---

#### 4.3.6 — Compaction strategies `[normal]`(高风险,~4–5 天)

**目标**:迁移 `agent/compaction/*`(full/micro/strategy/split-checkpoint/normal-task-checkpoint/render-messages)。压缩是最难对齐的子阶段之一,因为它直接调用 LLM 生成 summary,且依赖分词器一致性。

**依赖说明**:FullCompaction 直接读取 `agent.context.history`、`agent.context.tokenCountWithPending`,调用 `agent.records`、`agent.config`、`agent.tools.loopTools`,并通过 `runStepLoop` 的 hooks 在 turn 中触发。因此**4.3.6 依赖 4.3.1 + 4.3.2 + 4.3.5**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.6.1 | 实现 `CompactionStrategy` / `DefaultCompactionStrategy` | 对齐 `agent/compaction/strategy.ts`:threshold、compact count 计算、overflow 回退 |
| 4.3.6.2 | 实现 `FullCompaction` | 对齐 `agent/compaction/full.ts`:begin/cancel/block、compaction worker、retry、summary 提取、todo list 后缀 |
| 4.3.6.3 | 实现 `MicroCompaction` | 对齐 `agent/compaction/micro.ts` |
| 4.3.6.4 | 实现 `SplitPlanCheckpoint` + `NormalModeTaskCheckpoint` | 对齐 `agent/compaction/split-checkpoint.ts` + `normal-task-checkpoint.ts` |
| 4.3.6.5 | 实现 `renderMessagesToText` | 对齐 `agent/compaction/render-messages.ts` |
| 4.3.6.6 | L1 + L3 fixture | 超长 history → compact 后 records;compaction 事件流 |

**对照测试设计(L1 + L3)**:
- L1:**用录制/固定的 LLM summary fixture** 喂给压缩逻辑,比对 compact 后的 history 与 records(不依赖真实 LLM 输出一致性)。
- L3:自动触发的 compaction 事件序列(`compaction.started`/`compaction.blocked`/`compaction.completed`/`compaction.cancelled`/`error`)与 TS 一致。
- 分词器前提:4.3.6 必须等 Phase 1-A 的 Wasm 分词器可用,或复用 tiktoken-rs 并校准到与 TS 一致。

**门 G4-3-6**:压缩逻辑 L1 100% 绿(固定 summary);compaction 事件 L3 绿;micro/split/normal-task checkpoint 行为与 TS 一致。

**No-Go 信号**:分词器无法对齐导致 compact count 不一致 → 该子阶段保留 TS 回调,登记 gap,不阻塞 4.3.7/4.3.8/4.3.9。

---

#### 4.3.7 — Session modes & prompt injection `[plan]`(高风险,~4–5 天)

**目标**:迁移 `agent/session-mode/` 与 `agent/injection/`。session mode(plan/design/office-hours/game-design)是产品的核心工作流,enter/exit/cancel 会切换 context partition、变更模型、注入特定 prompt。InjectionManager 负责在 step 前注入 goal/reminder/todo/skills/knowledge 等内容。

**依赖说明**:SessionMode 直接操作 `agent.config`、`agent.context`、`agent.records`、`agent.replayBuilder`,并在 enter/exit/cancel 中调用 `agent.setContextMode`。InjectionManager 的注入器读取 `agent.sessionMode`、`agent.context.history`、`agent.tools`、`agent.skills`、`agent.goals`,并在 `inject()` 中调用 `agent.context.appendUserMessage`/`appendSystemReminder`。因此**4.3.7 依赖 4.3.1 + 4.3.2 + 4.3.5**;其中 injection 的生命周期接口(`onContextClear/onContextCompacted/onContextMessageRemoved`)需与 4.3.1 约定,可与 4.3.1 并行开发。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.7.1 | 迁移 `SessionMode` + behaviors | 对齐 `agent/session-mode/index.ts` + `behaviors/*`:enter/exit/cancel、model 恢复、context partition 切换、plan/design 文件读写 |
| 4.3.7.2 | 迁移 topic-generator / directory / reviewer / model-auth | 对齐 `agent/session-mode/*` |
| 4.3.7.3 | 迁移 `InjectionManager` + 所有 injector | 对齐 `agent/injection/manager.ts` + 各 mode injector + goal/todo-list/knowledge-microagent/parts-manifest |
| 4.3.7.4 | 迁移 `ReplayBuilder` | 对齐 `agent/replay/index.ts`:记录 mode/config/permission/message 更新,用于 resume 校验 |
| 4.3.7.5 | L3 fixture | enter plan/design/office-hours/game-design → 模型切换事件、partition 切换后的 context 事件、注入的 prompt 内容 |

**对照测试设计(L3)**:
- scenario:进入 plan mode → 注入 system reminder → 运行一步 → 退出 → 恢复模型。
- 比对 `session_mode.enter` / `session_mode.exit` / `agent.status.updated` / `context.append_message` / `turn.step.*` 事件序列与 payload。
- 重点:direct plan↔design 切换时的 `_preModeModelAlias` 恢复、partition defer(开放 step 时 exit 不立即切换)。

**门 G4-3-7**:session-mode enter/exit/cancel L3 绿;injection 在 step 前注入的内容与 TS 逐字段一致;replay 记录与 TS 一致。

---

#### 4.3.8 — Background tasks & cron `[normal]`(中风险,~3–4 天)

**目标**:迁移 `agent/background/*` 与 `agent/cron/manager.ts`。后台任务管理 bash/subagent/question 任务;cron 调度定时 steer。

**依赖说明**:BackgroundManager 在任务结束时调用 `agent.turn.steer(content, origin)` 和 `agent.context.appendUserMessage`。CronManager 通过 `agent.turn.hasActiveTurn` 判断空闲,并通过 `agent.turn.steer(content, origin)` 注入定时 prompt。因此**4.3.8 依赖 4.3.5**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.8.1 | 迁移 `BackgroundManager` + task types | 对齐 `agent/background/index.ts` + `task.ts` + `agent-task.ts` + `process-task.ts` + `question-task.ts`:register/stop/wait/list/getOutputSnapshot、terminal effects、notification |
| 4.3.8.2 | 迁移 `BackgroundTaskPersistence` | 对齐 `agent/background/persist.ts` |
| 4.3.8.3 | 迁移 `CronManager` | 对齐 `agent/cron/manager.ts`:store/scheduler、loadFromDisk、fire → steer、stale 判断、SIGUSR1 |
| 4.3.8.4 | L3 fixture | 后台任务生命周期事件;cron fire → steer 事件 |

**对照测试设计(L3)**:
- 后台任务:mock 一个快速完成的任务,比对 `background.task.started` / `background.task.terminated` 事件及 `getBackground` 返回值。
- cron:用 `ODY_CRON_MANUAL_TICK=1` 手动 tick,比对 `cron.fired` 事件与 steer 后产生的 turn 事件。
- 并发顺序归一化:多任务同时完成时,按 taskId 排序后再比对(纪律 10)。

**门 G4-3-8**:后台任务与 cron 的 L3 fixture 绿;任务输出快照、持久化、terminal notification 行为与 TS 一致。

---

#### 4.3.9 — Agent orchestrator & CoreHost integration `[plan]`(最高风险,~4–5 天)

**目标**:把前面 9 个子阶段组装成完整的 `Agent` 实例,接入 `ody-host` 的 `CoreHost`,并通过 L2/L3/L4 硬门。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.3.9.1 | 实现 `Agent` 组装器 | 对齐 `agent/index.ts`:构造所有子模块、setContextMode、refreshLlm、llm getter、generate wrapper、useProfile、resume |
| 4.3.9.2 | 实现 `AgentAPI` RPC 路由 | 在 `ody-host` 把 `prompt`/`steer`/`cancel`/`setModel`/`setThinking`/`setPermission`/`enterPlan`/`getConfig`/… 路由到 `Agent` |
| 4.3.9.3 | 实现 resume 路径 | `AgentRecords.replay()` → 恢复 context/config/permission/session-mode/tools/background/cron |
| 4.3.9.4 | L2 对照 | 所有 `AgentAPI` 方法 TS vs Rust 返回值一致 |
| 4.3.9.5 | L3 对照 | mock provider 多轮 tool-call scenario、session-mode handoff scenario、background/cron scenario |
| 4.3.9.6 | L4 对照 | 完整会话:TS 创建 → Rust resume → 继续 → TS 再 resume |
| 4.3.9.7 | 📊 基准 | 常驻内存、冷启动、空闲 CPU 对照(G3 ADR 基线) |

**对照测试设计(L2 + L3 + L4)**:
- L2:每个 `AgentAPI` 方法单独构造输入,比对 JSON 响应。
- L3:确定性 scenario 覆盖:hello-world、file-edit、multi-turn-tool、enter/exit plan mode、background task、cron fire。
- L4:录制真实会话脚本,两后端重放,比对响应、事件流、落盘 records。

**门 G4-3**:
- 4.3.0–4.3.8 每子阶段门绿;
- 所有 `AgentAPI` L2 对照绿;
- mock provider 多轮 tool-call、session-mode handoff、background/cron L3 绿;
- L4 完整会话重放绿;
- records 双向互读 100% 绿;
- 📊 常驻内存 / 冷启动 / 空闲 CPU 实测改善(兑现 G3 ADR)。
- **No-Go 信号**:事件流顺序在并发场景无法稳定对齐、records 互读不兼容、或任一关键子阶段(4.3.0/4.3.1/4.3.4/4.3.5)未过门 → agent 整体回退到 TS,Phase 4 停在「kaos+kosong 已 Rust 化、agent 留 TS」这一**合法中间终态**。

---

### 4.4 — `tools/builtin` 拆分迁移(中风险,拆为 9 子阶段)

**为什么必须拆**:当前 4.4 把约 7.3K LOC 的工具清单压成「逐个迁移」,缺少明确的依赖分组。源码审计(`packages/agent-core/src/tools/builtin/*.ts` 以及 `tools/background/*.ts`、`tools/cron/*.ts`)显示,工具与工具之间、工具与 4.3 agent 子系统之间存在清晰分层:纯 I/O 工具可早早对照,而 session-mode、background/cron、collaboration 工具强依赖 4.3 的内存状态机。若不分组,AI 执行时容易先迁 `AskUser`/`Agent` 却发现 `session-mode` 未就绪,或先迁 `Bash` 背景版却发现 `BackgroundManager` 未落地。

**Rust 落地**:新增 `tools-rs` crate(与 `kaos-rs`/`kosong-rs`/`agent-rs` 并列,若工具与 host 耦合过重也可先落地在 `ody-host/src/tools/` 内),把工具按依赖族分组。`ody-host` 的 `ToolManager` 等价物最终通过 `tools-rs` 注册全部 builtin 工具。

**拆分原则与依赖关系**(经源码核对 `packages/agent-core/src/tools/builtin/*` 与 `packages/agent-core/src/agent/tool/index.ts`):
- 按「基础设施 → 无状态 I/O → 主机注入服务 → agent 上层状态机」分层。
- **4.4.0 是硬前置**:定义 `BuiltinTool` trait(等价 TS `BuiltinTool<T>`/`ExecutableTool`)、`ToolExecution`、`ToolAccesses`、路径安全策略(`path-access.ts`)、规则匹配(`rule-match.ts`)、参数 schema 转换(`input-schema.ts`)、结果构建器(`result-builder.ts`)、文件类型嗅探(`file-type.ts`)、rg 定位器(`rg-locator.ts`)、工作区配置(`workspace.ts`)、参数校验器(`args-validator.ts`)、工具 store(`store.ts`)。这些被几乎所有工具复用,先落地避免每家重复。
- **4.4.1 文件 & shell 核心工具**与**4.4.2 Web 工具**在 4.4.0 之后可**完全并行**:它们只依赖 `kaos` 和主机注入的 fetch/search 服务,不直接依赖 agent 内存状态。
  - `WriteTool`/`EditTool` 的 plan/design 模式文件路径重定向需要 `agent.sessionMode`(4.3.7),但核心读写可在 4.4.1 先行对照;重定向路径作为 4.4.5 后的补齐项。
  - `BashTool` 的前景执行只依赖 `kaos` + `cwd`;后台执行(`run_in_background`)依赖 `BackgroundManager`(4.3.8),归入 4.4.3 就绪后的补齐。
  - `ReadMediaFileTool` 需要模型 capability 与 video uploader(video uploader 来自 4.2 的 provider),对照可用 mock。
- **4.4.3 后台 & cron 管理工具**依赖 4.3.8 的 `BackgroundManager`/`CronManager`,但不依赖 4.4.1/4.4.2,因此可与 I/O 工具并行开发( manager 在 4.3.8 交付,tool 只是其 RPC 薄壳)。
- **4.4.4 协作工具**(`AskUserQuestion`/`Skill`/`Agent`)、**4.4.5 会话模式工具**、**4.4.6 目标/状态工具**都强依赖 4.3 的 agent 核心,必须等待对应 agent 子模块就绪:
  - 4.4.4 依赖 4.3.5(TurnFlow,因为工具在 turn 内执行)、4.3.7(SessionMode/Skills,`SkillTool` 检查 `hiddenInModes`)、4.3.8(BackgroundManager,`AskUser`/`Agent` 的后台模式);`AgentTool` 还依赖 `SessionSubagentHost`。
  - 4.4.5 依赖 4.3.7(SessionMode + injection 契约 + i18n)以及部分 RPC 同步(sync artifact)。
  - 4.4.6 依赖 4.3.2 的 `ToolStore`(已在 `ToolManager` 内)和 `Agent` 的 `goals`/`checkpointCoordinator`;`SessionGoalStore` 目前属于 `session/goal`,在 4.3 路线图中未显式拆分,需作为 4.4.6 的前置缺口补齐。
- **4.4.7 质量 & 专用工具**是"叶子":`HarvestOdyMarkers` 依赖 4.4.1 的 `GrepTool` + telemetry;`ReviewTests` 依赖 4.3.7 的 `AdvancedSessionReviewer` + kaos;`RunE2ETests` 依赖 `@odysseythink/e2e-testing` 包;`SaveIdeaReport` 依赖 `agent.context` + kaos;`ShowDesignMockup` 依赖 4.3.7 的 session-mode + `rpc.openExternal`。
- **4.4.8 是 G4-4 真正硬门**:所有工具在 `ToolManager` 中注册,并在 Rust Agent turn 中跑 L2/L3 对照。**任一工具卡住只搁置该工具,不阻塞其余**,符合「单单元回滚」纪律。

**建议执行模式**（见 §3 表格列）:
- **4.4.0 / 4.4.4 / 4.4.5 / 4.4.6 / 4.4.8 → plan 模式**：4.4.0 是全部工具的 trait/策略/结果构建器契约；4.4.4 协作工具涉及 subagent/background/session-mode 多状态机交互；4.4.5 涉及多模式 enter/exit/handoff/artifact sync；4.4.6 需先补齐 `SessionGoalStore` 缺口并定义 goal/checkpoint 集成；4.4.8 是 ToolManager 注册与 L2/L3 集成门。
- **4.4.1 / 4.4.2 / 4.4.3 / 4.4.7 → normal 模式**：文件/shell、Web、后台/cron 管理、叶子专用工具在 4.4.0 契约和 4.3 子系统就绪后，属于机械迁移，L1/L3 fixture 钉死即可。
- **4.4 无 design 模式子任务**：工具分层、trait 边界、「SSH/RequestCodeReview defer」已在路线图中锁定；4.4.6 的 `SessionGoalStore` 缺口是实施缺口，不是开放设计问题。若执行时发现某工具无法在不改变 `BuiltinTool` 契约的前提下实现，再升级为 design 模式。

**工具全集(按子阶段)**:

| 子阶段 | 工具 | TS 源文件 | 主要依赖 | 备注 |
|---|---|---|---|---|
| 4.4.0 | 共享基础设施 | `agent/tool/*`, `tools/{args-validator,store,support,policy}/*` | 无(agent 类型仅为 `import type`) | 含 schema/AJV、路径策略、结果构建器 |
| 4.4.1 | Read / Write / Edit / Glob / Grep / ReadMedia / Bash | `builtin/file/*.ts`, `builtin/shell/bash.ts` | `kaos`(4.1) + workspace + 4.4.0;Write/Edit 重定向需 4.3.7;Bash 后台需 4.3.8 | 文件/shell 核心七件套 |
| 4.4.2 | FetchURL / WebSearch | `builtin/web/*.ts`, `tools/providers/*` | 4.4.0 + 主机注入 fetcher/searcher | 主机提供实现;对照用 mock provider |
| 4.4.3 | TaskList / TaskOutput / TaskStop / CronCreate / CronList / CronDelete | `tools/background/*.ts`, `tools/cron/cron-{create,delete,list}.ts` | 4.4.0 + 4.3.8 BackgroundManager/CronManager | Bash 后台执行需这些工具就绪 |
| 4.4.4 | AskUserQuestion / Skill / Agent | `builtin/collaboration/*.ts` | 4.4.0 + 4.3.5 + 4.3.7 + 4.3.8 + subagent host | 协作工具,状态机最重 |
| 4.4.5 | Enter/Exit Plan/Design;Enter/Exit Office-hours + 8 个辅助;Enter/Exit Game-design + 8 个辅助 | `builtin/planning/*.ts`, `builtin/office-hours/*.ts`, `builtin/game-design/*.ts` | 4.4.0 + 4.3.7 + i18n + RPC sync | 会话模式工作流入口 |
| 4.4.6 | CreateGoal / GetGoal / SetGoalBudget / UpdateGoal / TodoList / Checkpoint | `builtin/goal/*.ts`, `builtin/state/*.ts` | 4.4.0 + `ToolStore`(4.3.2) + `SessionGoalStore` + checkpoint coordinator | `SessionGoalStore` 是 4.3 未显式拆分的缺口 |
| 4.4.7 | HarvestOdyMarkers / ReviewTests / RunE2ETests / SaveIdeaReport / ShowDesignMockup | `builtin/code-quality/*`, `builtin/test-review/*`, `builtin/run-e2e-tests.ts`, `builtin/idea/*`, `builtin/visual/*` | 4.4.0 + 各叶子依赖(见正文) | `RequestCodeReviewTool` 来自 `@odysseythink/code-review`,本阶段单独评估 |
| 4.4.8 | ToolManager 注册与集成 | `agent/tool/index.ts` 等价物 | 全部 4.4.0–4.4.7 + 4.3.9 | L2/L3 门 |

#### 4.4.0 — Tool infrastructure & shared support `[plan]`(低风险,~2–3 天)

**目标**:先把工具公共契约和辅助函数钉死,避免后续每个工具都重复实现路径安全、schema 转换、结果截断等逻辑。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.0.1 | 迁移 `BuiltinTool`/`ToolInfo`/`ToolExecution` 类型 | 对齐 `agent/tool/types.ts` + `loop/types.ts` |
| 4.4.0.2 | 迁移 `ToolAccesses` 与访问声明 | 对齐 `loop/tool-access.ts` |
| 4.4.0.3 | 迁移路径安全策略 | 对齐 `tools/policies/path-access.ts`:workspace/absolute-outside-allowed/search 模式 |
| 4.4.0.4 | 迁移规则匹配 | 对齐 `tools/policies/rule-match.ts`:literal/glob 规则、路径 subject 匹配 |
| 4.4.0.5 | 迁移参数 schema 转换 | 对齐 `tools/support/input-schema.ts`:zod schema → JSON Schema |
| 4.4.0.6 | 迁移结果构建器 | 对齐 `tools/support/result-builder.ts`:截断、ok/error 构造 |
| 4.4.0.7 | 迁移文件类型嗅探 | 对齐 `tools/support/file-type.ts`:image/video/text/unknown 判定、图片尺寸嗅探 |
| 4.4.0.8 | 迁移 rg 定位器 | 对齐 `tools/support/rg-locator.ts`:rg 路径探测、不可用提示 |
| 4.4.0.9 | 迁移参数校验器 | 对齐 `tools/args-validator.ts`:AJV draft-07/2019/2020 选择、错误格式化 |
| 4.4.0.10 | L1 golden fixture | 路径策略用例、rule-match 用例、schema 转换用例、file-type 用例 |

**对照测试设计(L1)**:
- 路径策略:给定 cwd + workspace + 输入路径 → 预期 resolved path / 错误。
- rule-match:给定规则与工具名/参数 → 预期 match 结果。
- input-schema:复用一组 zod schema,断言 TS 与 Rust 产出的 JSON Schema 一致。
- file-type:给定文件头字节 → 预期 kind/mime/dimensions。

**门 G4-4-0**:公共基础设施 L1 100% 绿;`BuiltinTool` trait 与 `ToolExecution` 结构与 TS 一致。**此门不绿,禁止进入 4.4.1–4.4.7。**

#### 4.4.1 — File & shell core tools `[normal]`(中风险,~4–5 天)

**目标**:迁移文件/shell 核心七件套。这是产品最高频的工具,必须逐字节对齐。

**依赖说明**:核心只依赖 4.1 `kaos` + 4.4.0。`WriteTool`/`EditTool` 的 plan/design 模式文件路径重定向需要 4.3.7 `SessionMode`,可在 4.4.1 中用"不重定向"桩实现先行对照,4.4.5 后补齐。`BashTool` 的背景执行需要 4.3.8 `BackgroundManager` 和 4.4.3 的 Task 工具,前景执行无此依赖。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.1.1 | 迁移 `ReadTool` | 对齐 `file/read.ts`:行号/偏移/n_lines、行尾检测、截断、二进制拒绝 |
| 4.4.1.2 | 迁移 `WriteTool` | 对齐 `file/write.ts`:overwrite/append、父目录检查、字节计数;plan/design 重定向路径可 deferred |
| 4.4.1.3 | 迁移 `EditTool` | 对齐 `file/edit.ts`:replace_once/all、行尾 materialize、uniquness 检查 |
| 4.4.1.4 | 迁移 `GlobTool` | 对齐 `file/glob.ts`:brace expansion、MAX_MATCHES、include_dirs、mtime 排序 |
| 4.4.1.5 | 迁移 `GrepTool` | 对齐 `file/grep.ts`:rg 参数构造、输出模式、敏感文件过滤、分页 |
| 4.4.1.6 | 迁移 `ReadMediaFileTool` | 对齐 `file/read-media.ts`:image/video 判定、base64、video uploader 桩 |
| 4.4.1.7 | 迁移 `BashTool`(前景执行) | 对齐 `shell/bash.ts` 前景路径:timeout、abort、SIGTERM→SIGKILL、环境变量 |
| 4.4.1.8 | L1 + L3 fixture | 文件 I/O 纯函数级 + 经 agent turn 的事件级 |

**对照测试设计(L1 + L3)**:
- L1:构造已知目录树,对同一组 args 分别调 TS 工具与 Rust 工具,比对返回值与落盘文件字节。
- L3:mock provider 触发一次 `Read`→`Edit`→`Bash` 链,比对 `tool.call.started`/`tool.output`/`tool.result` 事件 payload。
- 行尾:CRLF 文件经 Read→Edit 后逐字节一致。
- 截断:大文件/大输出场景,截断标记位置一致。

**门 G4-4-1**:文件/shell 七件套 L1 100% 绿;L3 经 agent turn 调用事件序列一致;行尾/截断边界行为一致。

#### 4.4.2 — Web tools `[normal]`(低风险,~2–3 天)

**目标**:迁移 `FetchURL` 与 `WebSearch`。它们不访问本地 fs,只调用主机注入的网络服务。

**依赖说明**:只依赖 4.4.0 和主机注入的 `UrlFetcher`/`WebSearchProvider` 接口。TS 侧实现位于 `tools/providers/`(如 `local-fetch-url.ts`、多个 web-search provider)。这些 provider 可选择保留 TS 经混合后端回调,或在本子阶段一并迁 Rust。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.2.1 | 迁移 `FetchURLTool` | 对齐 `web/fetch-url.ts`:HttpFetchError、passthrough/extracted |
| 4.4.2.2 | 迁移 `WebSearchTool` | 对齐 `web/web-search.ts`:结果格式化、错误分类 |
| 4.4.2.3 | (可选)迁移 host provider 实现 | `tools/providers/*`:fetch-url + web-search registry |
| 4.4.2.4 | L1 + L3 fixture | mock fetcher/searcher 返回固定结果,比对输出与事件 |

**对照测试设计(L1 + L3)**:
- L1:mock `UrlFetcher`/`WebSearchProvider` 返回固定 payload,比对 Rust/TS 工具输出字符串。
- L3:在 agent turn 中调用 `WebSearch`,比对事件 payload。

**门 G4-4-2**:Web 工具 L1+L3 绿;错误分类与 TS 一致。

#### 4.4.3 — Background & cron management tools `[normal]`(中风险,~2–3 天)

**目标**:迁移后台任务管理工具(TaskList/TaskOutput/TaskStop)和 cron 管理工具(CronCreate/CronList/CronDelete)。

**依赖说明**:这些工具是 `BackgroundManager`/`CronManager` 的薄壳,依赖 4.3.8。它们本身不依赖 4.4.1/4.4.2,但 `BashTool` 的后台执行需要这些工具就绪(否则 `run_in_background=true` 应被拒绝)。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.3.1 | 迁移 `TaskListTool` | 对齐 `background/task-list.ts` |
| 4.4.3.2 | 迁移 `TaskOutputTool` | 对齐 `background/task-output.ts`:output preview、分页提示 |
| 4.4.3.3 | 迁移 `TaskStopTool` | 对齐 `background/task-stop.ts`:reason、terminal 状态 |
| 4.4.3.4 | 迁移 `CronCreateTool` | 对齐 `cron/cron-create.ts`:parse/validate/jitter/cap |
| 4.4.3.5 | 迁移 `CronListTool` | 对齐 `cron/cron-list.ts` |
| 4.4.3.6 | 迁移 `CronDeleteTool` | 对齐 `cron/cron-delete.ts` |
| 4.4.3.7 | L3 fixture | 后台任务生命周期、cron fire → steer 事件 |

**对照测试设计(L3)**:
- 后台任务:mock 快速完成任务,比对 `background.task.started`/`background.task.terminated` 与 `getBackground` 返回值。
- cron:手动 tick,比对 `cron.fired` 事件与 steer 后 turn 事件。
- 并发顺序归一化:多任务同时完成按 taskId 排序后比对。

**门 G4-4-3**:后台/cron 工具 L3 绿;任务输出快照、持久化、terminal notification 行为与 TS 一致。

#### 4.4.4 — Collaboration tools `[plan]`(高风险,~3–4 天)

**目标**:迁移 `AskUserQuestionTool`、`SkillTool`、`AgentTool`。它们是状态机最重的协作工具。

**依赖说明**:
- 三者都需要 4.3.5(TurnFlow,因为工具在 turn 内执行)和 4.4.0。
- `SkillTool` 需要 4.3.2 `SkillManager`/registry 和 4.3.7 `SessionMode`(检查 `hiddenInModes`)。
- `AskUserQuestionTool` 需要 `agent.rpc.requestQuestion`、telemetry,后台模式需要 4.3.8 `BackgroundManager`。
- `AgentTool` 需要 `SessionSubagentHost`、可选 `BackgroundManager`(后台 agent)、`ResolvedAgentProfile.subagents`。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.4.1 | 迁移 `SkillTool` | 对齐 `collaboration/skill-tool.ts`:inline skill 调用、recursion cap、mode 隐藏 |
| 4.4.4.2 | 迁移 `AskUserQuestionTool` | 对齐 `collaboration/ask-user.ts`:requestQuestion、background question task |
| 4.4.4.3 | 迁移 `AgentTool` | 对齐 `collaboration/agent.ts`:spawn/resume、foreground/background、timeout |
| 4.4.4.4 | L3 fixture | skill 调用 → system reminder;question 回答;subagent 完成/失败/后台 |

**对照测试设计(L3)**:
- Skill:调用后比对 `context.append_message` 事件中的 `<kimi-skill-loaded>` payload。
- AskUser:mock `requestQuestion` 返回固定答案,比对 `tool.output` 与 telemetry。
- Agent:mock `SessionSubagentHost` 返回固定 completion,比对 foreground/background 输出格式。

**门 G4-4-4**:协作工具 L3 绿;skill 递归深度、question dismissed、subagent 超时/取消行为一致。

#### 4.4.5 — Session-mode workflow tools `[plan]`(高风险,~3–4 天)

**目标**:迁移 plan/design/office-hours/game-design 的 enter/exit 工具及辅助工具。

**依赖说明**:全部强依赖 4.3.7 的 `SessionMode` + injection 契约 + i18n;部分(`SyncOfficeHoursArtifactTool`/`SyncGameDesignArtifactTool`)还依赖 `agent.mcp`/gbrain CLI/RPC sync。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.5.1 | 迁移 planning enter/exit 工具 | `planning/enter-plan-mode.ts`, `planning/exit-plan-mode.ts`(含 E2E enrichment), `planning/enter-design-mode.ts`, `planning/exit-design-mode.ts` |
| 4.4.5.2 | 迁移 office-hours 工具集 | `office-hours/enter\|exit\|set-language\|ensure-routing\|append-profile\|append-learning\|search-learnings\|sync-artifact.ts` |
| 4.4.5.3 | 迁移 game-design 工具集 | `game-design/enter\|exit\|set-language\|ensure-routing\|append-profile\|append-learning\|search-learnings\|sync-artifact.ts` |
| 4.4.5.4 | L3 fixture | 进入/退出各模式的事件序列、partition 切换、artifact sync |

**对照测试设计(L3)**:
- 进入 plan mode → 模型切换/文件创建事件;退出 → `session_mode.exit` + handoff 事件。
- office-hours/game-design:进入事件、语言设置、profile/learning 追加后的记录。
- artifact sync:mock gbrain MCP/CLI,比对输出(不测真实网络)。

**门 G4-4-5**:session-mode 工具 L3 绿;进入/退出/切换事件序列与 TS 一致。

#### 4.4.6 — Goal & state tools `[plan]`(中风险,~2–3 天)

**目标**:迁移目标管理工具、TODO 列表、手动 checkpoint。

**依赖说明**:需要 4.4.0 + 4.3.2 的 `ToolStore`(由 `ToolManager` 提供)。`CreateGoal`/`GetGoal`/`SetGoalBudget`/`UpdateGoal` 依赖 `Agent.goals`(`SessionGoalStore` from `session/goal`),该子系统目前在 4.3 路线图中未显式拆分,是 4.4.6 的前置缺口,需先补齐(或并入 4.3.x 作为小补丁)。`CheckpointTool` 依赖 `Agent.checkpointCoordinator`。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.6.1 | (若未迁)补齐 `SessionGoalStore` | 对齐 `session/goal.ts`(或相关文件),作为 agent 子模块 |
| 4.4.6.2 | 迁移 goal CRUD 工具 | `goal/create-goal.ts`, `goal/get-goal.ts`, `goal/set-goal-budget.ts`, `goal/update-goal.ts` |
| 4.4.6.3 | 迁移 `TodoListTool` | 对齐 `state/todo-list.ts`:store 读写 |
| 4.4.6.4 | 迁移 `CheckpointTool` | 对齐 `state/checkpoint.ts` |
| 4.4.6.5 | L2 + L3 fixture | goal CRUD RPC 返回值;todo 更新事件;checkpoint 事件 |

**对照测试设计(L2 + L3)**:
- L2:goal 创建/读取/更新后的 JSON 快照。
- L3:todo 列表更新 → `tools.update_store` record;checkpoint → 持久化事件。

**门 G4-4-6**:goal/state 工具 L2+L3 绿;goal 快照、todo store 持久化、checkpoint 行为一致。

#### 4.4.7 — Quality & specialized tools `[normal]`(中风险,~3–4 天)

**目标**:迁移质量/专用工具。它们是"叶子",各自依赖前面的工具或子系统。

**依赖说明**:
- `HarvestOdyMarkersTool`:依赖 4.4.1 的 `GrepTool` + telemetry。
- `ReviewTestsTool`:依赖 4.3.7 的 `AdvancedSessionReviewer` + kaos + agent config。
- `RunE2ETestsTool`:依赖 `@odysseythink/e2e-testing` 包 + kaos + agent config。
- `SaveIdeaReportTool`:依赖 agent context(history 检测 skill 激活) + kaos。
- `ShowDesignMockupTool`:依赖 4.3.7 session-mode + `rpc.openExternal`。
- `RequestCodeReviewTool` 来自 `@odysseythink/code-review`,不在 `tools/builtin` 内,本阶段单独评估是否迁移或保留 TS 回调。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.7.1 | 迁移 `HarvestOdyMarkersTool` | 对齐 `code-quality/harvest-ody-markers.ts`:marker 解析、台账渲染 |
| 4.4.7.2 | 迁移 `ReviewTestsTool` | 对齐 `test-review/review-tests.ts`:git status、reviewer 调用、报告格式 |
| 4.4.7.3 | 迁移 `RunE2ETestsTool` | 对齐 `run-e2e-tests.ts`:影响分析、生成器调用、执行器 |
| 4.4.7.4 | 迁移 `SaveIdeaReportTool` | 对齐 `idea/save-idea-report.ts`:skill 激活检测、文件生成 |
| 4.4.7.5 | 迁移 `ShowDesignMockupTool` | 对齐 `visual/show-design-mockup.ts`:HTML 写出、openExternal |
| 4.4.7.6 | L1 + L3 fixture | marker 扫描、测试评审报告、E2E 执行、idea 报告、mockup 写出 |

**对照测试设计(L1 + L3)**:
- HarvestOdyMarkers:固定代码树,比对 markdown 输出。
- ReviewTests:mock reviewer,比对报告格式。
- RunE2ETests:mock generator/executor,比对 summary。
- SaveIdeaReport:构造含 idea skill 的 history,比对写出文件。
- ShowDesignMockup:mock `openExternal`,比对写出路径与调用参数。

**门 G4-4-7**:各叶子工具 L1+L3 绿;输出格式与事件 payload 与 TS 一致。

#### 4.4.8 — Tool registration integration & L2/L3 gate `[plan]`(中风险,~2–3 天)

**目标**:把 4.4.0–4.4.7 的全部工具在 Rust `ToolManager` 等价物中按条件注册,并通过 L2/L3 硬门。

**依赖说明**:依赖 4.3.9(Agent 组装完成)和全部 4.4 子模块。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.4.8.1 | 实现 Rust `ToolManager` 的 builtin 注册 | 对齐 `agent/tool/index.ts:initializeBuiltinTools`:条件注册(capability/flag/rpc availability) |
| 4.4.8.2 | 实现 `ToolStore` 更新持久化 | 对齐 `tools.update_store` record |
| 4.4.8.3 | 实现 active tools / mcpAccessPatterns | 对齐 `setActiveTools`、`loopTools` 排序 |
| 4.4.8.4 | L2 对照 | `getTools`/`registerTool`/`unregisterTool`/`setActiveTools` 返回值 |
| 4.4.8.5 | L3 对照 | file-edit、multi-turn-tool、web-search、background task、cron fire 等 scenario |
| 4.4.8.6 | 📊 基准 | 高频工具(read/write/bash/grep)Rust vs TS 单工具延迟 |

**门 G4-4**:
- 4.4.0–4.4.7 每子阶段门绿;
- `getTools`/`setActiveTools` L2 绿;
- file-edit / multi-turn-tool / web-search / background / cron L3 绿;
- 📊 高频工具单工具延迟改善或记录边界税;
- **任一工具无法对齐 → 该工具保留 TS 回调或单独跳过,不阻塞 G4-4 整体**。

---

### 4.5 — 收官与终态固化(中风险,拆为 8 子阶段)

**为什么必须拆**:原 4.5 把「删双份 / 固契约 / 处理 deferred gap / 回归门 / ADR」压成 5 行,AI 执行时容易遗漏以下风险:
- 未显式盘点 4.1–4.4 所有 deferred gap(ssh、readText error modes、kimi-files、RequestCodeReviewTool、compaction tokenizer 等)就贸然删 TS;
- 删 TS 后失去对照基线,导致后续回归无据可依;
- API 冻结与 TUI 契约固化被当作一句口号,没有独立评审门;
- 成功度量表未在删双份前实测就填写,成为纸面数据。

**拆分原则与依赖关系**:
- **4.5.0 是硬前置**:必须先完整盘点 4.0–4.4 产生的 `parity/known-gaps.md`,并对每个 gap 做终态决策(迁 Rust / 永久 TS 回调 / Phase 4 外 defer)。**4.5.0 用 design 模式**,因为终态决策 expensive-to-reverse。
- **4.5.1 / 4.5.2 / 4.5.3 彼此无依赖**,在 4.5.0 决策后可并行推进:分别处理 kaos、kosong/provider、agent/tool 三类 deferred gap。已经决定「永久 TS 回调」的 gap 不进入这三子阶段。
- **4.5.4 删 TS 双份**可与 4.5.1–4.5.3 并行,但只能删除「已稳定 + 无未决 gap」的单元;被 4.5.0 判定为永久 TS 回调的单元保留 TS 实现并登记。
- **4.5.5 冻结 CoreAPI/SDKAPI & TUI 契约**必须在所有 API 变更(含 4.5.1–4.5.3 迁移可能引入的新 RPC)完成后进行。
- **4.5.6 L4 回归门 & golden 归档**必须在 4.5.4 删双份前完成 TS golden 快照固化(纪律 P4-R8),并在删双份后继续回归。
- **4.5.7 ADR & 成功度量**是最后一步,依赖所有实测数据。

**建议执行模式**（见 §3 表格列）:
- **4.5.0 → design 模式**：终态 gap 分类决策是 expensive-to-reverse 的产品/架构决策,需要显式方案、假设与批准。
- **4.5.1 / 4.5.3 / 4.5.5 → plan 模式**：kaos 高级 gap(SSH 等)、agent/tool 外部包/分词器集成、API 冻结都是多文件/接口/回滚策略决策。
- **4.5.2 / 4.5.4 / 4.5.6 / 4.5.7 → normal 模式**：provider 协议实现、TS 双份删除、L4 回归归档、ADR 填写在决策/接口锁定后可直接执行。

#### 4.5.0 — Final gap inventory & triage `[design]`(中风险,~2–3 天)

**目标**:在删 TS 双份前,先把 4.0–4.4 留下的所有 deferred gap 完整盘点,并做终态决策。这是 Phase 4 最后一个会影响产品终态的决策点,因此进入 design 模式。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.0.1 | 汇总 `parity/known-gaps.md` | 收集 4.1(ssh/readText)、4.2(未迁 provider/kimi-files/video uploader)、4.3(compaction tokenizer/concurrency 归一化)、4.4(RequestCodeReviewTool/其他外部包工具)所有 gap |
| 4.5.0.2 | 建立 triage 维度表 | 对每个 gap 评估:用户影响 / 安全影响 / 维护成本 / Rust 迁移成本 / 是否阻塞删双份 |
| 4.5.0.3 | 终态决策 | 每个 gap 三选一:**A)** 在 4.5.1–4.5.3 内迁 Rust;**B)** 正式记为「永久 TS 回调」,保留 TS 实现并声明维护责任;**C)** 移出 Phase 4,记为后续阶段工作项 |
| 4.5.0.4 | 更新 `parity/known-gaps.md` | 每个 gap 标注终态类别、Owner、验收标准 |
| 4.5.0.5 | 同步 ADR-0000(或对应 ADR) | 把「永久 TS 回调」清单写入架构决策记录,作为后续维护约束 |

**对照测试设计**:本阶段不产代码,产出是评审过的 `known-gaps.md` 与 ADR 更新。

**门 G4-final-0**:`known-gaps.md` 中每个 4.x 产生的 gap 都有终态标签;A/B/C 决策有明确 Owner 和验收标准;design 模式批准通过。

---

#### 4.5.1 — Migrate deferred kaos gaps `[plan]`(高风险,~4–6 天)

**目标**:处理 4.1 阶段 deferred 的 kaos 高级/边缘能力。**仅迁移 4.5.0 决策为 A 的项**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.1.1 | SSH 客户端抽象(若迁) | 对齐 `packages/kaos/src/ssh.ts`:连接池、jump host、agent forwarding、stderr 合并;评估 `russh`/`openssh` crate |
| 4.5.1.2 | `readText` 错误模式兼容(若找到方案) | 对齐 Node Buffer `strict/replace/ignore` 语义;若仍不可调和,回退为永久 TS 回调 |
| 4.5.1.3 | 其他 kaos 边缘能力(Windows Git Bash 高级定位、特殊 fs 行为等) | 按 gap 清单逐项处理 |
| 4.5.1.4 | L1 对照 | ssh fixture / readText error fixture TS vs Rust |

**对照测试设计(L1)**:
- SSH:mock SSH server 或录制 fixture,比对 `exec` 返回值、环境变量、工作目录、kill 行为。
- readText:非法 UTF-8/UTF-16LE 输入,比对 strict/replace/ignore 输出。

**门 G4-final-1**:迁移项 L1 对照绿;未迁移项在 `known-gaps.md` 中明确为永久 TS 回调。

**No-Go 信号**:SSH 在 Rust 中无法达到与 TS 等价的安全/功能边界 → 正式记为永久 TS 回调,不阻塞 4.5.4 删双份。

---

#### 4.5.2 — Migrate deferred kosong/provider gaps `[normal]`(中风险,~3–5 天)

**目标**:处理 4.2 阶段 deferred 的 provider 特性。**仅迁移 4.5.0 决策为 A 的项**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.2.1 | KimiFiles / video uploader(若迁) | 对齐 `packages/kosong/src/providers/kimi-files.ts`:文件上传、`ms://<file-id>` URL、上传后 tool 参数替换 |
| 4.5.2.2 | 其他未迁 provider(若 4.2.x 有 No-Go) | 按 4.5.0 决策单独立项 |
| 4.5.2.3 | L1 SSE / L2 对照 | 上传 fixture、provider 配置 fixture |

**对照测试设计(L1 SSE)**:
- KimiFiles:mock 上传端点,比对上传请求体与返回的 `ms://` URL 替换结果。
- 其他 provider:复用 4.2.x 的 SSE fixture 模式。

**门 G4-final-2**:迁移项 L1/L2 对照绿;未迁移 provider 正式记为永久 TS 回调。

---

#### 4.5.3 — Migrate deferred agent/tool gaps `[plan]`(中风险,~3–5 天)

**目标**:处理 4.3/4.4 阶段 deferred 的 agent 特性与外部包工具。**仅迁移 4.5.0 决策为 A 的项**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.3.1 | Compaction 真分词器集成(若 Phase 1-A 就绪) | 把 Wasm/tiktoken-rs 分词器接入 4.3.6,校准 token 计数 |
| 4.5.3.2 | `SessionGoalStore` 缺口(若 4.4.6 未提前补齐) | 作为 agent 子模块落地,供 goal tools 使用 |
| 4.5.3.3 | `RequestCodeReviewTool` 等外部包工具(若迁) | 评估 `@odysseythink/code-review` 依赖,决定迁 Rust 或保留 TS 回调 |
| 4.5.3.4 | 并发事件顺序归一化(若 4.3.8/4.4.3 有残留) | 按 task-id 分组排序等策略固化 |
| 4.5.3.5 | L1 + L3 对照 | 分词器 fixture、goal tool scenario、外部工具 mock |

**对照测试设计(L1 + L3)**:
- 分词器:固定文本 → TS/Rust token 数逐值一致。
- Goal tools:复用 4.4.6 L2/L3 scenario。
- RequestCodeReview:mock reviewer,比对报告格式。

**门 G4-final-3**:迁移项 L1/L3 对照绿;未迁移项正式记为永久 TS 回调或 Phase 4 外 defer。

---

#### 4.5.4 — Delete TS dual implementations `[normal]`(中风险,~3–5 天)

**目标**:对每个已稳定且无未决 gap 的迁移单元,删除 TS 旧实现(母路线纪律 8:稳定即删旧实现)。**只能删 4.5.0 已决策终态且 4.5.1–4.5.3 已完成的单元**。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.4.1 | 删除 kaos TS 实现 | `packages/kaos/src/` 中已 Rust 化的模块(保留 ssh 等永久 TS 回调项) |
| 4.5.4.2 | 删除 kosong TS 实现 | `packages/kosong/src/providers/` 中已 Rust 化的 provider(保留永久 TS 回调 provider) |
| 4.5.4.3 | 删除 agent-core 中已 Rust 化的子模块 | 按 4.3.x 子阶段逐单元删除,保留 SessionGoalStore 等可能仍在 TS 的缺口 |
| 4.5.4.4 | 删除 tools/builtin 中已 Rust 化的工具 | 按 4.4.x 子阶段逐单元删除,保留永久 TS 回调工具 |
| 4.5.4.5 | 全量测试 + typecheck | `pnpm -r test` / `pnpm -r typecheck` / `cargo test` |

**门 G4-final-4**:每个删除单元在删除前通过对应 L1/L2/L3 门;删除后全测试 + typecheck 绿;`known-gaps.md` 中永久 TS 回调项的 TS 实现保留。

---

#### 4.5.5 — Freeze CoreAPI/SDKAPI & TUI transport contract `[plan]`(高风险,~2–3 天)

> **模式说明(Rubric §3.0 规则 3)**:本子阶段标 plan,但产物是**对外接口的终态冻结契约**。进入时第一步先做一次 design-lite 终态决策——枚举待冻结的方法/事件/帧格式清单及其向后兼容边界,记录取舍后再进入下方固化条目。

**目标**:当所有 Rust 实现稳定、所有 API 变更落地后,把 `CoreAPI`/`SDKAPI` 和 TUI Socket transport 客户端契约固化为冻结合同。后续非兼容性变更需走正式 RFC。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.5.1 | 固化 `CoreAPI` 方法列表 | 文档化每个方法的输入/输出/错误码,附 JSON Schema |
| 4.5.5.2 | 固化 `SDKAPI` 事件类型 | 文档化 `emitEvent` 事件名、payload schema、有序性保证 |
| 4.5.5.3 | 固化 TUI transport 客户端契约 | Socket 帧格式、重连策略、心跳、消息边界 |
| 4.5.5.4 | 建立兼容性测试 | 向后兼容检查:新增方法/事件不得破坏旧 TUI 客户端 |
| 4.5.5.5 | 更新 API 合同文档 | 写入 `docs/architecture/` 或 ADR |

**门 G4-final-5**:`CoreAPI`/`SDKAPI`/TUI transport 合同文档评审通过;兼容性测试基线建立;后续变更需 RFC 的流程写入 `CONTRIBUTING.md` 或等效文档。

---

#### 4.5.6 — L4 regression gate & golden archive `[normal]`(中风险,~2–3 天)

**目标**:在删 TS 双份前把 TS golden 快照固化存档,防止删后无对照基线;删双份后把 L4 全量场景设为发布前回归门。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.6.1 | 固化 TS golden 快照 | 对每个 L4 scenario 保存 `expected.json`(响应 + 事件流 + records),存档于 `parity/golden/` |
| 4.5.6.2 | 建立 L4 CI job | 在 `.github/workflows/rust-host.yml` 新增 `parity-l4` job,每晚/发布前运行 |
| 4.5.6.3 | 断基线告警 | Rust 结果与 `expected.json` 不一致时红,并上传 diff artifact |
| 4.5.6.4 | 回归场景覆盖 | file-edit、multi-turn-tool、session-mode handoff、background task、cron fire、web search、goal management |

**门 G4-final-6**:TS golden 快照已归档;L4 CI job 运行绿至少一次;断基线告警机制可用。

---

#### 4.5.7 — ADR update & success metrics `[normal]`(低风险,~1–2 天)

**目标**:把 Phase 4 的实际终态写入 ADR,并把母路线 §成功度量「Phase 4 后」列填实测值。

| 编号 | 条目 | 落地 |
|---|---|---|
| 4.5.7.1 | 更新 Phase 4 ADR | 记录最终架构、永久 TS 回调清单、关键决策与回滚路径 |
| 4.5.7.2 | 填写成功度量表 | 常驻内存、冷启动、崩溃域、客户端多样性等实测值 |
| 4.5.7.3 | 更新母路线图 Phase 4 节 | 把「Phase 4 后」列替换为实测值和遗留 TS 回调说明 |
| 4.5.7.4 | 发布说明 / CHANGELOG | 记录用户可见变更(性能、新后端、已知限制) |

**门 G4-final-7**:ADR 与母路线图更新完成;成功度量表所有项有实测值或「N/A」说明;发布说明通过产品/技术评审。

**门 G4-final**:
- 4.5.0–4.5.7 每子阶段门绿;
- L4 端到端场景重放对照绿(或对剩余 TS 回调项有显式登记);
- 删除 TS 双份后全测试 + typecheck 绿;
- TUI 经纯 Rust 后端跑通真实会话(非 mock)冒烟;
- `CoreAPI`/`SDKAPI`/TUI transport 已冻结;
- 成功度量「Phase 4 后」列全部兑现(常驻内存/冷启动/崩溃域/客户端多样性);
- `known-gaps.md` 中所有 gap 都有终态标签,永久 TS 回调项不阻塞发布。

---

## 5. 执行顺序与依赖

```
4.0 对照框架(硬前置)
  │
  ▼
4.1 kaos
  │
  ├──► 4.1.0 路径/环境 + 共享 helper(硬前置)
  │       │
  │       ├──► 4.1.1 目录操作 ────────┐
  │       ├──► 4.1.2 文件读写 ────────┤
  │       └──► 4.1.3 进程执行 ────────┤
  │                                   │
  └───────────────────────────────────┘
              │
              ▼
        4.1.4 CoreHost 集成 / L2 门
              │
              ▼
4.2 kosong
  │
  ├──► 4.2.0 共享数据模型 + generate 循环
  │       │
  │       └──► 4.2.1 通用工具层 ──────┐
  │                                   │
  │       ┌──► 4.2.2 OpenAI Chat Completions + OpenAI Legacy
  │       │       │                   │
  │       │       ├──► 4.2.3 OpenAI Responses
  │       │       └──► 4.2.5 Kimi / DeepSeek / GLM
  │       │                           │
  │       ├──► 4.2.4 Anthropic ──────┤
  │       └──► 4.2.6 Google GenAI ───┤
  │                                   │
  └───────────────────────────────────┘
              │
              ▼
        4.2.7 CoreHost factory / L2-L3 门
              │
              ▼
4.3 agent
  │
  ├──► 4.3.0 Records & persistence foundation(最硬前置)
  │       │
  │       ├──► 4.3.1 Context & projection ───────────────┐
  │       │       ▲(injection 生命周期接口)               │
  │       ├──► 4.3.2 Config / usage / tool & skill ──────┤
  │       │       └──► 4.3.3 Permission system           │
  │       │                                              │
  │       ├──► 4.3.4 Stateless loop engine ──────────────┤
  │       │       (仅依赖 kosong 类型 + hooks)            │
  │       │                                              │
  │       └──────────────┬───────────────────────────────┘
  │                      │
  │                      ▼
  │              4.3.5 Turn flow & LLM adapter
  │                      │
  │        ┌─────────────┼─────────────┐
  │        ▼             ▼             ▼
  │  4.3.6 Compaction  4.3.7 Session   4.3.8 Background
  │     strategies      modes &        tasks & cron
  │                     injection          │
  │                        │               │
  │                        └───────┬───────┘
  │                                ▼
  │                       4.3.9 Agent orchestrator
  │                            & CoreHost / L2-L3-L4 门
  │                                │
  │                                ▼
  │                       4.4 tools
  │                         │
  │            ┌────────────┼────────────┐
  │            ▼            ▼            ▼
  │      4.4.0 Infra   4.4.1 File/   4.4.2 Web
  │      (硬前置)        shell I/O    (host fetch/search)
  │            │            │            │
  │            │            ▼            ▼
  │            │      4.4.3 Background/ 4.4.6 Goal/State
  │            │            Cron        (after goals)
  │            │           (after       │
  │            │           4.3.8)       ▼
  │            │      4.4.4 Collaboration
  │            │      (after 4.3.5/7/8) │
  │            │                        ▼
  │            │               4.4.5 Session-mode
  │            │               (after 4.3.7)
  │            │
  │            └───────┬────────────────┘
  │                    ▼
  │            4.4.7 Specialized tools
  │                    │
  │                    ▼
  │            4.4.8 Integration / L2-L3 门
  │                    │
  │                    ▼
  │            4.4.8 Integration / L2-L3 门
  │                    │
  │                    ▼
  │            4.5.0 Gap inventory & triage(硬前置,design)
  │                    │
  │        ┌───────────┼───────────┬──────────────┐
  │        ▼           ▼           ▼              ▼
  │  4.5.1 kaos   4.5.2 kosong  4.5.3 agent/  4.5.4 Delete
  │     gaps       /provider     tool gaps     TS duals
  │  (after 4.1)  (after 4.2)   (after 4.3/4)  (per-unit)
  │        │           │           │              │
  │        └───────────┴───────────┴──────────────┘
  │                            │
  │                            ▼
  │              4.5.5 Freeze CoreAPI/SDKAPI/TUI
  │                            │
  │                            ▼
  │              4.5.6 L4 regression gate & golden archive
  │                            │
  │                            ▼
  │              4.5.7 ADR update & success metrics

  (4.1.1/4.1.2/4.1.3 彼此无依赖,仅在 4.1.0 之后并行)
  (4.2.2 是 4.2.3 与 4.2.5 的共享层前置;4.2.4 与 4.2.6 彼此独立)
  (4.3.1/4.3.2/4.3.3/4.3.4 在 4.3.0 之后可并行;4.3.5 是它们的集成硬门)
  (4.3.6/4.3.7/4.3.8 都依赖 4.3.5;4.3.6 另依赖 Phase 1-A 真分词器一致)
  (4.4.0 是 4.4 硬前置;4.4.1/4.4.2 在 4.4.0 后、4.4.3 在 4.3.8 后可并行;
   4.4.4/4.4.5/4.4.6 分别依赖 4.3.5/4.3.7/4.3.8 等 agent 子模块;4.4.7 是叶子;
   4.4.8 是 G4-4 集成硬门)
  (4.5.0 是 4.5 硬前置,design 模式终态决策;4.5.1/4.5.2/4.5.3/4.5.4 在 4.5.0 后
   可并行;4.5.5/4.5.6/4.5.7 顺序收尾)
```

- **严格自底向上**:4.1→4.2→4.3 不可乱序(对照测试需隔离故障源)。
- **4.1 内部顺序**:
  - **4.1.0 是硬前置**:定义 `Kaos` trait/struct、实例级 `cwd`、错误类型以及共享 helper(`decodeTextWithErrors` / `globPatternToRegex` / `BufferedReadable`)。4.1.1/4.1.2/4.1.3 都需要这些基座。
  - **4.1.1 / 4.1.2 / 4.1.3 彼此无依赖**,在 4.1.0 完成后可并行开发;建议仍按「目录 → 文件 → 进程」顺序合入,因为 agent/tools 最早需要的是目录/文件能力,且进程子阶段的安全评审最耗时。
  - **4.1.4 是 G4-1 真正硬门**:它依赖 4.1.1/4.1.2/4.1.3 全部完成,负责把 `kaos-rs` 接入 `CoreHost` 并通过 `env.*` RPC 的 L2 对照,才允许后续 4.2 依赖 kaos。
- **4.2 内部顺序**:
  - **4.2.0 是硬前置**:定义 `Message`/`ContentPart`/`ToolCall`/`StreamedMessagePart`/`ThinkingEffort`/`FinishReason`/`TokenUsage`/`GenerateResult`/`GenerateOptions`/`ProviderType`/`ChatProvider` trait 与错误类型。**4.2.1 及所有 provider 都依赖它**。
  - **4.2.1 依赖 4.2.0**:实现 `tool-call-id`/`request-auth`/`capability-registry`/`catalog`;其中 `catalog` 需要 4.2.0 中定义的 `ProviderType`。
  - **4.2.2 依赖 4.2.0+4.2.1**:建立 OpenAI Chat Completions 共享解析层(`openai-common` + `chat-completions-stream`)并完成 OpenAI Legacy provider。
  - **4.2.3 依赖 4.2.2**:OpenAI Responses provider 复用 `openai-common` 的错误转换与 `thinkingEffort↔reasoning_effort` 映射。
  - **4.2.5 依赖 4.2.2**:Kimi/GLM 复用 `openai-common` + `chat-completions-stream`;DeepSeek 直接封装 `OpenAILegacyChatProvider`。Kimi 子任务包含 `kimi-schema.ts` 与 `KimiFiles` 上传。
  - **4.2.4 Anthropic 与 4.2.6 Google GenAI 彼此独立**,也与 4.2.2/4.2.3/4.2.5 无直接依赖(只依赖 4.2.0+4.2.1)。
  - **4.2.7 是 G4-2 真正硬门**:provider factory 导入全部 provider(4.2.2–4.2.6),CoreHost 集成后通过 L2/L3 对照。
- **4.3 内部顺序**:
  - **4.3.0 是最硬前置**:records schema(`AgentRecordEvents`) 不钉死,context/config/permission/turn 都无法写 WAL 和恢复。虽然 record 类型定义会引用下游模块的类型,但这些 import 都是纯类型,运行时 records 模块不依赖下游业务逻辑,因此 4.3.0 可先独立落地。
  - **4.3.1 / 4.3.2 / 4.3.3 / 4.3.4 在 4.3.0 之后可并行**:
    - 4.3.1 Context 需要 injection 提供 `onContextClear/onContextCompacted/onContextMessageRemoved` 生命周期接口,但不需要注入器完整实现。
    - 4.3.2 Config/Usage/Tool 可独立;Skill 的注册/渲染可在 4.3.2 完成,但 `recordActivation(prompt)` 路径需等 4.3.5。
    - 4.3.3 Permission 依赖 4.3.2 的 ToolManager(策略需要 tool 元数据)。
    - 4.3.4 Loop 是无状态引擎,只依赖 kosong 类型与 `ExecutableTool`/`LoopHooks` 抽象,可与上层完全并行。
  - **4.3.5 是 4.3.1/4.3.2/4.3.3/4.3.4 的集成硬门**:TurnFlow 直接调用 context/usage/config/tools/permission/injection/session-mode/compaction,并通过 `runTurn` 驱动 loop engine。**4.3.5 不绿,禁止进入 4.3.6/4.3.7/4.3.8。**
  - **4.3.6 / 4.3.7 / 4.3.8 都依赖 4.3.5**,因为它们在 turn 生命周期内被调用(compaction hooks、injection per-step、session-mode enter/exit、background/cron 通过 `turn.steer` 注入);4.3.6 还额外依赖 Phase 1-A 真分词器。
  - **4.3.9 是 G4-3 真正硬门**:只有全部子模块组装进 `Agent` 并接入 `CoreHost` 后,L2/L3/L4 才绿。
- **4.4 内部顺序**(经源码核对后细化):
  - **4.4.0 是 4.4 硬前置**:定义 `BuiltinTool` trait、`ToolExecution`、`ToolAccesses`、路径策略、规则匹配、schema 转换、结果构建器、文件类型嗅探、rg 定位器、参数校验器、ToolStore。4.4.1–4.4.7 都需要这些公共基座。
  - **4.4.1 / 4.4.2 在 4.4.0 之后可并行**:文件/shell 工具与 Web 工具都只依赖 `kaos`(4.1)和主机注入服务,不依赖 agent 内存状态。`WriteTool`/`EditTool` 的 plan/design 重定向、`BashTool` 的后台执行可 deferred 到对应 agent 子模块就绪后补齐。
  - **4.4.3 后台/cron 工具依赖 4.3.8**:它们是 `BackgroundManager`/`CronManager` 的薄壳,可与 4.4.1/4.4.2 并行开发,但 `BashTool` 的 `run_in_background=true` 必须等 4.4.3 就绪才启用。
  - **4.4.4 协作工具依赖最重**:需要 4.3.5(TurnFlow) + 4.3.7(SessionMode/Skills) + 4.3.8(BackgroundManager) + `SessionSubagentHost`(AgentTool)。
  - **4.4.5 会话模式工具依赖 4.3.7**:enter/exit 工具直接调用 `SessionMode`;sync artifact 工具还需 `rpc`/`mcp`/gbrain CLI。
  - **4.4.6 目标/状态工具依赖 `ToolStore`(4.3.2) + `SessionGoalStore` + checkpoint coordinator**:其中 `SessionGoalStore` 在 4.3 路线图中未显式拆分,是 4.4.6 的前置缺口,需先补齐。
  - **4.4.7 质量/专用工具是叶子**:依赖前面已迁移的工具或子系统(如 `HarvestOdyMarkers` 依赖 4.4.1 的 `GrepTool`,`ReviewTests` 依赖 4.3.7 的 reviewer)。
  - **4.4.8 是 G4-4 真正硬门**:全部工具在 Rust `ToolManager` 等价物中按条件注册,并通过 L2/L3 对照。**任一工具无法对齐只搁置该工具,不阻塞 G4-4 整体。**
- **4.5 内部顺序**(收官阶段):
  - **4.5.0 是 4.5 的硬前置,进入 design 模式**:必须完整盘点 4.0–4.4 的 `parity/known-gaps.md`,对每个 gap 做终态决策(迁 Rust / 永久 TS 回调 / Phase 4 外 defer)。**未做 triage 前禁止删 TS 双份。**
  - **4.5.1 / 4.5.2 / 4.5.3 / 4.5.4 在 4.5.0 后可并行**:分别处理 kaos、gkosong/provider、agent/tool 的 deferred gap,以及按单元删除 TS 双份。已经决策为「永久 TS 回调」的单元不进入 4.5.1–4.5.3,其 TS 实现保留到 4.5.4 之后。
  - **4.5.5 冻结 CoreAPI/SDKAPI & TUI transport 契约**必须在所有 API 变更(含 4.5.1–4.5.3 可能引入的新 RPC)完成后进行,是发布前的合同固化。
  - **4.5.6 L4 回归门 & golden 归档**必须在 4.5.4 大规模删双份前完成 TS golden 快照固化(纪律 P4-R8);删双份后 Rust 对照静态 golden 继续回归。
  - **4.5.7 ADR & 成功度量**是最后一步,依赖所有实测数据。
- **任一子阶段 No-Go = 合法中间终态**:Phase 4 可停在「kaos 4.1.0–4.1.3 已实现但 4.1.4 未集成」「kaos Rust 化、其余 TS」「kaos+kosong 4.2.0–4.2.6 已实现但 4.2.7 未集成」「kaos+kosong Rust 化、agent TS」「kaos+kosong+agent 4.3.0–4.3.8 已实现但 4.3.9 未集成」等任意分层,每层都是可发布的双后端可切换状态。

---

## 6. 工程纪律(继承母路线 + Phase 4 增量)

继承母路线 §贯穿全程 8 条(契约冻结 / 双轨回退 / golden 优先 / 先 profile / 基准即交付物 / 单单元回滚 / 依赖图守护 / 稳定即删)。**Phase 4 增量**:

9. **对照测试先行**:无对照测试不得合入 Rust 实现;对照测试须先 TS-vs-TS 自比对绿。
10. **归一化清单即合同**:每条归一化规则单独评审,清单膨胀视为语义漂移告警。
11. **混合后端可运行**:迁移期任一时刻,`ODY_BACKEND` 二选一(或模块级开关)都能跑通对照,生产永远可整体降级回 TS。
12. **records 双向兼容是硬门**:agent 迁移期 TS/Rust 落盘必须互读,否则禁止灰度。

---

## 7. 成功度量(Phase 4 专属)

| 维度 | 入口(Phase 3 后) | 目标(Phase 4 后) |
|---|---|---|
| 对照覆盖 | session 生命周期基本面 | L1+L2+L3 全模块绿,L4 场景回归门 |
| kaos | TS | Rust(ssh 除外或含)+ L1 逐字节一致 |
| kosong | TS 8 家 | Rust ≥6 家 + SSE 逐值一致(kimi-files 可 defer) |
| agent | TS | Rust 4.3.0–4.3.9 全子阶段绿 + records 双向互读绿 + L4 场景回归 |
| tools | TS 全量 | Rust 文件/shell 七件套 + 逐工具对照绿 |
| 常驻内存/冷启动 | Node 基线 | Rust 实测改善(兑现 G3 ADR 预期) |
| 正确性 | — | 对照测试 100%(硬门,逐字节/逐值) |
| 可降级 | worker 隔离 | 任意子阶段可整体回退 TS |

---

## 8. 风险与应对(Phase 4 专属)

| 编号 | 风险 | 应对 |
|---|---|---|
| **P4-R1** | mock provider 两侧产出不一致 → L3 全线失真 | mock provider 本身先做 L1 对照(§2.4),作为 4.0 门的一部分 |
| **P4-R2** | 归一化过度,掩盖真实语义漂移 | 归一化清单单独评审(纪律 10);清单增长触发复审 |
| **P4-R3** | readText 错误模式 / UTF-8 语义 Rust 难复刻 | 4.1 隔离测试;不可调和则该方法保留 TS 回调,登记 gap |
| **P4-R4** | SSE 流解析 provider 间差异爆炸 | 逐 provider VCR fixture;单 provider 卡死不阻塞整层 |
| **P4-R5** | agent 并发事件顺序非确定,对照不稳定 | background task 设专用归一化(按 task-id 分组排序);必要时 defer 并发子单元 |
| **P4-R6** | records 落盘格式 TS/Rust 不兼容 → 无法灰度 | 双向互读列为硬门(纪律 12);blob 格式先定 schema |
| **P4-R7** | compaction 依赖分词器,两侧分词不一致 | compaction 迁移晚于 Phase 1-A 真分词器;Rust 直接复用同一 Wasm/tiktoken-rs |
| **P4-R8** | 删 TS 双份后失去对照基线 | 删除前把对应 L4 场景的 TS golden 快照固化存档(`expected.json`),Rust 此后对静态 golden 回归 |
| **P4-R9** | 全栈 Rust 工作量大,中途人力不足 | 每子阶段都是合法终态;按价值排序,kaos+kosong 拿下即已是巨大收益,agent/tools 可缓 |

---

## 9. 一句话总览

> **Phase 4 不是「把 TS 翻译成 Rust」,而是「在协议级可互换的两个后端之间,用对照测试逐模块钉死等价,然后自底向上把引擎从 TS 换成 Rust」。**
> 先建 TS↔Rust 对照框架(4.0,TS-vs-TS 自证可信)→ 自底向上迁 kaos(L1 逐字节)→ kosong(SSE 重放逐值)→ agent(L3 事件流 + records 双向互读)→ tools(逐工具一门)→ 收官删双份。
> **每一步都由对照测试供数据门,任一子阶段 No-Go 都是一个可发布、可降级的合法中间终态**——不为纯度强迁,不为 Rust 而 Rust,正确性逐字节优先于一切性能收益。
