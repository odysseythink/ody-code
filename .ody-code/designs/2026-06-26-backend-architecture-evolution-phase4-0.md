# Phase 4.0 详细设计 — TS↔Rust 对照测试框架 + 双后端运行时开关

> **Document Type**: Phase 4.0 Detailed Design
> **Parent**: `.ody-code/roadmaps/backend-architecture-evolution-phase4-rust-host-migration-roadmap.md` §4.0
> **Predecessor**: `.ody-code/roadmaps/backend-architecture-evolution-phase3-fixup-roadmap.md`
> **Status**: DRAFT (awaiting approval)
> **Audit Level**: Deep

---

## Scope In/Out

### In Scope [C:USER]

1. 在 `packages/integration-tests/src/parity/` 下建立模块化 parity 框架：
   - `driver.ts` — 场景执行器
   - `backends.ts` — TS backend 与 Rust backend 工厂
   - `normalize.ts` — 非确定性字段归一化
   - `assert-parity.ts` — 逐字段 diff 断言
   - `scenarios/` — 三个 TypeScript 场景文件
   - `known-gaps.md` — 已知差异登记
2. 移植 3 个现有场景：session 生命周期 / setModel / mock prompt。
3. 实现 TS-vs-TS 自比对，证明 harness + 归一化可信。
4. 实现 TS-vs-Rust 比对，未实现方法按 `known-gaps.md` 显式 `skip(reason)`。
5. 引入 `ODY_BACKEND=ts|rust` 运行时开关 [C:USER]：
   - parity 测试读取它选择被测后端；
   - CLI/TUI 读取它作为默认后端；
   - `--host=ts|rust` 显式覆盖 `ODY_BACKEND`（原 `--host=inproc` 重命名为 `--host=ts`）。
6. 在 `.github/workflows/rust-host.yml` 新增 `parity` job，每次 PR 运行 [C:USER]。

### Out of Scope [C:USER]

| 条目 | 原因 |
|---|---|
| 模块级 `ODY_RUST_MODULES=kaos,kosong,...` 混合后端 | 4.0 仅全局二选一；模块级开关 4.1+ 按需引入 |
| L4 端到端场景重放 | 4.5 收官阶段 |
| provider SSE fixture 录制与 L1 重放 | 4.2 子阶段 |
| kaos / kosong / agent / tools 的真实迁移 | 4.1–4.4 子阶段 |
| records 双向互读 | 4.3 子阶段 |

---

## Prior Art

本设计不直接移植上游系统。参考思路：
- ** differential testing / parity testing ** 在编译器、数据库领域常见，核心模式是「同一输入 → 两个实现 → 归一化 → 对比」。
- 本设计的关键差异：两个后端已通过统一 RPC 协议（`CoreAPI`/`SDKAPI`）和同一 `SDKRpcClient` 驱动，因此 harness 对后端语言无感知。

---

## Architecture

### 数据流

```
Scenario (TS file)
    │
    ▼
ParityDriver.runScenario(backend, scenario)
    │
    ├──► TS Backend ──► SDKRpcClient ──► WorkerCoreAPI(+MockChatProvider)
    │
    └──► Rust Backend ──► SDKRpcClient.connect('stdio', binaryPath, ['--mock-provider'])
    │
    ▼
{ responses[], events[], records[] }
    │
    ▼
Normalizer.normalize(result)          // 抹平 ts/uuid/path/pid/...
    │
    ▼
AssertParity.deepEqual(ts, rust)      // 结构化 diff
```

### 组件职责

| 组件 | 文件 | 职责 |
|---|---|---|
| `ParityDriver` | `driver.ts` | 给定 backend 和 scenario，执行步骤并收集响应、事件、落盘记录 |
| `BackendFactory` | `backends.ts` | `makeTsBackend()` / `makeRustBackend()`，管理生命周期与临时目录 |
| `Normalizer` | `normalize.ts` | 按 §2.3 清单归一化非确定性字段 |
| `AssertParity` | `assert-parity.ts` | 递归深比较，失败时输出 scenario 名 + 结构化 diff |
| `Scenario` | `scenarios/*.ts` | 确定性脚本：调用 CoreAPI / 监听 SDKAPI 事件 |
| `KnownGaps` | `known-gaps.md` | 未实现方法的显式 skip 登记表 |

---

## Data Models

### Core Types

