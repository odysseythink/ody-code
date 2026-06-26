# 浏览器控制替代方案设计：从 Chrome DevTools MCP 到 Agent-Core 原生 Browser Agent

## Meta

- **Design ID**: `2026-06-08-通过chrome-devtools-mcp控制浏览器获取`
- **Audit Level**: Deep [C:USER]
- **Authoring Date**: 2026-06-08
- **Status**: Awaiting approval
- **Related Changesets**: TBD after approval

## Summary

将浏览器控制能力从 vendored `chrome-devtools` MCP server 下沉为 `packages/agent-core` 的原生内置工具，解决当前 MCP 在 macOS 上的连接不稳定、stdIO 开销大、工具粒度太细的问题。新方案提供三层能力：

1. **Connection Layer**: Puppeteer-based 浏览器连接管理器，优先 `puppeteer.connect({ channel: 'chrome' })` 复用用户已有 Chrome（及其登录态），失败时自动 `puppeteer.launch()` 启动新实例；长期可选 Chrome Extension 桥接进一步增强稳定性。
2. **Tool Layer**: 高阶意图工具（`BrowserBrowse`, `BrowserExtract`, `BrowserAct`）暴露给模型，内部自动编排原子操作；原子工具（`BrowserNavigate`, `BrowserSnapshot`, `BrowserClick`, `BrowserFill`, `BrowserEvaluate`, `BrowserScreenshot`）作为 fallback / debug 保留。
3. **Permission Layer**: 按 URL host 的 session-level 授权模型，首次访问某域名时 ask，同域名后续自动放行；敏感写操作（密码输入、提交支付）仍可单独 ask。

最终逐步替代内置的 `chrome-devtools` MCP server，但保留配置开关以兼容旧行为。

## Decision Record

| # | Dimension | Decision | Source |
|---|---|---|---|
| 1 | 目标 | 稳定 + 功能丰富 + 复用登录态 | [C:USER] |
| 2 | 浏览器扩展 | 可选增强：基础 CDP 可用，扩展解锁高级能力 | [C:USER] |
| 3 | 架构层级 | 下沉为 `agent-core` 原生工具 | [C:USER] |
| 4 | 底层引擎 | Puppeteer（CDP 原生） | [C:USER] |
| 5 | 生命周期 | 优先连接已有 Chrome，失败时自动启动新实例 | [C:USER] |
| 6 | 权限模型 | 按 URL/域名 session-level 授权 | [C:USER] |
| 7 | 接口设计 | 高阶意图工具 + 原子工具 fallback | [C:USER] |
| 8 | 总体方案 | 方案 C：扩展桥接增强版（完整版） | [C:USER] |

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `index.md` | 总体架构、决策记录、Assumptions、Risk Register | done |
| 2 | `connection-manager.md` | Puppeteer/CDP 连接与生命周期 | done |
| 3 | `agent-core-tools.md` | 高阶工具与原子工具接口 | done |
| 4 | `permission-model.md` | URL-based 权限策略 | done |
| 5 | `extension-bridge.md` | Chrome Extension 可选桥接 | done |
| 6 | `config-migration.md` | 配置、MCP 兼容与迁移 | done |

## Prior Art

### Claude Code (Anthropic)

- **Computer Use MCP**: 原生内置，基于截图+鼠标/键盘模拟的通用 "computer use" 接口。Claude 遵循 Connector → Shell → Browser Connector → Computer Use 的优先级分层。
- **MCP 生态**: 官方提供 `@modelcontextprotocol/server-puppeteer` 和 `@modelcontextprotocol/server-playwright`；社区有 `@browsermcp/mcp`（浏览器扩展桥接）、BrowserToolsMCP（扩展+HTTP bridge）。
- **启示**: 浏览器能力应分层；高阶意图工具比原子工具更适合 agent；扩展桥接能复用登录态并避免 bot 检测。

### Codex (OpenAI)

- **Browser Use Plugin**: 内置插件体系，两个后端：
  - `iab` (in-app browser): Codex 应用内嵌浏览器。
  - `chrome` (extension): 通过 Codex Chrome Extension 控制用户已有 Chrome。
