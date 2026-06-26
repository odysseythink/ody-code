# Part: Agent-Core Browser Tools

## Purpose

定义暴露给大模型的浏览器工具族。采用"高阶意图工具为主、原子工具为 fallback"的分层设计，减少模型推理回合，同时保留可调试性。

## Tool Taxonomy

### High-Level Intents (暴露给模型)

| Tool | Purpose | When to Use |
|---|---|---|
| `BrowserBrowse` | 导航到 URL，等待加载，返回页面文本/截图 | "打开 example.com 并告诉我内容" |
| `BrowserExtract` | 按 schema 从当前页面或指定 URL 提取结构化数据 | "提取价格列表" / "获取账户余额" |
| `BrowserAct` | 在已打开页面执行自然语言描述的操作 | "点击登录按钮并输入用户名" |

### Atomic Fallbacks (不默认暴露，保留给 debug / advanced profiles)

| Tool | Purpose |
|---|---|
| `BrowserNavigate` | 仅导航 |
| `BrowserSnapshot` | 返回可交互元素快照 |
| `BrowserClick` | 点击元素（selector or coordinates） |
| `BrowserFill` | 填写输入框 |
| `BrowserEvaluate` | 在当前页面执行 JavaScript |
| `BrowserScreenshot` | 截图 |

## Data Flow

```
Model ──► BrowserBrowse(args)
              │
              ▼
        BrowserBrowseTool.execute(args)
              │
              ├──► PermissionManager.beforeToolCall()  (URL host check)
              ├──► BrowserConnectionManager.resolveOrLaunchBrowser()
              ├──► Page.goto(url)
              ├──► waitForLoad / waitForSelector
              ├──► page.evaluate(extraction)
              ├──► page.screenshot()
              ▼
        Return ToolOutput { success, url, title, content, data, screenshot }
```

## Typed Interfaces

```typescript
// packages/agent-core/src/tools/builtin/browser-types.ts

export interface BrowserBrowseInput {
  url: string;
  goal?: string;                 // passed to extraction heuristic
  waitFor?: string | number;     // selector or milliseconds
  extract?: Record<string, string>; // CSS selector map
  takeScreenshot?: boolean;      // default false to save tokens
}

export interface BrowserExtractInput {
  url?: string;
  schema: Record<string, string>; // key -> CSS selector
  attribute?: 'text' | 'value' | 'href' | 'src' | string;
}

export interface BrowserActInput {
  instruction: string;
  url?: string;
  selectors?: string[];          // optional hints
  maxSteps?: number;             // default 5
}

export interface BrowserToolOutput {
  readonly success: boolean;
  readonly url: string;
  readonly title: string;
  readonly content?: string;     // truncated plain text
  readonly data?: unknown;       // structured extraction
  readonly screenshot?: string;  // base64 PNG
  readonly error?: string;
  readonly handleKind?: 'connected' | 'launched' | 'extension';
}

// Atomic tools
export interface BrowserNavigateInput { url: string; waitUntil?: string; }
export interface BrowserSnapshotInput { fullPage?: boolean; }
export interface BrowserClickInput { selector: string; }
export interface BrowserFillInput { selector: string; value: string; }
export interface BrowserEvaluateInput { script: string; args?: unknown[]; }
export interface BrowserScreenshotInput { fullPage?: boolean; selector?: string; }
```

## Algorithm: `BrowserBrowseTool.execute()`

