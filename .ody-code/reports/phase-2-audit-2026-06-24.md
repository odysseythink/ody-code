# Phase 2 完成度审计报告

> **审计范围**: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md` 中 Phase 2 的三条轨道：2-B（网络 transport）、2-E（首个 Native 模块）、2-D（mode 概念统一）。
> **审计方法**: 源码阅读 + 实际运行脚本/测试 + 构建探测，避免只走 happy path。
> **分支**: `feat/mode-concept-unification`
> **时间**: 2026-06-24

---

## 总体结论

Phase 2 **未全部完成**。代码层面实现了大部分基础设施，但关键门控（Gate）均未真正达标，且 2-D 的变更引入了构建中断和测试回归。当前状态**不推荐合并到 main**。

| 轨道 | 实现度 | 门控达标 | 关键问题 |
|------|--------|----------|----------|
| 2-B 网络 transport | 80% | ❌ G2-B 未达标 | `ody serve` 未进入构建产物；schema 生成 69/169 类型失败；无外部客户端 PoC |
| 2-E 首个 Native | 70% | ❌ G2-E 未达标 | 工作落在 `mcp-host` 而非路线图标明的 `packages/oauth`；成本报告缺 W vs N 决策结论 |
| 2-D mode 统一 | 60% | ❌ G2-D 未达标 | 文档/类型收敛完成，但破坏了 CLI 构建、agent-core/mcp-host typecheck、两个 runtime 测试 |

---

## 2-B｜网络 transport（Socket / `ody serve` / schema）

### 已交付

- `StreamTransport` 实现完整：length-prefixed / ndjson 双 framing、握手、token 鉴权、并发 reqId 关联、`onWire` 钩子。
- `MessagePortTransport`、`WebSocketTransport`、`InProcessTransport` 均存在。
- `ody serve` 子命令在源码中实现：支持 stdio、UDS、TCP、WebSocket 共享端口、单客户端策略、token 鉴权。
- `SDKRpcClient.connect` 支持 stdio / socketPath / TCP / WebSocket。
- `scripts/gen-rpc-schema.ts` 已存在，能输出 `scripts/generated/rpc-schema.json`。

### 测试结果

```bash
cd packages/agent-core && pnpm vitest run test/rpc
# 13 files passed, 50 tests passed

cd apps/ody-code && pnpm vitest run test/cli/serve.test.ts
# 1 file passed, 5 tests passed

cd packages/node-sdk && pnpm vitest run test/sdk-rpc-client-connect.test.ts test/core-server.test.ts
# 2 files passed, 5 tests passed
```

### 未达标项

#### 1. `ody serve` 未进入构建产物，CLI 本身无法构建

**证据**:

```bash
# 当前 dist 是 6 月 24 日构建的，不含 serve 命令
$ node apps/ody-code/dist/main.mjs serve --help
error: unknown option '--stdio'

$ grep -o 'registerServeCommand' apps/ody-code/dist/main.mjs | wc -l
0

# 尝试重新构建 CLI，失败
$ pnpm --filter ody-code build
[MISSING_EXPORT] "RuntimeMode" is not exported by "../../packages/node-sdk/src/index.d.ts"
```

**影响**: 用户无法通过 released CLI 启动 headless server；G2-B 的“外部客户端能连上 `ody serve`”无从谈起。

**根因**: `apps/ody-code/src/cli/options.ts` 从 `@odysseythink/ody-code-sdk` 引入 `RuntimeMode`，node-sdk 以 `export type { RuntimeMode }` 导出，tsdown/rolldown 在生成 dts 时把它当成 value import 处理。这是 2-D 引入的新依赖。

#### 2. 线协议 schema 生成失败率 41%

**证据**:

```bash
$ npx tsx scripts/gen-rpc-schema.ts | tail -10
CoreAPI methods: 65
SDKAPI methods: 7
Type schemas generated: 100
Type schemas failed: 69
```

失败类型多为 inline discriminated-union member，例如：

```text
{ readonly type: "turn.step.retrying"; readonly turnId: number; ... }
{ readonly type: "subagent.failed"; ... }
```

**根因**: `makeRef` 对匿名对象类型直接用 `checker.typeToString(t)` 作为 ref name，然后 `generateSchemaForType` 无法根据这个字符串找到 symbol。这导致跨语言客户端拿到的是 100 个有定义的类型 + 69 个 null，schema 不完整。

#### 3. 无外部客户端 PoC

路线图标明 G2-B 需要“最小外部客户端（curl/Python）能跑通建会话→发 prompt→收事件流”。仓库中无此类脚本、测试或文档。现有 `sdk-rpc-client-connect.test.ts` 只验证了 `createSession`，未发 prompt，未收事件流。

#### 4. 传输层 parity 测试有 happy-path 空洞

`packages/agent-core/test/rpc/transports/transport-parity.test.ts` 定义了完整的 `runScenario`（含 emitEvent / requestApproval / fail / getConfig），但第一个测试用例仅创建 transport 后就 `// skip full scenario for simplicity`，并未真正跑 scenario：