- **运行时**: `node_repl` + `codex app-server` + plugin runtime；sandbox policy 控制 `networkAccess`。
- **痛点**: Windows 上 `codex app-server` 路径问题频发；企业网络策略阻断外部导航；扩展安装是必需项。
- **启示**: 扩展方案虽好但实施复杂；内置的、可控的运行时比外部 helper 进程更可靠；权限策略需要按 origin 而非全局控制。

### 其他参考

- `@browsermcp/mcp`: 扩展+本地 stdio MCP server，能直接操作当前 active tab，避免 `remote-debugging-port` 配置。
- BrowserToolsMCP: 100% MCP 2025-03-26 规范，自定义 HTTP bridge，9 个浏览器工具。
- Wavebox MCP: 通过 AppleScript / Native Messaging 控制 Wavebox 浏览器，复用多账号登录态。

## Scope In / Scope Out

### In Scope

- `packages/agent-core` 新增 `BrowserConnectionManager` 与浏览器工具族。
- 新增高阶意图工具：`BrowserBrowse`, `BrowserExtract`, `BrowserAct`。
- 新增原子工具（fallback）: `BrowserNavigate`, `BrowserSnapshot`, `BrowserClick`, `BrowserFill`, `BrowserEvaluate`, `BrowserScreenshot`。
- 新增 `BrowserHostPermissionPolicy`，实现按 URL host 的 session-level 授权。
- 配置层扩展 `KimiConfig.browser`，支持 `enabled`, `autoLaunch`, `chromePort`, `extensionBridge`。
- 默认禁用内置 `chrome-devtools` MCP server，但保留 `config.browser.legacyMcpEnabled` 回退开关。
- 单测覆盖：连接管理器、工具执行、权限策略、降级路径。

### Out of Scope (Phase 2 / Future)

- Chrome Extension 的完整实现与商店发布（本设计只定义扩展桥接接口与回退机制） [C:DEFERRED]。
- Playwright 引擎支持（当前仅 Puppeteer） [C:DEFERRED]。
- Computer Use 式的像素级鼠标模拟 / 视觉推理 [C:DEFERRED]。
- 浏览器录制、性能 trace、Lighthouse 审计等高级 DevTools 功能 [C:DEFERRED]。
- 跨 tab / 跨窗口的复杂同步场景（扩展桥接阶段再解决） [C:DEFERRED]。

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Agent Core Loop                                │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                        ToolManager (tool/index.ts)                    │  │
│  │   registers: BrowserBrowse, BrowserExtract, BrowserAct               │  │
│  │   + atomic fallbacks: BrowserNavigate, BrowserSnapshot, ...          │  │
│  └────────────────────┬──────────────────────────────────────────────────┘  │
│                       │ calls via ExecutableTool.execute()                  │
│                       ▼                                                     │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                  BrowserConnectionManager                              │  │
│  │   - resolveOrLaunchBrowser() → BrowserHandle                          │  │
│  │   - page pool (single active page)                                    │  │
│  │   - CDP session cache                                                 │  │
│  └────────────────────┬──────────────────────────────────────────────────┘  │
│                       │                                                     │
│         ┌─────────────┼─────────────┐                                       │
│         ▼             ▼             ▼                                       │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐                              │
│  │ puppeteer│  │ puppeteer│  │ Extension    │                              │
│  │ .connect │  │ .launch  │  │ Bridge       │  (deferred)                  │
│  │ (channel)│  │ (new)    │  │ (WebSocket)  │                              │
│  └────┬─────┘  └────┬─────┘  └──────┬───────┘                              │
│       │             │               │                                       │
│       └─────────────┴───────────────┘                                       │
│                     │                                                       │
│                     ▼                                                       │
│              Chrome / Chromium                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Data Flow

### 1. High-level tool invocation flow

```
Model ──► BrowserBrowse(url, goal)
            │
            ▼
    BrowserToolExecutor
            │
            ├──► PermissionManager.beforeToolCall()
            │         └── BrowserHostPermissionPolicy.evaluate(url.host)
            │             └── ask / approve / deny
            ▼
    BrowserConnectionManager.resolveOrLaunchBrowser()
            │
            ├──► try puppeteer.connect({ channel: 'chrome' })
            ├──► fallback puppeteer.launch()
            ▼
    PageController.navigate(url) ──► waitForLoad() ──► snapshotOrExtract()
            │
            ▼
    Return ToolOutput
```

