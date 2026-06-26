# Part: Connection Manager

## Purpose

`BrowserConnectionManager` 负责浏览器实例的生命周期管理：发现已有 Chrome、连接 CDP、自动启动新实例、以及长期支持扩展桥接。它是所有浏览器工具的唯一入口。

## Scope In / Out

### In

- 已有 Chrome 实例发现与连接（`puppeteer.connect`）。
- 通过 `--remote-debugging-port` 或 `DevToolsActivePort` 文件定位 CDP endpoint。
- 自动启动新的 Chromium 实例（`puppeteer.launch`）。
- 单例 handle 缓存与页面租借（acquire/release）。
- Agent 关闭时的资源清理。

### Out

- Extension bridge 的具体协议实现（见 `extension-bridge.md`） [C:DEFERRED]。
- 浏览器可执行文件的自动下载管理（依赖 Puppeteer 的默认行为） [C:UPSTREAM]。
- 多 tab 多窗口的高级同步策略 [C:DEFERRED]。

## Data Flow

```
BrowserToolExecutor
       │
       ▼
BrowserConnectionManager.resolveOrLaunchBrowser()
       │
       ├──► Strategy 1: connectExistingViaChannel()
       │       └── puppeteer.connect({ channel: 'chrome' })
       │
       ├──► Strategy 2: connectExistingViaPort(port)
       │       └── read DevToolsActivePort → ws endpoint → puppeteer.connect({ browserWSEndpoint })
       │
       ├──► Strategy 3: launchNewBrowser()
       │       └── puppeteer.launch({ headless, userDataDir })
       │
       └──► Strategy 4: connectExtensionBridge()  [C:DEFERRED]
               └── WebSocket → Extension Bridge
```

## Typed Interfaces

```typescript
// packages/agent-core/src/browser/connection.ts

export type BrowserHandleKind = 'connected' | 'launched' | 'extension';

export interface BrowserConnectionOptions {
  readonly chromePort?: number;
  readonly autoLaunch?: boolean;        // default true
  readonly headless?: boolean;          // default true for launched
  readonly userDataDir?: string;
  readonly executablePath?: string;
  readonly extensionBridge?: {
    readonly enabled: boolean;
    readonly wsEndpoint?: string;
  };
}

export interface BrowserHandle {
  readonly id: string;
  readonly kind: BrowserHandleKind;
  readonly browser: Browser;            // Puppeteer Browser
  readonly defaultPage: Page;
  acquirePage(): Promise<Page>;
  releasePage(page: Page): void;
  close(): Promise<void>;
}

export class BrowserConnectionError extends Error {
  constructor(
    message: string,
    public readonly code: 'NO_BROWSER' | 'LAUNCH_FAILED' | 'CONNECT_FAILED' | 'EXTENSION_UNAVAILABLE',
  ) {
    super(message);
  }
}

export class BrowserConnectionManager {
  private activeHandle: BrowserHandle | undefined;
  private closed = false;

  constructor(
    private readonly options: BrowserConnectionOptions,
    private readonly telemetry: Telemetry,
  ) {}

  async resolveOrLaunchBrowser(): Promise<BrowserHandle> {
    // Algorithm in index.md
  }

  getActiveHandle(): BrowserHandle | undefined {
    return this.activeHandle;
  }

  async closeAll(): Promise<void> {
    if (this.activeHandle) {
      await this.activeHandle.close();
      this.activeHandle = undefined;
    }
    this.closed = true;
  }
}
```

## Algorithm: `connectExistingViaChannel()`

```text
INPUT: none
OUTPUT: Browser | undefined

1. TRY:
     browser ← await puppeteer.connect({ channel: 'chrome', defaultViewport: null })
2. CATCH error:
     LOG debug: 'channel connect failed', error.message
     RETURN undefined
3. RETURN browser
```

## Algorithm: `connectExistingViaPort(port)`

```text
INPUT: port (number)
OUTPUT: Browser | undefined

1. TRY:
     a. activeFile ← locate DevToolsActivePort for Chrome on this platform
     b. If activeFile exists:
          lines ← readFileSync(activeFile, 'utf8').split('\n')
          wsPort ← parseInt(lines[0], 10)
          wsPath ← lines[1] ?? '/devtools/browser'
          wsEndpoint ← `ws://127.0.0.1:${wsPort}${wsPath}`
        Else:
          wsEndpoint ← fetch from http://127.0.0.1:port/json/version → webSocketDebuggerUrl

     b. browser ← await puppeteer.connect({ browserWSEndpoint: wsEndpoint, defaultViewport: null })

2. CATCH error:
     LOG warn: 'port connect failed', { port, error: error.message }
     RETURN undefined

3. RETURN browser
```

## Algorithm: `launchNewBrowser()`

```text
INPUT: options
OUTPUT: Browser

1. launchOptions ← {
     headless: options.headless ?? true,
     executablePath: options.executablePath,
     userDataDir: options.userDataDir,
     args: ['--no-sandbox', '--disable-setuid-sandbox']   // CI-friendly defaults
   }

2. TRY:
     browser ← await puppeteer.launch(launchOptions)
3. CATCH error:
     THROW BrowserConnectionError('Failed to launch browser', 'LAUNCH_FAILED')

4. RETURN browser
```

## Call-Sites

| Location | File | Lines | Integration Point |
|---|---|---|---|
| Tool registration | `packages/agent-core/src/agent/tool/index.ts:388-462` | `BrowserConnectionManager` instantiated in `ToolManager` and passed to browser tools. |
| Agent shutdown | `packages/agent-core/src/agent/index.ts` | [C:INFERRED] Need to verify Agent has shutdown hook to call `connectionManager.closeAll()`. |
| Config wiring | `packages/agent-core/src/config/schema.ts` | [C:INFERRED] `browser.*` fields feed into `BrowserConnectionOptions`. |

## Error / Degradation

| Error Code | Meaning | Handling |
|---|---|---|
| `NO_BROWSER` | 没有可用浏览器且不允许启动 | 返回清晰错误，提示用户启动 Chrome 或开启 autoLaunch。 |
| `CONNECT_FAILED` | CDP 连接失败 | 记录日志，尝试下一个策略（port → launch → extension）。 |
| `LAUNCH_FAILED` | 启动 Chromium 失败 | 提示检查 puppeteer 依赖、可执行路径、权限。 |
| `EXTENSION_UNAVAILABLE` | 扩展桥接配置但未连接 | 回退到 CDP 或 launch，并提示扩展状态。 |

## Test Assertions

1. `connectExistingViaChannel` returns a browser when Chrome is running; returns `undefined` when no Chrome is running.
2. `connectExistingViaPort` correctly parses `DevToolsActivePort` file on macOS and connects via WebSocket.
3. `resolveOrLaunchBrowser` returns cached handle on second call without reconnecting.
4. `closeAll` closes the browser and clears the active handle.
5. `launchNewBrowser` uses `headless=true` by default and respects `options.headless=false`.