```ts
it('default path and explicit InProcessTransport produce identical wire semantics', async () => {
  const [connectCoreDefault, connectHostDefault] = createInProcessTransportPair(...);
  // skip full scenario for simplicity
});
```

---

## 2-E｜首个 Native 模块（ody-crypto）

### 已交付

- `rust-ody/crates/ody-crypto` napi-rs crate 实现 `randomBytes`、`sha256`、`pkceChallenge`、`verifyIdToken`。
- TS facade `@odysseythink/ody-crypto` + 5 个 per-platform 子包 + TS fallback。
- `apps/ody-code/scripts/native/native-deps.mjs` 注册 ody-crypto，SEA 构建收集 `.node`。
- `.github/workflows/native-crypto.yml` 在 darwin-arm64 / darwin-x64 / linux-x64 / win32-x64 上构建 native + SEA + smoke。
- 报告 `.ody-code/reports/2026-06-25-phase-2e-native-sea-cost.md` 已存在。

### 测试结果

```bash
cd packages/ody-crypto && pnpm vitest run
# 2 files passed, 6 tests passed

cd packages/mcp-host && pnpm vitest run test/oauth
# 3 files passed, 20 tests passed
```

### 未达标项

#### 1. 工作包与路线图标明不一致

路线图原文：

> `oauth` crypto（PKCE/JWT/签名）用 napi-rs。

实际：

- `packages/oauth` **没有** 引用 `@odysseythink/ody-crypto`（grep 无命中）。该包只使用 `node:crypto` 的 `randomUUID` / `randomBytes` 做临时文件名。
- OAuth PKCE / ID token 校验实际发生在 `packages/mcp-host/src/oauth`，由 `mcp-host` 调用 `ody-crypto`。

**影响**: 若路线图的“oauth”指 `packages/oauth` 这个包，则目标未达成；若指整个代码库的 OAuth 流程，则功能已迁移，但语义有歧义。

#### 2. G2-E 决策数据缺失

路线图 G2-E 要求：

> 对比 W vs N 的“性能增益 ÷ 工程复杂度”，产出“后续 I/O 模块走 N 增量还是攒着等 H”的结论。

现有报告只描述了构建/运行时成本、矩阵、风险，**没有**:

- 同一功能在 TS fallback vs native 下的端到端性能对比（latency / throughput / 内存）。
- W（Wasm）方案在同一功能上的假设性成本/收益估算。
- 明确结论：后续 I/O 模块是否继续走 N，还是等 H（Rust Host）。

#### 3. Native 真实二进制未在自动化测试中验证

`packages/ody-crypto/test/loader.test.ts` 用 `vi.mock('node:module')` 把 `require` 直接 mock 成失败或成功对象，**从未加载真实的 `.node` 二进制**。本地手动 `require('./packages/ody-crypto-darwin-arm64/ody-crypto.node')` 成功，但 CI/test 不保证这一点。

#### 4. CI 矩阵不完整

报告已指出风险：`linux-arm64` 与 `win32-arm64` 未纳入 GitHub-hosted matrix，release 需补充。

---

## 2-D｜mode 概念统一

### 已交付

- `SessionModeKind` / `RuntimeMode` 两层类型系统清晰：`SESSION_MODE_KINDS` + `normal`。
- `SystemPromptContext.sessionMode?: RuntimeMode` 作为 mode/profile 收敛点。
- `docs/architecture/modes-vs-profiles.md` 完整文档化责任分层、切换图、决策矩阵、自检查问题。
- `BaseSessionModeInjector` / `ModeBehaviorRegistry` 抽象存在，四种 mode 均有 injector。
- 全仓库字面量替换为 `RuntimeMode`。

### 未达标项

#### 1. 构建中断

- `apps/ody-code` 无法构建（见 2-B 第 1 项）。
- `packages/agent-core` typecheck 失败（9 个错误，多与 2-D 测试相关）。
- `packages/mcp-host` typecheck 失败（3 个错误，其中 2 个在 oauth service）。

#### 2. Runtime 测试回归

全量运行 `packages/agent-core` 测试：

```bash
cd packages/agent-core && pnpm vitest run
# 5 failed | 237 passed
```

与 2-D 直接相关的失败：

- `test/tools/plan-mode-hard-block.test.ts:290` — `sessionMode.exit()` 调用 `this.agent.refreshLlm()`，但 mock agent 未提供该方法。
- `test/agent/tool/enter-office-hours.test.ts:76` — 同上。