### 2. Permission approval flow

```
Tool call BrowserBrowse('https://kimi.com/code/console', ...)
            │
            ▼
    BrowserHostPermissionPolicy
            │
            ├──► parse URL host = 'kimi.com'
            ├──► check config.browser.allowedHosts (static allowlist)
            ├──► check sessionApprovalRulePatterns for 'Browser*(kimi.com)'
            ├──► if none matched → kind: 'ask'
            │         reason: { host: 'kimi.com', tool: 'BrowserBrowse' }
            ▼
    User approves with scope='session'
            │
            ▼
    PermissionManager.recordApprovalResult()
            └── sessionApprovalRulePatterns.add('Browser*(kimi.com)')
            └── subsequent calls on *.kimi.com auto-approved
```

### 3. Extension bridge fallback flow (Phase 2)

```
Agent Core ──► BrowserConnectionManager.connect()
            │
            ├──► primary: puppeteer.connect({ channel: 'chrome' })
            ├──► fallback: puppeteer.launch()
            └──► tertiary (if extensionBridge enabled):
                     ExtensionBridgeClient.connect(ws://127.0.0.1:<port>)
                     └── Chrome Extension Native Messaging / WebSocket
```

## Typed Interfaces

### BrowserConnectionManager

```typescript
interface BrowserConnectionOptions {
  chromePort?: number;           // explicit remote debugging port
  autoLaunch?: boolean;          // default true
  headless?: boolean;            // only for launched instances
  userDataDir?: string;          // only for launched instances
  extensionBridge?: {
    enabled: boolean;
    wsEndpoint?: string;
  };
}

interface BrowserHandle {
  readonly id: string;
  readonly kind: 'connected' | 'launched' | 'extension';
  readonly browser: Browser;     // Puppeteer Browser
  readonly defaultPage: Page;
  acquirePage(): Promise<Page>;
  releasePage(page: Page): void;
  close(): Promise<void>;
}

class BrowserConnectionManager {
  constructor(options: BrowserConnectionOptions);
  resolveOrLaunchBrowser(): Promise<BrowserHandle>;
  getActiveHandle(): BrowserHandle | undefined;
  closeAll(): Promise<void>;
}
```

### Browser Tool Executor

```typescript
interface BrowserToolContext {
  readonly connection: BrowserConnectionManager;
  readonly permission: PermissionManager;
  readonly telemetry: Telemetry;
}

// Each browser tool's resolveExecution() returns an execution with:
//   approvalRule: string   // e.g. 'Browser*(kimi.com)'
//   matchesRule: (argPattern: string) => boolean   // true if argPattern === url.host
// This allows SessionApprovalHistoryPermissionPolicy to cache per-host approvals.
```

interface BrowserBrowseInput {
  url: string;
  goal?: string;                 // e.g. "check remaining quota"
  waitFor?: string | number;     // selector or ms
  extract?: Record<string, string>; // schema for structured extraction
}

interface BrowserExtractInput {
  url?: string;                  // if omitted, use current page
  schema: Record<string, string>;
}

interface BrowserActInput {
  instruction: string;           // natural language, e.g. "click the login button"
  selectors?: string[];          // optional hint selectors
}

interface BrowserToolOutput {
  readonly success: boolean;
  readonly url: string;
  readonly title: string;
  readonly content?: string;     // snapshot or extracted text
  readonly data?: unknown;       // structured extraction result
  readonly screenshot?: string;  // base64 PNG
  readonly error?: string;
}
```

### Permission

```typescript
interface BrowserHostPermissionPolicyOptions {
  readonly allowedHosts?: readonly string[]; // static config allowlist
  readonly sensitivePatterns?: readonly string[]; // always-ask patterns
}