```ts
// 后端种类 [C:USER]
type BackendKind = 'ts' | 'rust';

// 统一后端句柄 [C:INFERRED]
interface ParityBackend {
  readonly kind: BackendKind;
  readonly client: SDKRpcClient;
  readonly homeDir: string;
  close(): Promise<void>;
}

// 场景定义 [C:USER]
interface Scenario {
  readonly name: string;
  // 每个 step 可异步访问 backend.client 并返回任意可序列化的快照
  readonly run: (backend: ParityBackend) => Promise<ScenarioSnapshot>;
}

// 场景原始输出 [C:INFERRED]
interface ScenarioSnapshot {
  readonly responses: readonly unknown[];
  readonly events: readonly AgentEvent[];
  readonly records?: readonly unknown[];
  readonly fsTree?: unknown;          // 4.0 仅 session/setModel 用不到；接口预留
}

// 归一化后输出 [C:INFERRED]
interface NormalizedSnapshot {
  readonly responses: readonly unknown[];
  readonly events: readonly NormalizedAgentEvent[];
  readonly records?: readonly unknown[];
  readonly fsTree?: unknown;
}

// 差异报告 [C:INFERRED]
interface ParityDiff {
  readonly scenarioName: string;
  readonly ts: NormalizedSnapshot;
  readonly rust: NormalizedSnapshot;
  readonly diffs: readonly FieldDiff[];
}

interface FieldDiff {
  readonly path: string;
  readonly tsValue: unknown;
  readonly rustValue: unknown;
}

// known gap 登记表项 [C:INFERRED]
interface KnownGap {
  readonly scenario: string;   // 支持通配符 '*'
  readonly reason: string;
  readonly layer: 'L2' | 'L3' | 'L4';
}
```

### Backend Configuration

```ts
// TS backend配置 [C:INFERRED]
interface TsBackendConfig {
  readonly homeDir: string;
  readonly mockLlm?: MockChatProvider;   // 仅 mock prompt 场景使用
}

// Rust后端配置 [C:USER]
interface RustBackendConfig {
  readonly homeDir: string;
  readonly binaryPath: string;
  readonly transport: 'stdio' | { socketPath: string } | { host: string; port: number };
  readonly extraArgs?: readonly string[];   // e.g. ['--mock-provider']
}
```

### Normalization Configuration

```ts
interface NormalizerOptions {
  readonly homeDir: string;     // 替换为 <HOME>
  readonly tmpDir: string;      // 替换为 <TMP>
  readonly fixedIds?: ReadonlyMap<string, string>; // 固定 seed -> 占位
}
```

---

## Algorithms

### ParityDriver.runScenario [C:INFERRED]

```
function runScenario(backend: ParityBackend, scenario: Scenario): Promise<ScenarioSnapshot>
  events := empty list
  unsubscribe := backend.client.onEvent((event) => events.push(event))
  try
    responses := empty list
    result := await scenario.run(backend)
    if result.responses is defined
      responses := result.responses
    else
      // scenario 直接返回 undefined 表示不收集同步响应，仅收集事件
      responses := []
    return { responses, events, records: result.records, fsTree: result.fsTree }
  finally
    unsubscribe()
```

### Normalizer.normalize [C:USER]

输入：`snapshot: ScenarioSnapshot`，`options: NormalizerOptions`  
输出：`NormalizedSnapshot`

```
function normalize(snapshot, options)
  normalized := deepClone(snapshot)
  walk(normalized, (value, path) =>
    if value is string
      value := replaceHomeDir(value, options.homeDir, "<HOME>")
      value := replaceTmpDir(value, options.tmpDir, "<TMP>")
      value := replaceTimestamp(value, "<ts>")
      value := replaceUuid(value, options.fixedIds)
      value := replacePid(value, "<pid>")
      value := replacePort(value, "<port>")
      value := replacePathSeparators(value, path)   // 按 pathClass 归一
    if value is number and isTimestampish(path)
      value := 0
    if value is Error-ish object
      keep { code, kind, messageShape }; drop stack/absolute paths
  )
  // 流式分片：保留最终拼接结果；分片数量单独记录为 meta
  if snapshot contains streaming chunks
    joinChunksInPlace(normalized)
  return normalized
```

归一化规则清单（与 roadmap §2.3 对齐） [C:USER]：

