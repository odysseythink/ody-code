# Phase A1 — TS SDK 详细设计

> **所属设计**: `.ody-code/designs/2026-06-26-index.md`  
> **Part**: 2 / 2  
> **Scope**: `packages/node-sdk/src/rpc.ts` 及其二进制连接测试

---

## 1. Local Scope

### In Scope

1. `createExternalTransport` 保持 argv 构建为 `['serve', ...transportFlags, ...extraArgs]` [C:USER]
2. 在 `createExternalTransport` 内为所有三种 transport mode（stdio / socket / tcp）增加 `proc.on('error')` 监听 [C:USER]
3. spawn error 在启动阶段即 reject `createExternalTransport` 返回的 Promise [C:USER]
4. 更新 `sdk-rpc-client-connect-binary.test.ts`，验证 mock host 收到 `serve` 参数 [C:INFERRED]

### Out of Scope

| # | 项目 | 原因 |
|---|---|---|
| L1 | 修改 `SDKRpcClientConnectOptions` 类型 | A1 不引入新连接选项 |
| L2 | 修改 `waitForReadyMessage` 的解析逻辑 | 只增加 proc error 监听，ready message 语义不变 |
| L3 | 在 `RustHostConnector` 中重试 spawn | A1 只做错误传播 |
| L4 | 修改 WebSocket / TCP 非 spawn 路径 | 非 spawn 模式没有 `proc`，无需新增监听 |

---

## 2. Interfaces & Types

```typescript
// 保持不变
interface SDKRpcClientConnectOptions {
  readonly transport:
    | 'stdio'
    | { readonly socketPath: string; readonly spawn?: boolean }
    | { readonly host: string; readonly port: number; readonly webSocket?: boolean; readonly spawn?: boolean };
  readonly binaryPath?: string;
  readonly token?: string;
  readonly homeDir?: string;
  readonly configPath?: string;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient;
}

interface ExternalTransportResult {
  transport: Transport;
  proc?: ChildProcess | undefined;
}

// 内部函数签名不变，行为增强
async function createExternalTransport(
  options: SDKRpcClientConnectOptions,
  dispatch: Dispatch,
): Promise<ExternalTransportResult>
```

**contract**: `createExternalTransport` 在 binary 无法 spawn 时立即 reject，并携带 binary 路径和实际 argv；在 spawn 成功但 host 未 ready 时仍由 `waitForReadyMessage` 处理超时/错误。

---

## 3. Algorithms

### Algorithm: Spawn with error listener

```
function spawnHost(binaryPath, argv, transportPredicate):
    proc = spawn(binaryPath, argv, { stdio: ['pipe', 'pipe', 'pipe'] })

    return new Promise((resolve, reject) => {
        cleanup = () => {
            proc.off('error', onError)
            // stderr 监听由 waitForReadyMessage 自行清理
        }

        onError = (err) => {
            cleanup()
            reject(new Error(`Failed to spawn host ${binaryPath} with args ${JSON.stringify(argv)}: ${err.message}`))
        }

        proc.once('error', onError)

        waitForReadyMessage(proc.stderr, transportPredicate)
            .then((msg) => {
                cleanup()
                resolve({ proc, readyMessage: msg })
            })
            .catch((err) => {
                cleanup()
                reject(err)
            })
    })
```

### Algorithm: Build argv per transport mode

```
function buildArgv(options):
    extraArgs = []
    if options.configPath !== undefined:
        extraArgs.push('--config', options.configPath)
    if options.homeDir !== undefined:
        extraArgs.push('--home', options.homeDir)

    if options.transport === 'stdio':
        return ['serve', '--stdio', ...extraArgs]
    if options.transport.socketPath !== undefined:
        return ['serve', '--socket-path', options.transport.socketPath, ...extraArgs]
    // tcp
    return ['serve', '--tcp-host', options.transport.host, '--tcp-port', String(options.transport.port), ...extraArgs]
```

---

## 4. Call-Site Integration

### 4.1 `packages/node-sdk/src/rpc.ts:185-275`

**Before/After**: 三个 transport mode 分支中，在 `spawn()` 之后、`waitForReadyMessage()` 之前，统一调用新的 `spawnHost` 辅助函数。原有 `waitForReadyMessage` 调用被替换为 `spawnHost`。