class BrowserHostPermissionPolicy implements PermissionPolicy {
  readonly name = 'browser-host';
  constructor(options: BrowserHostPermissionPolicyOptions);
  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined;
}
```

## Call-Sites (Verified in Code)

| Call Site | File | Lines | Purpose |
|---|---|---|---|
| Built-in tool registration | `packages/agent-core/src/agent/tool/index.ts:388-462` | `initializeBuiltinTools()` registers `ReadTool`, `BashTool`, etc. New browser tools will be added here. |
| MCP server registration | `packages/agent-core/src/agent/tool/index.ts:138-211` | `registerMcpServer()` currently wraps `chrome-devtools` MCP tools. Will be bypassed when native browser tools are active. |
| Built-in MCP registry | `packages/agent-core/src/mcp/built-in/registry.ts:44-51` | `isDisabled('chrome-devtools')` checks `config.browser?.enabled === false`. Will be extended to also respect `config.browser.legacyMcpEnabled`. |
| Chrome DevTools MCP definition | `packages/agent-core/src/mcp/built-in/chrome-devtools.ts:1-33` | Current `createChromeDevToolsServerDefinition`. Will be deprecated but kept. |
| Permission evaluation | `packages/agent-core/src/agent/permission/index.ts:96-114` | `beforeToolCall()` → `evaluatePolicies()`. New `BrowserHostPermissionPolicy` will be added to `createPermissionDecisionPolicies()`. |
| Session approval caching | `packages/agent-core/src/agent/permission/policies/session-approval-history.ts:12-45` | `SessionApprovalHistoryPermissionPolicy` checks `sessionApprovalRulePatterns`. New browser policy will generate patterns like `Browser*(host)`. Browser tools must implement `execution.matchesRule` to match the host argPattern. |
| Permission rule matching | `packages/agent-core/src/agent/permission/matches-rule.ts` | To be reused for `Browser(host)` pattern matching. |
| Current browser tool policy | `packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts:7-16` | `BrowserToolAskPermissionPolicy` matches `mcp__chrome-devtools__*`. Will be replaced / extended. |
| Config schema | `packages/agent-core/src/config/schema.ts` | [C:INFERRED] Assumed location of `KimiConfig` browser field definition. Needs verification before implementation. |

## Algorithms

### Algorithm: `resolveOrLaunchBrowser()`

```text
INPUT: options (chromePort?, autoLaunch?, headless?, userDataDir?, extensionBridge?)
OUTPUT: BrowserHandle

1. If active handle exists and not closed:
     RETURN active handle

2. PRIMARY: Try connect to existing Chrome
   a. If chromePort provided:
        endpoint ← read DevToolsActivePort from user-data-dir
        OR endpoint ← http://127.0.0.1:chromePort/json/version
   b. Else:
        Try puppeteer.connect({ channel: 'chrome' })   // auto-detect
   c. If connect succeeds:
        handle ← CREATE BrowserHandle(kind='connected', browser)
        active ← handle
        RETURN handle

3. FALLBACK: If autoLaunch is true:
   a. browser ← puppeteer.launch({ headless, userDataDir })
   b. handle ← CREATE BrowserHandle(kind='launched', browser)
   c. active ← handle
   d. ON process exit OR agent shutdown:
        CALL handle.close()
   e. RETURN handle

4. If extensionBridge.enabled:
   a. Try WebSocket connect to extension bridge
   b. If succeeds:
        handle ← CREATE BrowserHandle(kind='extension', bridge)
        active ← handle
        RETURN handle

5. THROW BrowserConnectionError("No browser available")
```

### Algorithm: `BrowserHostPermissionPolicy.evaluate()`

```text
INPUT: context (toolCall, args, execution)
OUTPUT: PermissionPolicyResult | undefined

1. If toolCall.name does not start with 'Browser' → RETURN undefined

2. urlString ← EXTRACT url from args (tool-specific key: url / actionUrl / currentUrl)
   If no url present → RETURN undefined (let other policies decide)

3. host ← PARSE(urlString).host
   If parse fails → RETURN { kind: 'ask', reason: { invalid_url: urlString } }

4. If host matches sensitivePatterns →
     RETURN { kind: 'ask', reason: { host, sensitive: true } }

5. If host matches allowedHosts (static allowlist) →
     RETURN { kind: 'approve', reason: { host, allowlist: true } }

