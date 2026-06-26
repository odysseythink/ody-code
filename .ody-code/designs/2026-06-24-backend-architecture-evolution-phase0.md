# Phase 0 详细设计：RPC Transport 抽象与 Golden Parity

> **对应总路线图**: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md` 的 "Phase 0 — 双地基锁定"。
> **Document Type**: Design · **Status**: DRAFT (awaiting approval) · **Audit Level**: Deep

---

## Scope In/Out

### In Scope [C:USER]

- 新增 `Transport` 抽象接口,承载 `Uint8Array` 消息,`send` 返回 `Promise<Uint8Array>`。
- 新增 `InProcessTransport` 默认实现:两端 `send` 直连对端 dispatch,保留 `setTimeout(0)` 异步语义,保证**零行为变化**。
- 改造 `createRPC` 通过选项对象 `{ transport?: [Transport, Transport] }` 接收 transport 对;未传入时内部创建默认 `InProcessTransport`。
- 保留旧调用点零改动与旧签名兼容。
- 新增 RPC golden parity 测试:记录重构前后的线消息,断言序列化字节/消息语义一致。
- `rust-ody/` 框架保持现状,作为 Phase 0 的已就绪地基,本阶段不改动。

### Out of Scope [C:USER]

- MessagePort / Socket / stdio transport 实现(Phase 1-B / Phase 2-B)。
- `agent-core` 拆包(Phase 1-C)。
- 任何契约方法签名或 payload 形状变更(Phase 0 严格冻结契约)。
- 多路复用 / callId 路由(因 `send` 已返回 `Promise<Response>`,Phase 0 不需要)。
- 网络鉴权、加密、压缩(无网络暴露)。

---

## Prior Art

本阶段为纯内部重构,无外部上游系统需要照搬。参考先例均为本仓库既有实践:

- `rust-ody/ts/bench.ts` 的 JS-vs-Wasm 逐字节 parity 方法 [C:UPSTREAM]。
- `packages/kaos/test/e2e/*-parity.test.ts` 的"同表面、双实现、断言一致"模式 [C:UPSTREAM]。

---

## Reuse Analysis

| # | 候选文件 | 可复用内容 | 使用方式 |
|---|---|---|---|
| 1 | `packages/agent-core/src/rpc/client.ts:31-103` | 现有 `createRPC`、`bindAllFunctions`、`mapRpcFunction`、`simulateNetwork`、`abortableRpc` | 改造:把 `simulateNetwork` 替换为 `Transport.send`,其余控制流保留 [C:INFERRED] |
| 2 | `packages/agent-core/test/rpc/create-rpc.test.ts:16-202` | 现有路由、错误、原型绑定、JSON 序列化测试 | 扩展:新增 transport-parity 与显式 transport 注入测试 [C:INFERRED] |
| 3 | `packages/agent-core/test/agent/harness/snapshots.ts:12-318` | 消息序列归一化与快照序列化器 | 复用:golden parity 测试中对 volatile 字段做归一化 [C:INFERRED] |
| 4 | `rust-ody/build.sh:1-25` / `rust-ody/ts/bench.ts:42-71` | 双轨 parity + 基准方法论 | 复用:作为 Phase 0 完成状态与后续 Phase 1-A 的基准范本 [C:UPSTREAM] |

> 结论:无现成 Transport 抽象,属 greenfield 接口设计;实现逻辑大量复用现有 `createRPC`。

---

## Architecture & Data Flow

```
callerProxy.method(payload)
  → mapRpcFunction(payload)
    → encodeJson(payload) → Uint8Array
      → transport.send(bytes) ────────────┐
        (InProc: setTimeout(0))            │
                                          ▼
                              peer.dispatch(bytes)
                                → decodeJson(bytes) → payload
                                  → fn(payload)
                                    → result | error
                                  → encodeJson({ ok, value/error }) → Uint8Array
                                → return bytes
                                          │
      ← Promise<Uint8Array> ◄─────────────┘
    ← decodeJson(bytes) → response
  ← return value | throw OdyError
```

关键控制点:

1. `createRPC` 负责把本地实现绑定、暴露远程代理、把方法调用序列化为消息并交给 `Transport`。
2. `Transport` 只负责**字节搬运**,不解析 JSON、不理解 RPC 语义。
3. `InProcessTransport` 是 `Transport` 的进程内实现,两端 `send` 直接调用对端 `dispatch` 函数,保留异步 tick。
4. 错误在 dispatch 侧被捕获并编码为 `OdyErrorPayload`,穿越 transport 后在调用侧由 `fromOdyErrorPayload` 还原。

---

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| A1 | 所有现有 `createRPC` 调用点均使用无参形式 `createRPC<...>()`,可透明降级到默认 `InProcessTransport`;无调用点依赖 `simulateNetwork` 的具体实现细节 | Medium | 若某调用点传了自定义 transport 或依赖内部闭包,默认路径会失效 | Grep `createRPC` 全仓库,确认调用点均为无参 [C:INFERRED] |
| A2 | 所有 `CoreAPI`/`SDKAPI` payload 均 JSON 安全(无函数、`AbortSignal`、流等不可序列化对象穿越) | Medium | 若有不可序列化对象,InProc 也会在新路径中失败 | Phase 0 测试 + 静态扫描 `CoreAPI`/`SDKAPI` 类型 |
| A3 | `Uint8Array` + UTF-8 JSON 编码足以承载当前所有 RPC 消息,无需处理二进制大对象或分片 | Medium | 大消息可能导致内存峰值,但 InProc 场景与今日 `JSON.stringify` 等价 | 在 golden 测试中加入大 payload 案例 |
| A4 | 默认 `InProcessTransport` 的 `send` 永远不会抛出,因此 `onError` 在 Phase 0 不会被触发 | Medium | 若实现缺陷导致 send 抛错,异常传播路径需明确 | 单测覆盖 send 抛错时的 `onError` 回调 |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R0.1 | `Transport` 接口过度设计,为未到来的 Socket 阶段引入不必要抽象 | 中 | 增加理解与维护成本 | Phase 0 只保留 `send/onError/onWire/close`,不引入 callId、流控、分片 [C:USER] |
| R0.2 | 默认 InProcessTransport 引入微小时序变化(如 `setTimeout` 顺序)导致 flaky 测试 | 中 | 测试不稳定 | golden parity 断言消息序列与时序;保留现有 `setTimeout(0)` 语义 [C:USER] |
| R0.3 | 某处调用点未使用默认 transport,导致生产实际仍走旧路径 | 低 | Phase 0 目标未达成 | 所有调用点加显式 transport 注入测试,或 CI 禁止旧签名新调用 |
| R0.4 | `Uint8Array` 编码暴露 `JSON.stringify` 的 undefined/NaN/Date 语义差异 | 中 | 与原行为不一致 | 复用现有 `simulateNetwork` 的 `JSON.stringify/parse`, golden 测试覆盖边界值 |

---

## Parts

本设计为单一相干子系统(RPC Transport 抽象),无需拆分。

---

## Data Models

### Core Types

```typescript
// 一个字节传输通道。Transport 不解释消息内容,只负责搬运字节。
// [C:USER]
interface Transport {
  // 发送字节,返回对端 dispatch 处理后的字节。
  // 对 InProcessTransport 这是同步调用对端 handler 后返回的 Promise。
  // 对 MessagePort/Socket 这将是 postMessage/write → 等待 response 的 Promise。
  send(bytes: Uint8Array): Promise<Uint8Array>;

  // 当底层通道发生不可恢复错误时由 transport 调用。
  // createRPC 会把它转成远端 OdyError 抛给正在等待的调用。
  onError?(error: Error): void;

  // 调试用钩子,记录每个方向上线的原始字节。
  // 不修改行为,仅用于测试与观测。
  onWire?(direction: 'send' | 'recv', bytes: Uint8Array): void;

  // 可选生命周期钩子。InProcessTransport 无需实现;未来网络 transport 用于关闭 socket/port。
  close?(): void;
}

// 一对互连的 Transport,分别用于 createRPC 的左右两端。
// [C:USER]
type TransportPair = [Transport, Transport];

// createRPC 的新选项。保持旧签名兼容:不传 options 时与现在行为一致。
// [C:USER]
interface CreateRPCOptions {
  transport?: TransportPair;
}

// RPC 内部消息:请求 payload 或响应 envelope。
// 注意:Phase 0 没有 envelope 包装 callId,直接序列化 payload/response。
// [C:INFERRED]
type RpcWireMessage = Uint8Array;

// dispatch 函数签名:createRPC 为每端生成的本地消息分发器。
// [C:INFERRED]
type Dispatch = (bytes: Uint8Array) => Promise<Uint8Array>;
```

### Internal Shapes

```typescript
// 与当前 client.ts:17 一致,仅改格式说明。
// [C:UPSTREAM]
type RpcResponse =
  | { readonly ok: true; readonly value: unknown }
  | { readonly ok: false; readonly error: OdyErrorPayload };

// createRPC 返回的客户端签名不变 [C:UPSTREAM]
export type RPCClient<Self, Other> = (self: PromisableMethods<Self>) => Promise<RPCMethods<Other>>;
```

---

## Algorithms

### ALG-1: `createInProcessTransportPair(dispatchLeft, dispatchRight) → [Transport, Transport]`

创建一对互连的进程内 Transport,左端 `send` 调用 `dispatchRight`,右端 `send` 调用 `dispatchLeft`,均通过 `setTimeout(0)` 保持异步语义。

```
function createInProcessTransportPair(
  dispatchLeft: Dispatch,
  dispatchRight: Dispatch,
): TransportPair {
  const left: Transport = {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      return new Promise((resolve, reject) => {
        setTimeout(() => {
          dispatchRight(bytes).then(resolve, reject);
        }, 0);
      });
    },
    onError?: undefined, // InProc 不触发
    onWire?: undefined,
    close(): void { /* no-op */ },
  };

  const right: Transport = {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      return new Promise((resolve, reject) => {
        setTimeout(() => {
          dispatchLeft(bytes).then(resolve, reject);
        }, 0);
      });
    },
    onError?: undefined,
    onWire?: undefined,
    close(): void { /* no-op */ },
  };

  return [left, right];
}
```

关键不变量:
- 两端通过对方 dispatch 函数直接交换字节,无队列、无序列化、无网络。
- `setTimeout(0)` 保留当前 `simulateNetwork` 的异步 tick,使微任务/宏任务顺序与重构前一致。
- 若 `dispatchX` 抛出同步异常,`Promise` 构造会捕获并 reject send,行为与 `simulateNetwork` 中 `await` 一个抛错 Promise 一致。

### ALG-2: `createRPC(options?) → [RPCClient<Left,Right>, RPCClient<Right,Left>]`

整体流程与现有 `client.ts:31-103` 相同,仅把 `simulateNetwork` 替换为 transport 往返。

```
function createRPC<Left, Right>(options?: CreateRPCOptions): [RPCClient<Left,Right>, RPCClient<Right,Left>] {
  const [leftTransport, rightTransport] = options?.transport ?? createInProcessTransportPair(dispatchLeft, dispatchRight);

  const leftReady  = createControlledPromise<PromisableMethods<Left>>();
  const rightReady = createControlledPromise<PromisableMethods<Right>>();

  async function dispatchLeft(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes);
    const boundSelf = await leftReady;
    return await handleRpcCall(boundSelf, payload);
  }

  async function dispatchRight(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes);
    const boundSelf = await rightReady;
    return await handleRpcCall(boundSelf, payload);
  }

  // 如果调用方没有提供 transport,我们现在才有 dispatch 引用,因此采用延迟绑定:
  if (!options?.transport) {
    // 重定向默认创建的双端 transport 的 dispatch 到上述函数
    wireDefaultTransports(leftTransport, rightTransport, dispatchLeft, dispatchRight);
  }

  function handleRpcCall(boundSelf: PromisableMethods<Self>, payload: unknown): Promise<Uint8Array> {
    const { method, args } = payload as { method: string; args: unknown[] };
    const fn = boundSelf[method] as Function;
    try {
      const value = await abortableRpc(Promise.resolve(fn(...args)), /* signal deferred to Phase 1 */);
      return encodeJson({ ok: true, value });
    } catch (error) {
      return encodeJson({ ok: false, error: toOdyErrorPayload(error) });
    }
  }

  function mapRpcFunction(fn: Function, transport: Transport): Function {
    return async (payload: unknown, options?: RPCCallOptions) => {
      const requestBytes = encodeJson({ method: fn.name ?? '<anonymous>', args: [payload] });
      transport.onWire?.('send', requestBytes);
      const responseBytes = await transport.send(requestBytes);
      transport.onWire?.('recv', responseBytes);
      const response = decodeJson(responseBytes) as RpcResponse;
      if (response.ok) return response.value;
      throw fromOdyErrorPayload(response.error);
    };
  }

  async function leftClient(self: PromisableMethods<Left>): Promise<RPCMethods<Right>> {
    leftReady.resolve(bindAllFunctions(self));
    return objectMap(await rightReady, (key, fn) => [key, mapRpcFunction(fn, leftTransport)]) as RPCMethods<Right>;
  }

  async function rightClient(self: PromisableMethods<Right>): Promise<RPCMethods<Left>> {
    rightReady.resolve(bindAllFunctions(self));
    return objectMap(await leftReady, (key, fn) => [key, mapRpcFunction(fn, rightTransport)]) as RPCMethods<Left>;
  }

  return [leftClient, rightClient];
}
```

> **注意**:上式把方法调用编码为 `{ method, args }`,而当前 `mapRpcFunction` 直接传递 payload。为了保持零行为变化,需决定:
> - 方案 A(推荐):保留当前语义,`mapRpcFunction` 只序列化 `payload`,dispatch 端根据"当前被调用的 fn"直接执行,不需要 `method` 字段。
> - 方案 B:显式 envelope 带 `method`,dispatch 端做 method 路由。
> 因 Phase 0 要求零行为变化,应采用**方案 A**,dispatch 端复用当前 `mapRpcFunction` 闭包中的 `fn` 引用。

### ALG-3: `encodeJson / decodeJson` (与当前 `simulateNetwork` 逐字节一致)

```
function encodeJson(value: unknown): Uint8Array {
  const json = JSON.stringify(value);
  // 复刻当前 simulateNetwork 对 undefined 的处理:JSON.stringify(undefined) === undefined
  if (json === undefined) {
    return new Uint8Array();
  }
  return new TextEncoder().encode(json);
}

