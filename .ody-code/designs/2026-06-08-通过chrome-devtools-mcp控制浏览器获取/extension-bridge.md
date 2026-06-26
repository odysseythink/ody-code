# Part: Extension Bridge (Phase 2)

## Purpose

定义可选的 Chrome Extension 桥接方案，作为 CDP/Puppeteer 连接的高级增强。Extension 桥接能提供更稳定的连接、复用用户已登录态、访问 cookies/localStorage、以及跨标签页能力。

## Status

**Deferred to Phase 2**. 本文件只定义接口、数据流和回退策略，不实现扩展本身。

## Why Extension Bridge?

| Capability | CDP Only | Extension Bridge |
|---|---|---|
| Reuse existing Chrome login state | ✅ (when Chrome is running) | ✅ (always) |
| Avoid `--remote-debugging-port` setup | ❌ | ✅ |
| Access cross-origin iframes | Limited | Better |
| Read/write cookies, localStorage | Requires extra CDP domains | Native via content scripts |
| Stability on macOS | Flaky (DevToolsActivePort, 404 on /json) | High (native messaging or fixed WS) |
| User install friction | Low (just Chrome setup) | High (must install extension) |

## Architecture

```
┌─────────────────┐      Native Messaging      ┌──────────────────────┐
│  Chrome         │ ◄────────────────────────► │  Browser Extension   │
│  (content       │                            │  (background script) │
│   scripts)      │                            └──────────┬───────────┘
└─────────────────┘                                       │ WebSocket
                                                          ▼
                                              ┌──────────────────────┐
                                              │  Extension Bridge    │
                                              │  Server (Node)       │
                                              └──────────┬───────────┘
                                                         │
                                                         ▼
                                              ┌──────────────────────┐
                                              │  Agent-Core          │
                                              │  BrowserConnection   │
                                              │  Manager             │
                                              └──────────────────────┘
```

## Interfaces

```typescript
// packages/agent-core/src/browser/extension-bridge.ts

export interface ExtensionBridgeOptions {
  readonly enabled: boolean;
  readonly wsPort?: number;         // 0 = random available port
  readonly wsEndpoint?: string;     // override full URL
}

export interface ExtensionBridgeClient {
  readonly connected: boolean;
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  send(command: ExtensionCommand): Promise<ExtensionResult>;
}

export type ExtensionCommand =
  | { type: 'navigate'; url: string; tabId?: number }
  | { type: 'click'; selector: string; tabId?: number }
  | { type: 'fill'; selector: string; value: string; tabId?: number }
  | { type: 'evaluate'; script: string; args?: unknown[]; tabId?: number }
  | { type: 'snapshot'; tabId?: number }
  | { type: 'screenshot'; fullPage?: boolean; tabId?: number }
  | { type: 'getCookies'; url?: string }
  | { type: 'setCookie'; cookie: { name: string; value: string; domain?: string } };

export interface ExtensionResult {
  readonly success: boolean;
  readonly data?: unknown;
  readonly error?: string;
  readonly screenshot?: string; // base64
}
```

## Algorithm: `connectExtensionBridge()`

```text
INPUT: options
OUTPUT: BrowserHandle | undefined

1. If options.enabled === false → RETURN undefined

2. endpoint ← options.wsEndpoint ?? `ws://127.0.0.1:${options.wsPort ?? 9229}`

3. TRY:
     ws ← new WebSocket(endpoint)
     await waitForOpen(ws, timeout=2000)

4. CATCH error:
     LOG info: 'Extension bridge not available', { endpoint, error: error.message }
     RETURN undefined

5. bridge ← new ExtensionBridgeClient(ws)
   handle ← CREATE BrowserHandle(kind='extension', bridge)

6. RETURN handle
```

## Fallback Strategy

In `BrowserConnectionManager.resolveOrLaunchBrowser()`:

```text
1. Try CDP connect (channel or port)
2. If fails and autoLaunch → launch new browser
3. If extensionBridge enabled → try extension bridge
4. If all fail → throw BrowserConnectionError
```

When extension bridge is used as the active handle, atomic tool operations are translated into `ExtensionCommand` messages instead of Puppeteer calls. The high-level tools (`BrowserBrowse`, `BrowserExtract`, `BrowserAct`) remain unchanged because they operate through an abstraction layer.

## Extension Responsibilities (Future Work)

- Maintain a WebSocket server (or connect to Node bridge server).
- Inject content scripts when requested by agent.
- Relay DOM queries, clicks, fills, evaluations to the active tab.
- Capture screenshots via `chrome.tabs.captureVisibleTab`.
- Read/write cookies via `chrome.cookies` API.
- Surface its availability to the agent (heartbeat).

## Risk / Constraints

| Constraint | Implication |
|---|---|
| Chrome Web Store review | Extension publishing adds weeks to timeline. |
| Manifest V3 service worker lifetime | Background script may sleep; need keep-alive or content-script based relay. |
| Cross-origin iframe access | Content scripts can access same-origin iframes; cross-origin requires `chrome.webRequest` or `chrome.scripting.executeScript`. |
| Security model | Extension has broad access to all tabs; must require explicit user approval before connecting to agent. |

## Call-Sites

| Location | File | Integration |
|---|---|---|
| Connection manager | `packages/agent-core/src/browser/connection.ts` | Adds `connectExtensionBridge()` as tertiary strategy. |
| Config schema | `packages/agent-core/src/config/schema.ts` | Adds `browser.extensionBridge` fields. |

## Test Assertions (Phase 2)

1. When extension bridge is enabled and server is running, `resolveOrLaunchBrowser` returns `kind='extension'`.
2. When extension bridge is enabled but server is not running, falls back to CDP or launch without crashing.
3. `ExtensionBridgeClient.send({ type: 'navigate', url })` returns success after extension confirms navigation.