6. pattern ← "Browser*(" + host + ")"
   If pattern in sessionApprovalRulePatterns →
     RETURN { kind: 'approve', reason: { host, session_rule: pattern } }
   # Note: Browser tools MUST implement execution.matchesRule(pattern) so that
   # SessionApprovalHistoryPermissionPolicy can match the host argPattern.

7. RETURN { kind: 'ask', reason: { host } }
```

### Algorithm: `BrowserBrowse.execute()`

```text
INPUT: args (url, goal?, waitFor?, extract?)
OUTPUT: BrowserToolOutput

1. Validate URL; if invalid RETURN error
   Set context.execution.approvalRule = 'Browser*(' + parsedUrl.host + ')'
   Set context.execution.matchesRule = (argPattern) => argPattern === parsedUrl.host

2. permissionResult ← await permission.beforeToolCall(context)
   If blocked → RETURN error from permissionResult

3. handle ← await connection.resolveOrLaunchBrowser()

4. page ← handle.acquirePage()

5. TRY:
     a. await page.goto(url, { waitUntil: 'networkidle2', timeout: 30000 })
     b. If waitFor is number:
          await sleep(waitFor)
        Else if waitFor is string:
          await page.waitForSelector(waitFor, { timeout: 10000 })

     c. title ← await page.title()

     d. If extract schema provided:
          content ← await page.evaluate(extractScript, extract)
          result.data ← content
        Else:
          content ← await page.evaluate(() => document.body.innerText)
          result.content ← truncate(content, 8000)

     e. screenshot ← await page.screenshot({ encoding: 'base64', fullPage: false })

6. CATCH error:
     result.success ← false
     result.error ← formatError(error)

7. FINALLY:
     handle.releasePage(page)

8. RETURN result
```

## Error / Degradation Table

| Scenario | Detection | Degradation | User-Facing Message |
|---|---|---|---|
| No Chrome running, autoLaunch=true | `puppeteer.connect()` throws | Launch new Chromium | "未检测到运行中的 Chrome，已启动新的浏览器实例。注意：新实例不保留您的登录态。" |
| No Chrome running, autoLaunch=false | `puppeteer.connect()` throws, launch disabled | Block tool call | "未检测到 Chrome，且配置禁止自动启动。请启动 Chrome 并启用远程调试，或开启 browser.autoLaunch。" |
| Chrome 连接成功但页面导航超时 | `page.goto()` timeout | Return partial snapshot + error | "页面加载超时，已返回当前视图快照。" |
| URL parse 失败 | `new URL()` throws | Ask user | "无法识别目标地址，请提供有效 URL。" |
| Permission denied | user rejects approval | Block tool call | "用户未授权访问该网站。" |
| Extension bridge 配置但未启动 | WebSocket connect ECONNREFUSED | Fallback to CDP connect/launch | "扩展桥接未就绪，已回退到 CDP 模式。" |
| Puppeteer launch 在 CI/无头环境失败 | executable not found | Suggest headless config + optional chromium download | "无法启动 Chromium，请检查 Puppeteer 依赖或配置 browser.executablePath。" |

## Test Assertions

1. **Connection Manager**
   - When Chrome is running, `resolveOrLaunchBrowser()` returns `kind='connected'` handle.
   - When Chrome is not running and `autoLaunch=true`, it falls back to `kind='launched'`.
   - When Chrome is not running and `autoLaunch=false`, it throws `BrowserConnectionError`.
   - Calling `resolveOrLaunchBrowser()` twice returns the same active handle (singleton per manager).

2. **BrowserHostPermissionPolicy**
   - For tool `BrowserBrowse` with URL `https://kimi.com/code/console`, if `sessionApprovalRulePatterns` contains `Browser*(kimi.com)` AND the tool's `matchesRule('kimi.com')` returns true, returns `approve`.
   - For unknown host `https://evil.test/path`, returns `ask`.
   - For host in static `allowedHosts`, returns `approve`.
   - For non-browser tool `Read`, returns `undefined`.

3. **BrowserBrowse Tool**
   - Successfully navigates to `https://example.com`, returns `success=true`, non-empty `title` and `content`.
   - With invalid URL, returns `success=false` and `error` field.
   - With `extract` schema, returns `data` matching schema shape.