| # | 字段类 | 处理 | 正则/规则 |
|---|---|---|---|
| 1 | 时间戳 / duration / hrtime | 置零或 `<ts>` | `/\d{13,}/` 或已知字段名 |
| 2 | UUID / 未固定 sessionId | 替换为 `<id>` | `/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi` |
| 3 | 绝对路径 / homeDir / tmpdir | 替换为 `<HOME>` / `<TMP>` | 字符串替换 options.homeDir/tmpDir |
| 4 | 流式分片边界 | join 后比对；分片数量作为 meta | 合并 `assistant.delta` 文本 |
| 5 | 进程 pid / 端口 | 替换为 `<pid>` / `<port>` | `/\b\d{4,5}\b/` 结合字段名 |
| 6 | 平台行尾 / 路径分隔符 | 经 pathClass 归一 | `\\` → `/` 在 path 字段 |
| 7 | 错误对象 | 比对 `{ code, kind, messageShape }` | 删除 `stack`、message 中的绝对路径 |
| 8 | 固定 seed 的 id | `fixedIds` 映射为 `<id:0>` | 按 scenario 传入的 seed→占位映射 |

### AssertParity.deepEqual [C:INFERRED]

```
function assertParity(ts: NormalizedSnapshot, rust: NormalizedSnapshot): ParityDiff | null
  diffs := empty list
  collectDiffs(ts, rust, "$", diffs)
  if diffs is empty
    return null
  else
    return { scenarioName, ts, rust, diffs }

function collectDiffs(a, b, path, diffs)
  if a and b are both primitive
    if a ≠ b
      diffs.push({ path, tsValue: a, rustValue: b })
  else if a and b are both arrays
    if lengths differ
      diffs.push({ path, tsValue: a.length, rustValue: b.length })
    else
      for i in 0..a.length-1
        collectDiffs(a[i], b[i], `${path}[${i}]`, diffs)
  else if a and b are both objects
    keys := union(a keys, b keys)
    for key in keys
      collectDiffs(a[key], b[key], `${path}.${key}`, diffs)
  else
    diffs.push({ path, tsValue: a, rustValue: b })
```

### BackendFactory.makeTsBackend [C:USER]

```
function makeTsBackend(config: TsBackendConfig): ParityBackend
  endpoint := createRPCEndpoint<CoreAPI, SDKAPI>()
  [leftTransport, rightTransport] := createInProcessTransportPair(
    endpoint.dispatch,
    endpoint.dispatch
  )
  endpoint.setTransport(leftTransport)

  core := new WorkerCoreAPI(endpoint.client, {
    homeDir: config.homeDir,
    llmFactory: config.mockLlm !== undefined
      ? () => config.mockLlm
      : undefined,
  })
  void core

  client := new SDKRpcClient({
    // 内部构造：把 rightTransport 包成 SDKRpcClient 可用的 RPC 端点
    transport: rightTransport,
    homeDir: config.homeDir,
  })

  return {
    kind: 'ts',
    client,
    homeDir: config.homeDir,
    close: async () => { rightTransport.close?.(); leftTransport.close?.(); }
  }
```

> **注意**：`SDKRpcClient` 当前 `constructor` 签名是为外部传输设计的；若不能直接接收 `Transport`，则需在 parity 框架内用 `createRPCEndpoint` 包一层适配器 [C:INFERRED]。

### BackendFactory.makeRustBackend [C:USER]

```
function makeRustBackend(config: RustBackendConfig): ParityBackend
  client := await SDKRpcClient.connect({
    transport: config.transport,
    binaryPath: config.binaryPath,
    homeDir: config.homeDir,
    extraArgs: config.extraArgs,
  })
  return {
    kind: 'rust',
    client,
    homeDir: config.homeDir,
    close: async () => { await client.close?.(); }
  }
```

### CLI/TUI 后端解析 [C:USER]

```
function resolveBackendKind(cliOptions: CLIOptions): BackendKind
  if cliOptions.host !== undefined
    if cliOptions.host === 'rust' return 'rust'
    if cliOptions.host === 'ts'   return 'ts'
    throw OptionConflictError('Invalid --host: must be ts or rust')
  env := process.env['ODY_BACKEND']
  if env === 'rust' return 'rust'
  if env === 'ts'   return 'ts'
  if env !== undefined
    throw new Error('Invalid ODY_BACKEND: must be ts or rust')
  return 'ts'   // 默认保持 TS backend [C:USER]
```

调用点：`apps/ody-code/src/main.ts` 在 dispatch 到 `run-shell.ts` 或 `run-shell-rust.ts` 前，先调用 `resolveBackendKind(opts)` [C:INFERRED]。

