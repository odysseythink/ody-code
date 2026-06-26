# Part 3 — TS TUI Adaptation

> Scope: TS TUI 如何启动/连接 Rust host、消费事件流、处理断开错误。  
> Corresponds to index: [Architecture & Data Flow](../2026-06-25-backend-architecture-evolution-phase3.md)

---

## 1. Component Overview

TS TUI 在 Phase 3 原型中的改动是**最小侵入式**的：
1. 复用 `apps/ody-code/src/tui/ody-tui.ts` 及其事件处理管线 [C:INFERRED]。
2. 将底层 `KimiCore` / `SDKRpcClient` 的默认 inproc 路径，替换为 `SDKRpcClient.connect(...)` 连接外部 Rust host [C:USER]。
3. 在 `apps/ody-code/src/main.ts` 或 CLI 层增加 `--host=rust` / `--host-stdio` / `--host-socket` 选项 [C:INFERRED]。
4. 复用现有 reverse-RPC handler：`requestApproval`、`requestQuestion`、`toolCall` [C:USER]。
5. Rust host 断开后，TUI 打印错误并退出 [C:USER]。

---

## 2. Typed Interfaces

### 2.1 RustHostLaunchOptions

```typescript
interface RustHostLaunchOptions {
  readonly mode: 'stdio' | 'socket' | 'tcp';
  readonly binaryPath: string;              // ody-host 可执行文件绝对路径 [C:INFERRED]
  readonly socketPath?: string;             // mode === 'socket'
  readonly host?: string;                   // mode === 'tcp'
  readonly port?: number;                   // mode === 'tcp'
  readonly configPath?: string;             // 传给 --config [C:USER]
  readonly homeDir?: string;                // 传给 --home [C:INFERRED]
}
```

### 2.2 RustHostConnector

```typescript
class RustHostConnector {
  // contract: 根据 options 启动或连接 Rust host，返回一个 SDKRpcClient
  static async connect(options: RustHostLaunchOptions): Promise<SDKRpcClient>;

  // contract: 返回当前连接的进程/transport 是否存活
  get isAlive(): boolean;

  // contract: 注册断开回调（原型阶段用于退出 TUI）
  onDisconnect(handler: (error: Error) => void): Unsubscribe;
}
```

### 2.3 ClientAPI（SDKAPI 实现）

```typescript
class ClientAPI implements SDKAPI {
  constructor(
    private readonly client: SDKRpcClient,
    private readonly getRpc: () => Promise<ResolvedCoreAPI>,
  );

  emitEvent(event: AgentEvent): void;
  requestApproval(request: ApprovalRequest & WithSessionAgentId): Promise<ApprovalResponse>;
  requestQuestion(request: QuestionRequest & WithSessionAgentId): Promise<QuestionResult>;
  toolCall(request: ToolCallRequest): Promise<ToolCallResponse>;
  openExternal(request: OpenExternalRequest & WithSessionAgentId): Promise<OpenExternalResponse>;
  chatStreamInit(payload: ChatStreamInitPayload): Promise<ChatStreamInitResponse>;
  chatStreamCancel(payload: ChatStreamCancelPayload): void;
}
```

> 注：`ClientAPI` 已在 `packages/node-sdk/src/rpc.ts:993-1000` 存在；原型中保持其方法体不变，仅确保底层 transport 指向 Rust host [C:CODE]。

---

## 3. Algorithms

### 3.1 `RustHostConnector.connect` — 启动并连接 Rust host

```
INPUT: options: RustHostLaunchOptions
OUTPUT: SDKRpcClient

1. IF options.mode === 'stdio':
       proc = spawn(options.binaryPath, [
           'serve', '--stdio',
           '--config', options.configPath ?? '',
           '--home', options.homeDir ?? '',
       ], { stdio: ['pipe', 'pipe', 'pipe'] })
       wait_for_ready_on_stderr(proc)     // { type: 'ready', stdio: true }
       transport = createStreamTransport(
           proc.stdout, proc.stdin, dispatch,
           { framing: 'length-prefixed' }
       )
   ELSE IF options.mode === 'socket':
       proc = spawn(options.binaryPath, [
           'serve', '--socket-path', options.socketPath,
           '--config', options.configPath ?? '',
           '--home', options.homeDir ?? '',
       ])
       wait_for_ready_on_stderr(proc)     // { type: 'ready', socketPath: '...' }
       socket = net.connect(options.socketPath)
       await once(socket, 'connect')
       transport = createStreamTransport(socket, socket, dispatch, { framing: 'length-prefixed' })
   ELSE IF options.mode === 'tcp':
       // 类似 socket，但 --tcp-host/--tcp-port
2. endpoint = createRPCEndpoint<SDKAPI, CoreAPI>()
3. endpoint.setTransport(transport)
4. client = new SDKRpcClient({}, true)   // external mode
5. clientApi = new ClientAPI(client, () => client.getRpc())
6. rpc = await endpoint.client(clientApi)
7. assign client.rpc = rpc
8. register transport.onError / proc.on('exit') -> trigger onDisconnect handlers
9. RETURN client
```