4. **Registry Migration**
   - When `config.browser.legacyMcpEnabled === true`, `chrome-devtools` MCP server is still registered.
   - When default config, `chrome-devtools` MCP server is disabled and native browser tools are registered instead.

## Risk Register

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Puppeteer 依赖体积大，影响安装包大小 | 高 | 中 | 将 puppeteer 列为 `optionalDependencies` 或 peer dep；如果未安装则优雅降级并提示用户。 |
| R2 | macOS 上 `puppeteer.connect({ channel: 'chrome' })` 行为不稳定 | 中 | 高 | 保留 `--chrome-port` 显式端口回退；增加重试与错误日志；长期切换到扩展桥接。 |
| R3 | 自动启动的 Chromium 无登录态，用户困惑 | 高 | 中 | 在结果中显式标注 `handle.kind='launched'`；TUI 中显示连接状态与登录态提示。 |
| R4 | 按 host 授权过于粗放，子域名绕过 | 中 | 高 | 精确匹配 host，不支持通配符；后续可升级为 eTLD+1 匹配（如 `*.kimi.com` 需要显式配置）。 |
| R5 | 大页面快照/截图 token 占用过高 | 高 | 中 | 默认截断文本至 8k chars；截图可选、默认全屏=false；提供 `extract` schema 让模型只取所需数据。 |
| R6 | 与现有 `chrome-devtools` MCP 工具名冲突 | 中 | 高 | 原生工具使用 `Browser*` 前缀（如 `BrowserBrowse`），与 `mcp__chrome-devtools__*` 不冲突；但需要在 profile 中显式切换启用。 |
| R7 | Chrome Extension 桥接的 WebSocket 端口被防火墙/安全软件拦截 | 低 | 中 | 使用随机可用端口；扩展通过 native messaging 回退；提供明确诊断日志。 |
| R8 | 权限策略被 yolo/auto 模式绕过 | 中 | 高 | Deny 规则始终优先；browser policy 在 policy chain 中位于 yolo-approve 之前。 |

## Self-Review

### Highest-Stakes Decisions Under Scrutiny

1. **Permission pattern `Browser*(<host>)` + `matchesRule` integration**
   - Input A (expected approve): `toolName='BrowserBrowse'`, `pattern='Browser*(kimi.com)'`, `matchesRule('kimi.com') → true` → `SessionApprovalHistoryPermissionPolicy` approves. **Verified**: `picomatch.isMatch('BrowserBrowse', 'Browser*') === true` via ephemeral `node -e`.
   - Input B (expected reject): `toolName='BrowserBrowse'`, `pattern='Browser(kimi.com)'` (no star) → `picomatch.isMatch('BrowserBrowse', 'Browser') === false` → no approval. **This was the bug caught during self-review; design fixed to use `Browser*(`.
   - Input C (adversarial): `toolName='BrowserTool'`, `pattern='Browser*(kimi.com)'`, `matchesRule('evil.kimi.com')` → must return false because exact host match is required. Correct.

2. **`puppeteer.connect({ channel: 'chrome' })` as primary strategy on macOS**
   - Input A: Chrome running with standard profile on macOS → should auto-discover `DevToolsActivePort` and connect.
   - Input B: Chrome running with `--remote-debugging-port=9222` → should connect via port fallback.
   - Input C: No Chrome running, `autoLaunch=true` → should fall back to `puppeteer.launch()`. Verified by algorithm.

3. **URL host extraction from tool arguments**
   - Input A: `args.url = 'https://kimi.com/code/console'` → host = `kimi.com`. Correct.
   - Input B: `args.url = 'http://localhost:3000/path'` → host = `localhost:3000`. Design treats this as sensitive (no explicit allowlist) → ask. This may be overly strict; implementer can add `localhost`/`127.0.0.1` to default `allowedHosts` if desired.
   - Input C: `args.url = 'javascript:alert(1)'` → `new URL()` throws → ask with `invalid_url`. Correct.

### Four-Lens Sweep