function decodeJson(bytes: Uint8Array): unknown {
  if (bytes.length === 0) {
    return undefined;
  }
  const json = new TextDecoder().decode(bytes);
  return JSON.parse(json);
}
```

边界:
- `undefined` payload → 空 Uint8Array → decode 为 `undefined`。
- 其它值与今日 `JSON.stringify/parse` 完全一致。

### ALG-4: Golden Parity 测试记录器

```
function createWireRecorder(): {
  record(direction: 'send'|'recv', bytes: Uint8Array): void;
  snapshot(): WireSnapshot[];
} {
  const entries: WireSnapshot[] = [];
  return {
    record(direction, bytes) {
      entries.push({
        direction,
        json: decodeJson(bytes), // 用于可读性
        bytes: bytes.length,     // 用于检测长度漂移
      });
    },
    snapshot() { return entries; },
  };
}
```

测试流程:
1. 用旧 `createRPC` 跑同一组调用,记录请求/响应字节(若旧实现无 hook,可临时 monkey-patch `simulateNetwork`)。
2. 用新 `createRPC({ transport: inProcPair })` 跑同一组调用,通过 `onWire` 记录字节。
3. 对两端记录做归一化(替换 volatile 字段如 UUID、timestamp)。
4. 断言两个快照逐条相等。

---

## Call-Site Integration

### 修改点 1: `packages/agent-core/src/rpc/client.ts:31-103`

把 `createRPC` 签名改为接受 `CreateRPCOptions`,内部用 `Transport` 替代 `simulateNetwork`。

```typescript
// 改造前
export function createRPC<Left, Right>(): [RPCClient<Left, Right>, RPCClient<Right, Left>];