---

## Error Handling

### 错误分类与降级

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| Rust 二进制缺失/启动失败 | 抛错并退出 | 无；用户需 `ODY_BACKEND=ts` 或修复构建 [C:USER] | 二进制可执行后重试 |
| Parity diff 不匹配 | 测试失败，输出 diff | 若该 scenario 在 `known-gaps.md` 中登记，则 skip 并标记 gap | 修复 Rust 实现后重跑 |
| TS backend 构造失败 | 测试失败 | 无；harness 本身 bug | 修复 harness |
| Normalizer 规则误伤 | 测试失败（must-survive 被改写） | 收紧/修正归一化规则 | 规则修正 |
| Scenario 超时 | 失败并收集已产生事件 | 增加 timeout 配置 | 后端响应后重跑 |

### Known Gaps 机制 [C:INFERRED]

`known-gaps.md` 格式：

```markdown
| Scenario | Layer | Reason |
|---|---|---|
| mock prompt | L3 | Rust mock provider 事件 payload 未实现对齐 |
| * | L4 | records 持久化 4.3 才迁移 |
```

`ParityRunner` 在运行前读取该表：
- 若 `(scenario.name, layer)` 命中某行 → 该层 skip，测试状态为 `skipped(reason)`。
- 未命中 → 正常执行。
- 若某 scenario 已登记 gap 但实际通过 → 测试失败，提示「gap 已过期，请移除登记」。

---

## Test Plan

### Parity 框架自证测试 [C:USER]

| 测试名 | 断言 |
|---|---|
| `parity/ts-vs-ts/session-lifecycle` | 两个 TS backend 跑同一 scenario，`expect(diff).toBeNull()` |
| `parity/ts-vs-ts/setModel` | 同上，断言 model 字段一致 |
| `parity/ts-vs-ts/mock-prompt` | 同上，断言事件序列一致 |

### TS-vs-Rust 对照测试 [C:USER]

| 测试名 | 断言 |
|---|---|
| `parity/ts-vs-rust/session-lifecycle` | `expect(diff).toBeNull()`；若 Rust 未实现则 `known-gaps.md` skip |
| `parity/ts-vs-rust/setModel` | 同上 |
| `parity/ts-vs-rust/mock-prompt` | 同上；事件序列 `turn.started` / `assistant.delta` / `turn.ended` 顺序一致 |

### Normalizer 单元测试 [C:USER]

| 测试名 | 断言 |
|---|---|
| `normalize/uuid` | 输入含 UUID 的字符串，输出 `<id>` |
| `normalize/path` | 输入 `/Users/x/ody/home`，homeDir=`/Users/x/ody/home` → `<HOME>` |
| `normalize/timestamp` | 输入 `duration: 12345` → `duration: 0` |
| `normalize/must-survive` | 普通文本 `hello 12345` 不被改写 |

### CLI 后端解析测试 [C:USER]

| 测试名 | 断言 |
|---|---|
| `cli/resolveBackend/default` | 无 env 无 flag → `'ts'` |
| `cli/resolveBackend/env` | `ODY_BACKEND=rust` → `'rust'` |
| `cli/resolveBackend/flag-overrides-env` | `ODY_BACKEND=ts --host=rust` → `'rust'` |
| `cli/resolveBackend/invalid` | `ODY_BACKEND=foo` → throws |

### Done Criteria [C:USER]

```bash
# TS-vs-TS 自比对绿
pnpm --filter integration-tests vitest run test/parity/ts-vs-ts.test.ts

# TS-vs-Rust 在已知 gap 外绿
pnpm --filter integration-tests vitest run test/parity/ts-vs-rust.test.ts

# typecheck 绿
pnpm --filter integration-tests typecheck

# CLI 相关测试绿
pnpm --filter ody-code vitest run test/cli/resolve-backend.test.ts

# CI parity job 绿
act -j parity   # 或 push 后看 GitHub Actions
```

---

## Reuse Analysis

### 可复用组件