### 3.2 `main` — CLI 入口适配

```
INPUT: argv

1. Parse argv for --host=rust | --host-stdio | --host-socket PATH | --host-tcp HOST:PORT
2. IF any rust host flag present:
       options = buildRustHostLaunchOptions(argv)
       client = await RustHostConnector.connect(options)
       attachDisconnectHandler(client, () => {
           console.error('Rust host disconnected.')
           process.exit(1)
       })
       startTui(client)              // 复用 OdyTUI
   ELSE:
       startTuiWithExistingInprocOrWorker()   // 现有路径不变 [C:INFERRED]
```

### 3.3 `startTui` — 复用 OdyTUI

```
INPUT: client: SDKRpcClient

1. tui = new OdyTUI({
       client,                         // 替换原来的 KimiHarness/Session
       // 其余选项（theme, identity, etc.）保持不变
   })
2. tui.onExit(code) => process.exit(code)
3. await tui.run()
```

### 3.4 Event consumption — 复用 `SessionEventHandler`

```
1. client.onEvent((event) => sessionEventHandler.dispatch(event))
2. sessionEventHandler 保持现有逻辑：
   - 'user' -> append to transcript
   - 'assistant.delta' -> update streaming pane
   - 'assistant.finish' -> finalize message
   - 'tool.result' -> append tool card
   - 'error' -> show error banner
   - 'requestApproval' -> 已由 ClientAPI.requestApproval 处理
```

---

## 4. Call-Site Integration

### 4.1 `apps/ody-code/src/main.ts`

当前 `main.ts` 调用 `handleMainCommand` 创建本地 `KimiHarness` 或 worker。原型中增加分支：

```typescript
// approx line range unknown; new branch after argv parse
if (argv.host === 'rust') {
  const client = await RustHostConnector.connect({
    mode: argv.hostSocket ? 'socket' : argv.hostTcp ? 'tcp' : 'stdio',
    binaryPath: await resolveHostBinary(argv),
    socketPath: argv.hostSocket,
    host: argv.hostTcp?.split(':')[0],
    port: Number(argv.hostTcp?.split(':')[1]),
    configPath: argv.config,
    homeDir: argv.home,
  });
  client.onDisconnect(() => {
    console.error(chalk.red('Rust host disconnected.'));
    process.exit(1);
  });
  return runTui({ client, identity, ... });
}
```

### 4.2 `apps/ody-code/src/cli/commands.ts`

在 Commander program 中新增选项：

```typescript
program
  .option('--host <mode>', 'Connect to Rust host: rust | inproc | worker', 'inproc')
  .option('--host-stdio', 'Launch Rust host in stdio mode (default when --host=rust)')
  .option('--host-socket <path>', 'Launch Rust host listening on Unix socket')
  .option('--host-tcp <host:port>', 'Launch Rust host listening on TCP')
  .option('--host-binary <path>', 'Path to ody-host executable');
```

### 4.3 `apps/ody-code/src/tui/ody-tui.ts`

改动点：
- 构造函数参数从 `KimiHarness` 改为 `SDKRpcClient`（或保持 `KimiHarness` 接口但由 Rust host client 实现）。
- 复用 `client.onEvent` 替代 `harness.onEvent`。
- 其余 transcript、streaming、approval/question panels 不变。

---

## 5. Error Handling（局部）

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `Rust host spawn failed` | Print stderr + exit(1) | TUI 不启动 | 修正 binaryPath / 权限 |
| `Transport connect timeout` | Print timeout message + exit(1) | TUI 不启动 | 检查 host 是否已启动 |
| `Rust host disconnected` | Print error + exit(1) | TUI 退出（原型策略） | 用户重新启动 ody |
| `Reverse RPC timeout` | ClientAPI returns cancelled/null | Tool/approval 失败 | 用户重试 |
| `MethodNotImplemented` | Show error banner in TUI | 该操作不可用 | 不使用原型未覆盖功能 |

---

## 6. Local Test Notes

### Must-pass assertions

1. `pnpm vitest run apps/ody-code/src/tui/__tests__/rust-host-connector.test.ts`:
   - `connects_to_rust_host_via_stdio` — spawn mock Rust host（echo ready + respond to getCoreInfo），connector 返回可用 client。
   - `emits_disconnect_when_host_exits` — mock host 退出后 onDisconnect 被触发。
2. Manual end-to-end:
   - `ody --host=rust --host-stdio` 启动后能看到 TUI 并创建会话。
   - 在 TUI 中发送 prompt，看到 Rust host 返回的 assistant 消息。
   - `ody --host=rust --host-socket /tmp/ody.sock` 启动后重复上述流程。

### Must-reject assertions

1. `--host=rust` but binary missing → clear error and exit(1).
2. Rust host exits while TUI running → TUI prints "Rust host disconnected" and exits(1).
3. Calling prototype-out-of-scope CoreAPI method（e.g. `requestCodeReview`）→ TUI shows `MethodNotImplemented` error.