**Pseudocode sketch for stdio branch**:
```typescript
const proc = spawn(binaryPath, argv, { stdio: ['pipe', 'pipe', 'pipe'] });
const { proc: readyProc } = await spawnHost(binaryPath, argv, proc, (msg) => msg.stdio === true);
return {
  transport: createStreamTransport(readyProc.stdout!, readyProc.stdin!, dispatch, { framing: 'length-prefixed' }),
  proc: readyProc,
};
```

**Pseudocode sketch for socket branch**:
```typescript
const proc = spawn(binaryPath, argv, { stdio: ['ignore', 'ignore', 'pipe'] });
await spawnHost(binaryPath, argv, proc, (msg) => msg.socketPath === socketPath);
// ... connect socket ...
```

**Pseudocode sketch for tcp branch**:
```typescript
const proc = spawn(binaryPath, argv, { stdio: ['ignore', 'ignore', 'pipe'] });
await spawnHost(binaryPath, argv, proc, (msg) => msg.host === host && msg.port === port);
// ... connect tcp / websocket ...
```

### 4.2 `packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts`

**Before/After**: mock script 增加对 `serve` 参数位置的断言。

**Pseudocode sketch**:
```typescript
const mode = process.argv.includes('--stdio') ? 'stdio' : 'socket';
assert(process.argv[2] === 'serve', 'mock host expected serve subcommand');
```

---

## 5. Error Handling

| Error Class | Immediate Handling | Degradation Path | Recovery Condition |
|---|---|---|---|
| `ENOENT`（binary 不存在） | `spawnHost` reject，消息包含 binaryPath 和 argv | 调用方 `runShellWithRustHost` 捕获后向用户展示 | 安装 `ody-host` 或指定正确 `--host-binary` |
| `EACCES` / `EPERM`（binary 无执行权限） | `spawnHost` reject | 调用方展示权限错误 | 修正 binary 权限 |
| `EMFILE`（进程 fd 耗尽） | `spawnHost` reject | 调用方展示系统资源错误 | 关闭其他进程或增加 fd 限制 |
| host spawn 成功但 ready message 超时 | `waitForReadyMessage` 永远不 resolve，调用方超时 | 调用方 `close()` 并报告启动超时 | 检查 host stderr 输出或 binary 版本 |
| host spawn 成功后进程提前退出 | `proc.once('exit')` 在 `RustHostConnector.attachDisconnectHandlers` 中触发 | TUI 收到 disconnect 事件并退出 | 重启或检查 host 崩溃原因 |

---

## 6. Test Plan

### TS 单元测试（`packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts`）

| # | 测试名 | 输入 | 断言 |
|---|---|---|---|
| T1 | `spawns stdio binary with serve subcommand` | `transport: 'stdio', binaryPath: mockScript` | mock script 的 stderr/stdout 中 `process.argv[2] === 'serve'` |
| T2 | `spawns socket binary with serve subcommand` | `transport: { socketPath, spawn: true }, binaryPath: mockScript` | mock script 的 `process.argv[2] === 'serve'` 且 `--socket-path` 存在 |
| T3 | `rejects when binary does not exist` | `binaryPath: '/nonexistent/ody-host'` | `SDKRpcClient.connect` reject，message 包含 binaryPath 和 `serve` |
| T4 | `passes --config and --home in argv` | 现有测试保留 | `process.argv` 包含 `--config` 和 `--home` |

### 运行命令

```bash
pnpm vitest run packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts
```

---

## 7. Local Risk Notes

| # | Risk | 说明 |
|---|---|---|
| LR1 | `proc.on('error')` 与 `waitForReadyMessage` 竞态 | 若 error 在 ready 之后触发，已 resolve 的 Promise 不应再 reject；`once` + cleanup 保证只处理一次 |
| LR2 | `spawn` 参数顺序 | mock 测试依赖 `process.argv[2] === 'serve'`，需确保 `serve` 始终位于 argv 索引 2 |
| LR3 | socket/tcp 分支 `stdio: ['ignore', 'ignore', 'pipe']` | stderr 仍为 pipe 以便读取 ready message；spawn error 监听不改变此配置 |
| LR4 | 非 spawn 路径（socket/tcp spawn=false） | 这些路径不调用 spawn，无需监听 error，算法中需显式跳过 |