// 改造后 [C:USER]
export function createRPC<Left, Right>(
  options?: CreateRPCOptions,
): [RPCClient<Left, Right>, RPCClient<Right, Left>];
```

改造前调用:
```typescript
const [connectCore, connectHost] = createRPC<CoreSide, HostSide>();
```

改造后调用(零改动,向后兼容):
```typescript
const [connectCore, connectHost] = createRPC<CoreSide, HostSide>();
// 等价于 createRPC<CoreSide, HostSide>({ transport: createInProcessTransportPair(...) })
```

### 修改点 2: 新增 `packages/agent-core/src/rpc/transport.ts`

新增文件,导出:
- `interface Transport`
- `type TransportPair`
- `function createInProcessTransportPair(dispatchLeft, dispatchRight): TransportPair`
- 可选内部辅助 `encodeJson`/`decodeJson`(若 client.ts 也需要,可放在 transport.ts 或 utils)。

### 修改点 3: `packages/agent-core/src/rpc/index.ts`

新增导出:
```typescript
export type { Transport, TransportPair, CreateRPCOptions } from './transport';
export { createInProcessTransportPair } from './transport';
```

### 修改点 4: `packages/node-sdk/src/rpc.ts:137` 与 `packages/agent-core/src/rpc/core-impl.ts:205`

**无需改动** [C:USER]。二者继续使用默认 InProcessTransport。Phase 0 只验证默认路径与旧行为一致。

示例:
```typescript
// packages/node-sdk/src/rpc.ts:137 [C:UPSTREAM]
const [connectCore, connectHost] = createRPC<CoreAPI, SDKAPI>();
```

### 修改点 5: 新增 `packages/agent-core/test/rpc/transport-parity.test.ts`

 golden parity 测试新文件,见 [Test Plan](#test-plan)。

### 修改点 6: `packages/agent-core/test/rpc/create-rpc.test.ts:16-202`

扩展现有测试,加入显式 transport 注入的等价用例,确保默认路径与显式 InProcessTransport 路径结果一致。

---

## Error Handling

| Error Class | Immediate Handling | Degradation Path | Recovery Condition |
|---|---|---|---|
| `Transport.send` rejects | 异常沿调用栈向上传播,`mapRpcFunction` 的 `await transport.send` 抛错 | 调用侧收到原生 `Error` 或 `OdyError`(若 transport 把错误包成 OdyErrorPayload) | transport 层修复或调用方重试 |
| `Transport.onError(error)` 被触发 | createRPC 把所有 pending 的 `send` Promise 全部 reject,错误转成 `OdyError(ErrorCodes.INTERNAL, ...)` | 正在等待的 RPC 调用全部失败;上层可捕获并降级 | transport 通道恢复后重新建连(Phase 1/2) |
| `decodeJson` 失败(非法 UTF-8 或非法 JSON) | dispatch 侧捕获,返回 `{ ok: false, error: toOdyErrorPayload(parseError) }` | 对端收到 `OdyError` | 修复发送端编码 |
| 远端方法抛错(现有行为) | dispatch 侧 `catch` 并返回 `{ ok: false, error: toOdyErrorPayload(error) }` | 调用侧 `fromOdyErrorPayload` 还原为 `OdyError` | 修复被调用方法 |
| `AbortSignal` 已中止(现有行为) | `signal.throwIfAborted()` 在 send 前/后检查,抛出 `AbortError` | 调用侧取消等待 | 重新发起调用(不带 abort) |

> Phase 0 不引入新的错误码;所有 transport 层错误复用 `ErrorCodes.INTERNAL`。

---

## Test Plan

### 现有测试必须继续通过

- `pnpm vitest run packages/agent-core/test/rpc/create-rpc.test.ts`
- `pnpm vitest run packages/agent-core/test/rpc/plugins-rpc.test.ts`
- 任何引用 `createRPC` 的端到端/agent 测试。

### 新增测试 1: 显式 InProcessTransport 等价性

文件: `packages/agent-core/test/rpc/create-rpc.test.ts` 新增 describe 块。

断言:
```typescript
const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({
  transport: createInProcessTransportPair(),
});
// 复用现有 'routes request and response payloads across both sides' 的断言
await expect(hostProxy.requestApproval({ requestId: 'approval-1', toolName: 'Bash' }))
  .resolves.toEqual({ decision: 'approved:Bash' });