- **Security**: Checked the host-matching logic. Initial draft used `Browser(host)` which would NOT match `BrowserBrowse` (verified with picomatch). Fixed to `Browser*(host)` and added mandatory `matchesRule` on browser tool executions. Also verified that `SessionApprovalHistoryPermissionPolicy` requires both tool-name glob match AND `matchesRule` truthy result, preventing blanket approval of all browser tools.
- **Test**: Every behaviour now has both a must-pass and a must-reject assertion. Added explicit test that `Browser*(kimi.com)` pattern is required and that `matchesRule` must return true. No contradictions found after the pattern fix.
- **Ops**: Puppeteer dependency size flagged as R1; mitigation is `optionalDependencies` or peer dep. Concurrency handled by single active handle + page acquire/release. No identifier collision risk (`Browser*` prefix is distinct from `mcp__chrome-devtools__*`).
- **Integration**: Verified config schema exists at `packages/agent-core/src/config/schema.ts:186-193`; verified policy chain at `packages/agent-core/src/agent/permission/policies/index.ts:27-65`; verified tool registration hook at `packages/agent-core/src/agent/tool/index.ts:388-462`. **Discovered**: `Agent` class has no `close()`/`destroy()` lifecycle (verified by reading `packages/agent-core/src/agent/index.ts` end-to-end). Documented as A9 — cleanup must use process exit hooks or add a new Agent method.
- **Scope**: This remains ONE coherent design (browser control replacement) with well-deferred Phase 2 extension work. No decomposition needed.

## Assumptions

| ID | Assumption | Confidence | Tag |
|---|---|---|---|
| A1 | `packages/agent-core` 允许新增 builtin tool 并注册到 `ToolManager.initializeBuiltinTools()` 中。 | 已验证 | [C:USER] |
| A2 | `PermissionManager` 的 policy chain 可以新增 `BrowserHostPermissionPolicy`，且 `sessionApprovalRulePatterns` 机制可复用。 | 已验证 | [C:USER] |
| A3 | Puppeteer 的 `puppeteer.connect({ channel: 'chrome' })` 在 macOS / Linux / Windows 上都能自动发现用户正在运行的 Chrome 实例。 | 已接受（Deep audit gate, 2026-06-08） | [C:INFERRED] |
| A4 | 用户目标场景以"查看已登录网站数据"为主，因此复用登录态的优先级高于隔离性。 | 用户确认 | [C:USER] |
| A5 | 项目可接受将 `puppeteer` 作为 `packages/agent-core` 的依赖（或 peer/optional）。 | 已接受（Deep audit gate, 2026-06-08） | [C:INFERRED] |
| A6 | `KimiConfig` schema 可以扩展 `browser` 字段，且 TUI / CLI 配置系统会透传新字段。 | 已验证：`BrowserConfigSchema` 在 `packages/agent-core/src/config/schema.ts:186-193`，可扩展。 | [C:INFERRED] |
| A7 | Chrome Extension 桥接作为 Phase 2 实现，本设计阶段只定义接口与回退策略。 | 设计决策 | [C:DEFERRED] |
| A8 | 当前 `chrome-devtools` MCP server 可以完全禁用而不影响其他 agent-core 功能。 | 已验证（通过 `config.browser.enabled`） | [C:USER] |
| A9 | `Agent` 类目前没有显式的 `close()` / `destroy()` 生命周期方法；`BrowserConnectionManager` 的资源清理需要在实现时添加合适的 hook（如 process exit 或新增 Agent 方法）。 | 已接受（Deep audit gate, 2026-06-08） | [C:INFERRED] |

## Next Steps After Approval

1. Implement `BrowserConnectionManager` in `packages/agent-core/src/browser/connection.ts`.
2. Implement browser tool suite in `packages/agent-core/src/tools/builtin/browser-*.ts`.
3. Implement `BrowserHostPermissionPolicy` in `packages/agent-core/src/agent/permission/policies/browser-host.ts`.
4. Extend `KimiConfig` schema and default profile templates.
5. Update `ToolManager.initializeBuiltinTools()` to register native browser tools when `browser.enabled !== false`.
6. Update `BuiltInMcpRegistry.isDisabled()` to disable `chrome-devtools` by default unless `legacyMcpEnabled === true`.
7. Add unit tests for connection manager, tools, and permission policy.
8. Write changeset via `gen-changesets` skill.