```text
INPUT: args (url, goal?, waitFor?, extract?, takeScreenshot?)
OUTPUT: BrowserToolOutput

1. Validate args.url is valid URL; if not, RETURN { success: false, error: 'invalid url' }
   parsedUrl ← new URL(args.url)
   Set context.execution.approvalRule = 'Browser*(' + parsedUrl.host + ')'
   Set context.execution.matchesRule = (argPattern) => argPattern === parsedUrl.host

2. permission ← await context.permission.beforeToolCall(context)
   If permission.blocked:
     RETURN { success: false, error: permission.reason }

3. handle ← await context.connection.resolveOrLaunchBrowser()
   page ← await handle.acquirePage()

4. TRY:
     a. response ← await page.goto(args.url, { waitUntil: 'networkidle2', timeout: 30000 })
     b. If response is null or response.ok() === false and status >= 400:
          log warning; continue anyway (some SPAs return 200 later)

     c. If args.waitFor is number:
          sleep(args.waitFor)
        Else if args.waitFor is string:
          await page.waitForSelector(args.waitFor, { timeout: 10000, visible: true })

     d. title ← await page.title()

     e. If args.extract exists:
          data ← await page.evaluate(extractBySchema, args.extract)
          output.data ← data
        Else:
          rawText ← await page.evaluate(() => document.body.innerText)
          output.content ← truncate(rawText, 8000)

     f. If args.takeScreenshot:
          output.screenshot ← await page.screenshot({ encoding: 'base64', fullPage: false })

     g. output ← {
          success: true,
          url: page.url(),
          title,
          content: output.content,
          data: output.data,
          screenshot: output.screenshot,
          handleKind: handle.kind,
        }

5. CATCH error:
     output ← { success: false, url: args.url, title: '', error: formatError(error), handleKind: handle.kind }

6. FINALLY:
     handle.releasePage(page)

7. RETURN output
```

## Algorithm: `extractBySchema(schema)`

```text
// Runs inside page.evaluate
INPUT: schema: Record<string, string>   // name -> CSS selector
OUTPUT: Record<string, unknown>

result ← {}
FOR EACH (name, selector) IN schema:
  elements ← document.querySelectorAll(selector)
  IF elements.length === 0:
    result[name] ← null
  ELSE IF elements.length === 1:
    result[name] ← elements[0].innerText.trim()
  ELSE:
    result[name] ← Array.from(elements).map(el => el.innerText.trim())
RETURN result
```

## Algorithm: `BrowserActTool.execute()`

```text
INPUT: args (instruction, url?, selectors?, maxSteps?)
OUTPUT: BrowserToolOutput

1. If args.url provided:
     Run BrowserBrowseTool.execute({ url: args.url })

2. handle ← await context.connection.resolveOrLaunchBrowser()
   page ← await handle.acquirePage()

3. For step from 1 to args.maxSteps ?? 5:
     a. snapshot ← await buildAccessibleSnapshot(page)
     b. decision ← await modelSubCall('decide_next_action', { instruction, snapshot, selectors })
        // Returns one of: click(selector), fill(selector, value), scroll, wait, done(summary), error
     c. Execute action on page via Puppeteer
     d. If decision is done or error → break

4. finalContent ← await page.evaluate(() => document.body.innerText)
5. RETURN { success: true, url: page.url(), title: await page.title(), content: truncate(finalContent, 8000) }
```

## Call-Sites

| Location | File | Lines | Action |
|---|---|---|---|
| Tool registration | `packages/agent-core/src/agent/tool/index.ts:388-462` | Add `new BrowserBrowseTool(conn)`, `new BrowserExtractTool(conn)`, `new BrowserActTool(conn)` to builtinTools list. |
| Atomic registration | `packages/agent-core/src/agent/tool/index.ts:388-462` | Atomic tools registered only if profile enables them explicitly (e.g. `BrowserSnapshot` hidden by default). |
| Tool naming | `packages/agent-core/src/tools/builtin/*.ts` | Follow existing convention: class `BrowserBrowseTool` → name `BrowserBrowse`. |

## Error / Degradation

| Scenario | Handling |
|---|---|
| Model generates invalid selector in `BrowserAct` | Catch `page.waitForSelector` timeout, return snapshot + error, allow retry. |
| Page JS error during `BrowserEvaluate` | Return error message + stack if available. |
| `extractBySchema` returns all nulls | Still return success=true with empty data; model can decide next step. |
| Screenshot too large for context window | Default `fullPage=false`; if still large, compress or omit. |

## Test Assertions

1. `BrowserBrowse` returns success with title and content for `https://example.com`.
2. `BrowserExtract` correctly extracts multiple elements by CSS selector into arrays.
3. `BrowserBrowse` with invalid URL returns success=false and clear error.
4. `BrowserAct` completes in ≤ maxSteps when simple action sequence is requested.
5. Screenshot is only returned when `takeScreenshot=true`.