```

> 注意:由于 `createInProcessTransportPair` 需要 dispatch 引用,实际测试中会使用工厂版本 `createInProcessTransportPair({ left, right })` 或让 createRPC 内部绑定。具体接口见 [ALG-1](#alg-1-createinprocesstransportpairdispatchleft-dispatchright--transport-transport)。

### 新增测试 2: `onWire` 记录线消息

文件: `packages/agent-core/test/rpc/transport-wire.test.ts`(或合并到 transport-parity.test.ts)。

断言:
```typescript
const wire: { direction: 'send' | 'recv'; json: unknown }[] = [];
const recorder: Transport = {
  send: async (bytes) => {
    wire.push({ direction: 'send', json: decodeJson(bytes) });
    const response = await peerDispatch(bytes);
    wire.push({ direction: 'recv', json: decodeJson(response) });
    return response;
  },
};
// 调用后
expect(wire).toHaveLength(2 * numberOfCalls);
expect(wire[0].direction).toBe('send');
expect(wire[1].direction).toBe('recv');
```

### 新增测试 3: Transport 错误回调传播

断言:
```typescript
const buggyTransport: Transport = {
  send: () => Promise.reject(new Error('channel broken')),
};
const pair: TransportPair = [buggyTransport, buggyTransport];
const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({ transport: pair });
await expect(hostProxy.requestApproval({ requestId: 'x', toolName: 'Bash' }))
  .rejects.toThrow('channel broken');