| 组件 | 位置 | 复用方式 | 备注 |
|---|---|---|---|
| `createCoreServer` | `packages/node-sdk/src/core-server.ts:23` | 参考其 `createRPCEndpoint` + transport 模式 | 不直接复用，因为 parity 需要注入 mock LLM |
| `WorkerCoreAPI` | `packages/agent-core/src/rpc/worker-core.ts:10` | 直接构造 | 接受 `llmFactory`，可注入 `MockChatProvider` [C:USER] |
| `MockChatProvider` | `packages/kosong/test/fixtures/mock-provider.ts:32` | 直接复用 | TS backend 的确定性 mock LLM [C:USER] |
| `createRPCEndpoint` | `packages/agent-core/src/rpc/client.ts` | 直接复用 | 创建 TS backend 内存 RPC 端点 |
| `createInProcessTransportPair` | `packages/agent-core/src/rpc/transport.ts` | 直接复用 | TS↔TS 内存传输 [C:USER] |
| `SDKRpcClient.connect` | `packages/node-sdk/src/rpc.ts` | 直接复用 | 连接 Rust host [C:USER] |
| `withRustHost` 模式 | `packages/node-sdk/test/rust-host-connect.test.ts:42` | 适配复用 | 生命周期管理、tmpdir、binary 解析 |
| `RustHostConnector` | `apps/ody-code/src/host/rust-host-connector.ts:18` | CLI/TUI 已使用 | `--host=rust` 路径已存在 [C:USER] |
| `RustHostHarness` | `apps/ody-code/src/host/rust-host-harness.ts:29` | CLI/TUI 已使用 | 实现 `OdyHarness` [C:USER] |

### 需新建组件

- `ParityDriver`、`BackendFactory`、`Normalizer`、`AssertParity`（parity 框架核心）。
- `resolveBackendKind`（CLI 后端解析辅助函数）。
- 三个 TypeScript scenario 文件。

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify | Source |
|---|---|---|---|---|---|
| A1 | `SDKRpcClient` 的 constructor 可直接接收一个 `Transport` 用于 TS backend 内存连接 | Medium | 需额外写适配层 | 阅读 `packages/node-sdk/src/rpc.ts` constructor 签名 | [C:INFERRED] |
| A2 | `createInProcessTransportPair` 可产生一对互连的 `Transport`，分别给 `WorkerCoreAPI` 和 `SDKRpcClient` | High | TS backend 无法运行 | 阅读 `packages/agent-core/src/rpc/transport.ts` | [C:INFERRED] |
| A3 | Rust host `--mock-provider` 与 TS `MockChatProvider` 产生的事件序列语义一致 | Medium | L3 parity 失败 | 4.0 跑 mock-prompt scenario 验证 | [C:INFERRED] |
| A4 | CLI/TUI 当前 `--host=inproc` 可安全重命名为 `--host=ts`（允许的 breaking change） | Medium | CLI 用户脚本失效 | 用户确认 + 搜索内部/文档引用 | [C:USER] |
| A5 | `packages/integration-tests` 的 `package.json` 已声明所需依赖（agent-core, node-sdk 等） | High | 需补依赖 | 阅读 `packages/integration-tests/package.json` | [C:INFERRED] |
| A6 | 三个 4.0 scenario 无需真实 LLM 网络请求；mock provider 足够 | High | 需录制 fixture | 用 mock provider 运行 scenario | [C:USER] |
| A7 | `ODY_BACKEND` 环境变量在 CI/用户环境可正常传递 | High | 测试无法切换后端 | CI workflow 显式设置 | [C:USER] |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | `SDKRpcClient` 不接受裸 `Transport`，TS backend 需额外适配 | Medium | 增加 0.5–1 天工作量 | 预留适配层；若不可行则改用 `WorkerCoreAPI` 的 RPC 方法直接驱动 |
| R2 | mock provider 两侧事件顺序/字段不一致 | Medium | L3 parity 无法绿 | 把 mock provider 本身作为 4.0 首个 L1 对齐目标 |
| R3 | 归一化规则过度，掩盖真实差异 | Low | 测试假绿 | 每条归一化规则 PR 单独评审； must-survive 单元测试守护 |
| R4 | `--host=inproc` 重命名破坏内部脚本/文档 | Medium | 集成测试或用户脚本失效 | 全局搜索替换内部引用；changeset 标记 minor/major 待确认 |
| R5 | CI parity job 因平台差异（路径/行尾）不稳定 | Medium | PR 频繁失败 | 矩阵覆盖 darwin + linux；归一化规则优先处理路径/行尾 |
| R6 | known-gaps.md 与实际实现脱节 | Low | 跳过已修复项或运行未就绪项 | 测试强制：已登记 gap 通过时失败 |

---