这两个测试在 2-D 重构前不依赖 `refreshLlm`，现在需要更新 mock。

#### 3. TypeScript 错误（2-D 测试文件）

- `src/agent/injection/__tests__/*.test.ts` 多处把 `readonly ContextMessage[]` 强转为 mutable 数组，类型不兼容。
- `src/agent/injection/__tests__/session-mode-injector.test.ts:10` 缺少 `override` 修饰符。
- `src/agent/session-mode/__tests__/directory.test.ts:18` 错误引入 `../../agent`（应为 `../../` 或 `../../index`）。
- `test/agent/session-mode.test.ts` 三处给只读属性 `sessionMode` 赋值。

#### 4. CLI help 文本陈旧

`apps/ody-code/src/cli/commands.ts:77`：

```ts
.option('--session-mode <mode>', 'Start in session mode: plan, design, or normal.', 'normal')
```

但 `validateOptions` 接受 `normal | plan | design | office-hours | game-design`。help 应同步为五种，或说明 office-hours/game-design 有独立 flag。

---

## 前置依赖的附带发现

Phase 2 依赖 Phase 1 的门控结果。以下虽未写入 Phase 2，但会削弱 2-B/2-E 的论据：

- **Phase 1-B.5 AbortSignal 审计**: `createRPC` 目前用本地 `abortable()` 辅助函数处理 `AbortSignal`，并未实现为 transport 层 `cancel(callId)` 消息。MessagePort / Socket transport 不会收到取消信号。
- **Phase 1-B worker 崩溃恢复**: 无专门测试验证 worker 异常退出后 UI 存活、可重连降级。
- **Phase 0.3 golden parity**: `transport-parity.test.ts` 里的完整 scenario 被 skip，未形成真正的 golden。

---

## 修复建议（按优先级）

### P0 — 阻断合并

1. **修复 CLI 构建**: 解决 `RuntimeMode` type import 在 tsdown 中的导出/导入问题。可尝试：
   - 在 `apps/ody-code/src/cli/options.ts` 中改为 `import { type RuntimeMode }`（已写，但 dts 生成仍失败）。
   - 或在 `@odysseythink/ody-code-sdk` 中把 `RuntimeMode` 作为 value-safe 的 type 重新导出，并确保 tsdown 识别。
   - 作为兜底，CLI 内部可自己定义 `RuntimeMode` 的 runtime 检查，不再从 SDK import type。
2. **修复 agent-core 与 mcp-host typecheck**: 先修 2-D 引入的测试类型错误，再处理 mcp-host oauth service 的 `URL` / `OAuthClientInformationMixed` 类型不匹配。
3. **修复两个 SessionMode runtime 测试**: 给 mock agent 加上 `refreshLlm: vi.fn()`。

### P1 — 补齐 Phase 2 门控

4. **schema 生成**: 对 discriminated union 的 inline member 生成匿名 schema 或提取命名类型，把失败率降到可接受范围（建议 <5%），并加入 CI 检查。
5. **外部客户端 PoC**: 写一个 Python/curl 脚本跑通 `createSession → prompt → event stream`，作为 G2-B 的客观证据。
6. **G2-E 决策报告**: 补充 TS fallback vs native 的基准数据、W 方案估算、明确“N 增量 or H”结论。

### P2 — 工程卫生

7. 补齐 `transport-parity.test.ts` 中被 skip 的完整 scenario。
8. 更新 `--session-mode` help 文本。
9. 在 ody-crypto loader 测试中增加真实 native 二进制加载测试（至少当前平台）。
10. 将 AbortSignal 从本地 abort 升级为跨 transport 的 `cancel(callId)` 消息（Phase 1-B.5 债务）。

---

## 附录：关键命令速查

```bash
# 2-B schema
npx tsx scripts/gen-rpc-schema.ts

# 2-B RPC / serve 测试
pnpm --filter @odysseythink/agent-core vitest run test/rpc
pnpm --filter ody-code vitest run test/cli/serve.test.ts
pnpm --filter @odysseythink/ody-code-sdk vitest run test/sdk-rpc-client-connect.test.ts

# 2-E native
pnpm --filter @odysseythink/ody-crypto vitest run
pnpm --filter @odysseythink/mcp-host vitest run test/oauth

# 构建 / typecheck
pnpm --filter ody-code build
pnpm --filter @odysseythink/agent-core typecheck
pnpm --filter @odysseythink/ody-code-sdk build
pnpm --filter @odysseythink/mcp-host typecheck

# 全量 agent-core 测试（当前 5 失败）
cd packages/agent-core && pnpm vitest run
```