```

### 新增测试 4: Golden Parity

文件: `packages/agent-core/test/rpc/transport-parity.test.ts`。

目标:比较"旧 createRPC"与"新 createRPC + InProcessTransport"对同一组调用的线消息序列。

由于改造后旧实现将不存在,具体做法为:
1. 在 PR 中先保留旧 `createRPC` 的副本(如 `createRPCLegacy`)用于生成基准快照。
2. 新实现生成目标快照。
3. 两者经 `packages/agent-core/test/agent/harness/snapshots.ts` 的归一化后断言相等。
4. PR 合并前可删除 `createRPCLegacy`;长期由快照文件作为 golden。

断言示例:
```typescript
expect(normalizeWireSnapshot(newSnapshot)).toEqual(
  normalizeWireSnapshot(legacySnapshot),
);
```

### Done Criteria

以下命令必须全部通过:

```bash
# 1. 类型检查
pnpm tsc --noEmit -p packages/agent-core/tsconfig.json

# 2. RPC 相关测试
pnpm vitest run packages/agent-core/test/rpc/create-rpc.test.ts
pnpm vitest run packages/agent-core/test/rpc/plugins-rpc.test.ts
pnpm vitest run packages/agent-core/test/rpc/transport-parity.test.ts

# 3. 全量测试(确保默认 InProcessTransport 未引入回归)
pnpm vitest run packages/agent-core