## Self-Review

### 高代价决策 + 3 组 concrete inputs

#### D1 — Normalizer 正则是否误伤/漏过

已用 `node -e` 验证：

| 输入 | 期望 | 实际 | 结论 |
|---|---|---|---|
| `session-550e8400-e29b-41d4-a716-446655440000-end` | UUID 被替换 | `session-<id>-end` | ✅ |
| `uuid:550E8400-E29B-41D4-A716-446655440000` | 大写 UUID 被替换 | `uuid:<id>` | ✅ |
| `short-550e8400-e29b-41d4-a716-44665544000` | 31 字符非 UUID 保留 | 原样保留 | ✅ |
| `duration:1719782400000` | 13 位时间戳替换 | `duration:<ts>` | ✅ |
| `count:12345` | 5 位数字保留 | 原样保留 | ✅ |
| `hello 12345 world` | 普通文本保留 | 原样保留 | ✅ |

**风险残留**：若 `homeDir` 过短（如 `/a`），字符串替换会误伤正常文本。mitigation：实现时要求 `homeDir` 为绝对路径，并优先做边界检查（如仅替换路径前缀）。

#### D2 — `resolveBackendKind` 优先级

已验证：
- 无 env 无 flag → `'ts'`
- `ODY_BACKEND=rust` → `'rust'`
- `--host=rust` + `ODY_BACKEND=ts` → `'rust'`（flag 覆盖 env）
- `ODY_BACKEND=foo` 或 `--host=foo` → throw

#### D3 — Known-gap 通配匹配

已验证：
- `'mock prompt'` 匹配 `'mock prompt'`
- `'mock prompt'` 匹配 `'*'`
- `'session lifecycle'` 不匹配 `'mock prompt'`

### 四镜检视

**Security**
- UUID/时间戳/路径正则经节点脚本验证，无显著 false positive/negative。
- 路径替换需确保 `homeDir` 为绝对路径，避免短路径误伤。
- parity 失败 artifact 可能含用户 prompt / 文件路径；CI artifact 应仅保留最近 N 天，不公开访问。
- `known-gaps.md` 只登记技术差异原因，不涉 secret。

**Test**
- 每个行为都有 must-pass + must-reject：
  - must-pass：TS-vs-TS 自比对绿、有效 ODY_BACKEND 生效
  - must-reject：无效 env/flag 抛错、已登记 gap 实际通过时失败、normalizer 误伤普通文本时失败
- 关键常量与断言自洽：`resolveBackendKind` 的默认值 `'ts'` 与测试 `cli/resolveBackend/default` 一致。

**Ops**
- 每次 scenario 都新建 homeDir 并清理，避免状态泄漏。
- Rust backend 每个 scenario 单独 spawn；并发测试会启动多个 `ody-host` 进程，CI 需保证资源。
- 测试运行时间约为同等工作量 TS 测试的 2–3 倍（双后端 + 归一化），需在 PR 中关注超时。

**Integration**
- 依赖的代码钩子均存在：
  - `createRPCEndpoint` / `createInProcessTransportPair`（`agent-core/src/rpc/`）
  - `WorkerCoreAPI` + `llmFactory`（`agent-core/src/rpc/worker-core.ts`、`core-impl.ts`）
  - `MockChatProvider`（`packages/kosong/test/fixtures/mock-provider.ts`）
  - `SDKRpcClient.connect`（`packages/node-sdk/src/rpc.ts`）
  - Rust `--mock-provider`（`rust-ody/crates/ody-host/src/config.rs`）
  - CLI Rust host 路径（`apps/ody-code/src/host/rust-host-connector.ts`、`run-shell-rust.ts`）
- 设计落点为用户指定的 `packages/integration-tests/src/parity/`，未静默改目标。

**Scope**
- 本设计只覆盖 Phase 4.0：parity 框架 + 双后端开关。4.1–4.5 的真实模块迁移不在本设计范围内，边界清晰。

---

## User Final Approval

- **Audit level**: Deep [C:USER]
- **Audit gate §1 (Scope + Architecture)**: 接受 [C:USER]
- **Audit gate §2 (Data Models + Algorithms + A1/A2/A3)**: 全部接受 [C:USER]
- **Audit gate §3 (Error Handling + Test Plan + Reuse Analysis + A5/A7)**: 全部接受 [C:USER]
- **最终状态**: 已通过深度审计门，待 ExitDesignMode 批准