# 4. rust-ody 构建仍可用(Phase 0 地基)
cd rust-ody && ./build.sh
```

---

## Self-Review

### 1-3 个最昂贵的决策 +  adversarial 输入

**决策 1: `undefined` 通过空 `Uint8Array` 编码/解码**

| # | 输入 | 预期输出 | 验证 |
|---|---|---|---|
| 1.1 | `undefined` | `encode` → 空 `Uint8Array`; `decode` → `undefined` | ✓ 经 `node -e` 验证:0 bytes → undefined |
| 1.2 | `""` | `encode` → 2 bytes `""`; `decode` → `""` | ✓ 验证通过 |
| 1.3 | `null` | `encode` → 4 bytes `"null"`; `decode` → `null` | ✓ 验证通过,与 undefined 不混淆 |

**决策 2: `InProcessTransport` 保留 `setTimeout(0)` 异步语义**

| # | 输入 | 预期输出 |
|---|---|---|
| 2.1 | 顺序调用 A 然后 B | A 的 send Promise 在 macro-task tick 后 resolve,then B 进入下一个 tick;顺序与旧实现一致 |
| 2.2 | 并发调用 A 与 B | 两者同时进入各自 `setTimeout`,调度顺序由事件循环决定,与旧 `simulateNetwork` 一致 |
| 2.3 | 调用后紧跟 `await Promise.resolve()` | RPC 调用仍在至少一个 macro-task tick 后返回,不会在同一次 micro-task 中同步完成 |

**决策 3: `Transport.onError` 触发时 reject 所有 pending send**

| # | 输入 | 预期输出 |
|---|---|---|
| 3.1 | `onError` 在首次调用前触发 | 后续调用遇到已关闭通道,`send` 直接 reject |
| 3.2 | `onError` 在一次 pending `send` 期间触发 | 该 pending Promise reject 为 `OdyError(INTERNAL)`;其它 pending 同步 reject |
| 3.3 | `onError` 在所有调用完成后触发 | 无 pending,仅标记通道不可再用,不影响已返回结果 |

### 四镜扫描

- **Security**:检查了 `onWire` 可能记录含敏感信息的原始字节。发现:golden 测试需要原始字节,但 `onWire` 仅限测试/调试使用,不应在生产环境启用。已在 `Transport` 注释中说明。无输入过滤器/正则,仅依赖 `JSON.stringify/parse`,不会产生 false positive/negative。未发现 PII 泄漏到日志或文件名。
- **Test**:每个行为都有 must-pass 与 must-reject 用例。`undefined`/`""`/`null` 编码已用 `node -e` 验证不会互相混淆。所有断言与既有测试期望(如 `Number.NaN` → `null`)一致。
- **Ops**:每个 RPC 调用仍只引入一个 `setTimeout(0)` macro-task tick,与现状等价。无新增全局标识符,无并发冲突(`Transport` 无共享可变状态,`InProc` 两端互不阻塞)。`close()` 在 InProc 中为空操作,未来 transport 可扩展。
- **Integration**:验证了 `packages/agent-core/src/rpc/client.ts:31-103` 的 `createRPC`/`simulateNetwork`/`bindAllFunctions` 存在。Grep 发现额外调用点(多个 test harness / mcp / create-rpc 测试),但全部使用无参 `createRPC<...>()`;因此修正假设 A1 为"所有调用点均为无参,可透明降级到默认 InProcessTransport"。设计落在用户指定的 `packages/agent-core/src/rpc/` 路径,未静默改目标。
- **Scope**:Phase 0 仍是一个相干子系统(RPC Transport 抽象),未膨胀为独立的多项目设计。

### 内联修正

- 在 ALG-2 中明确保留当前 `mapRpcFunction` 的闭包 `fn` 引用,不引入 `{ method, args }` envelope,确保零行为变化。
- 在 `onWire` 注释中增加"测试/调试专用,勿在生产启用"的警示。

---

## User Final Approval

- [ ] 设计文件已读并理解
- [ ] Scope In/Out 接受
- [ ] Architecture & Data Flow 接受
- [ ] Data Models / Interfaces 接受
- [ ] Algorithms 接受
- [ ] Call-Site Integration 接受
- [ ] Error Handling 接受
- [ ] Test Plan / Done Criteria 接受
- [ ] Risk Register 接受
- [ ] Assumptions & Unverified Items 已逐项确认(见下节审计门)

