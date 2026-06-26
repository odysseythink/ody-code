# Phase B: Browser Tools — 原子工具 + 高阶工具 + 注册

---

### Task 6: Implement atomic browser tools

**Depends on:** `foundation.md` Task 5

**Files:**
- Create: `packages/agent-core/src/tools/builtin/browser/_utils.ts`
- Create: `packages/agent-core/src/tools/builtin/browser/navigate.ts`
- Create: `packages/agent-core/src/tools/builtin/browser/snapshot.ts`
- Create: `packages/agent-core/src/tools/builtin/browser/click.ts`
- Create: `packages/agent-core/src/tools/builtin/browser/fill.ts`
- Create: `packages/agent-core/src/tools/builtin/browser/evaluate.ts`
- Create: `packages/agent-core/src/tools/builtin/browser/screenshot.ts`

**Steps:**

- [ ] Create `packages/agent-core/src/tools/builtin/browser/_utils.ts`:

```typescript
const GLOB_LITERAL_SPECIAL = /[\\*?[\]{}()!+@|]/g;

export function browserHostApprovalRule(host: string): string {
  return `Browser*(${host.replace(GLOB_LITERAL_SPECIAL, '\\$&')})`;
}

export function matchesBrowserHostRule(ruleArgs: string, host: string): boolean {
  return ruleArgs === host;
}

export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength) + '\n[...truncated]';
}
```

- [ ] Create `packages/agent-core/src/tools/builtin/browser/navigate.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { BrowserNavigateInputSchema, type BrowserNavigateInput, type BrowserConnectionManager } from '../../../../browser';
import { browserHostApprovalRule, matchesBrowserHostRule } from './_utils';

export class BrowserNavigateTool implements BuiltinTool<BrowserNavigateInput> {
  readonly name = 'BrowserNavigate' as const;
  readonly description = 'Navigate the browser to a specific URL.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserNavigateInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserNavigateInput): ToolExecution {
    const host = new URL(args.url).host;
    return {
      accesses: ToolAccesses.none(),
      description: `Navigating to ${args.url}`,
      approvalRule: browserHostApprovalRule(host),
      matchesRule: (ruleArgs) => matchesBrowserHostRule(ruleArgs, host),
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserNavigateInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.goto(args.url, { waitUntil: 'networkidle2', timeout: 30000 });
      return { output: `Navigated to ${args.url}`, isError: false };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to navigate: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Create `packages/agent-core/src/tools/builtin/browser/snapshot.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { ToolResultBuilder } from '../../../support/result-builder';
import { BrowserSnapshotInputSchema, type BrowserSnapshotInput, type BrowserConnectionManager } from '../../../../browser';

export class BrowserSnapshotTool implements BuiltinTool<BrowserSnapshotInput> {
  readonly name = 'BrowserSnapshot' as const;
  readonly description = 'Get the text content of the current browser page, optionally scoped to a CSS selector.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserSnapshotInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserSnapshotInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: args.selector ? `Snapshot of ${args.selector}` : 'Snapshot of current page',
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserSnapshotInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      let text: string;
      if (args.selector) {
        const element = await page.$(args.selector);
        if (!element) {
          return { output: `Element not found: ${args.selector}`, isError: true };
        }
        text = await element.evaluate((el) => el.textContent ?? '');
      } else {
        text = await page.evaluate(() => document.body.innerText);
      }
      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(text);
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to get snapshot: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Create `packages/agent-core/src/tools/builtin/browser/click.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { BrowserClickInputSchema, type BrowserClickInput, type BrowserConnectionManager } from '../../../../browser';

export class BrowserClickTool implements BuiltinTool<BrowserClickInput> {
  readonly name = 'BrowserClick' as const;
  readonly description = 'Click an element on the current page by CSS selector.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserClickInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserClickInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: `Click ${args.selector}`,
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserClickInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.click(args.selector);
      return { output: `Clicked ${args.selector}`, isError: false };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to click: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Create `packages/agent-core/src/tools/builtin/browser/fill.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { BrowserFillInputSchema, type BrowserFillInput, type BrowserConnectionManager } from '../../../../browser';

export class BrowserFillTool implements BuiltinTool<BrowserFillInput> {
  readonly name = 'BrowserFill' as const;
  readonly description = 'Fill an input element on the current page by CSS selector.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserFillInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserFillInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: `Fill ${args.selector}`,
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserFillInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.type(args.selector, args.value);
      return { output: `Filled ${args.selector}`, isError: false };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Failed to fill: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Create `packages/agent-core/src/tools/builtin/browser/evaluate.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { ToolResultBuilder } from '../../../support/result-builder';
import { BrowserEvaluateInputSchema, type BrowserEvaluateInput, type BrowserConnectionManager } from '../../../../browser';

export class BrowserEvaluateTool implements BuiltinTool<BrowserEvaluateInput> {
  readonly name = 'BrowserEvaluate' as const;
  readonly description = 'Evaluate a JavaScript snippet in the context of the current page.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserEvaluateInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserEvaluateInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: 'Evaluate JavaScript in page',
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserEvaluateInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      const result = await page.evaluate((script: string) => {
        // eslint-disable-next-line no-eval
        return eval(script);
      }, args.script);
      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(typeof result === 'string' ? result : JSON.stringify(result, null, 2));
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Evaluation failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Create `packages/agent-core/src/tools/builtin/browser/screenshot.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { BrowserScreenshotInputSchema, type BrowserScreenshotInput, type BrowserConnectionManager } from '../../../../browser';

export class BrowserScreenshotTool implements BuiltinTool<BrowserScreenshotInput> {
  readonly name = 'BrowserScreenshot' as const;
  readonly description = 'Take a screenshot of the current page.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserScreenshotInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserScreenshotInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: args.fullPage ? 'Full-page screenshot' : 'Viewport screenshot',
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserScreenshotInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      const screenshot = await page.screenshot({
        encoding: 'base64',
        fullPage: args.fullPage ?? false,
      });
      return {
        output: `Screenshot (base64):\n${screenshot}`,
        isError: false,
      };
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Screenshot failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Run typecheck for `packages/agent-core`:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes (all new files compile, no missing imports).

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/browser/ && git commit -m "feat(browser): add atomic browser tools (navigate, snapshot, click, fill, evaluate, screenshot)"
```

---

### Task 7: Implement `BrowserBrowseTool`

**Depends on:** Task 6

**Files:**
- Create: `packages/agent-core/src/tools/builtin/browser/browse.ts`

**Steps:**

- [ ] Create `packages/agent-core/src/tools/builtin/browser/browse.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { ToolResultBuilder } from '../../../support/result-builder';
import {
  BrowserBrowseInputSchema,
  type BrowserBrowseInput,
  type BrowserConnectionManager,
} from '../../../../browser';
import { browserHostApprovalRule, matchesBrowserHostRule, truncateText } from './_utils';

export class BrowserBrowseTool implements BuiltinTool<BrowserBrowseInput> {
  readonly name = 'BrowserBrowse' as const;
  readonly description =
    'Navigate to a URL, wait for the page to load, and return the page title, URL, ' +
    'text content (truncated to 8000 chars), and a viewport screenshot.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserBrowseInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserBrowseInput): ToolExecution {
    const host = new URL(args.url).host;
    return {
      accesses: ToolAccesses.none(),
      description: `Browsing ${args.url}`,
      approvalRule: browserHostApprovalRule(host),
      matchesRule: (ruleArgs) => matchesBrowserHostRule(ruleArgs, host),
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserBrowseInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      await page.goto(args.url, { waitUntil: 'networkidle2', timeout: 30000 });

      if (args.waitFor !== undefined) {
        if (typeof args.waitFor === 'number') {
          await new Promise((resolve) => setTimeout(resolve, args.waitFor as number));
        } else {
          await page.waitForSelector(args.waitFor, { timeout: 10000 });
        }
      }

      const title = await page.title();
      const url = page.url();

      let extractedData: unknown | undefined;
      let content: string | undefined;

      if (args.extract) {
        extractedData = await page.evaluate((schema: Record<string, string>) => {
          const result: Record<string, string> = {};
          for (const [key, selector] of Object.entries(schema)) {
            const el = document.querySelector(selector);
            result[key] = el ? (el.textContent ?? '') : '';
          }
          return result;
        }, args.extract);
      } else {
        const rawText = await page.evaluate(() => document.body.innerText);
        content = truncateText(rawText, 8000);
      }

      const screenshot = await page.screenshot({
        encoding: 'base64',
        fullPage: false,
      });

      const result: {
        success: true;
        url: string;
        title: string;
        content?: string;
        data?: unknown;
        screenshot?: string;
      } = {
        success: true,
        url,
        title,
      };

      if (content !== undefined) result.content = content;
      if (extractedData !== undefined) result.data = extractedData;
      if (typeof screenshot === 'string') result.screenshot = screenshot;

      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(JSON.stringify(result, null, 2));
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(
        JSON.stringify({ success: false, url: args.url, title: '', error: msg }, null, 2),
      );
      return builder.error('Browser browse failed');
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Run typecheck:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes.

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/browser/browse.ts && git commit -m "feat(browser): add BrowserBrowse high-level tool"
```

---

### Task 8: Implement `BrowserExtractTool` and `BrowserActTool`

**Depends on:** Task 7

**Files:**
- Create: `packages/agent-core/src/tools/builtin/browser/extract.ts`
- Create: `packages/agent-core/src/tools/builtin/browser/act.ts`

**Steps:**

- [ ] Create `packages/agent-core/src/tools/builtin/browser/extract.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import { ToolResultBuilder } from '../../../support/result-builder';
import {
  BrowserExtractInputSchema,
  type BrowserExtractInput,
  type BrowserConnectionManager,
} from '../../../../browser';
import { browserHostApprovalRule, matchesBrowserHostRule } from './_utils';

export class BrowserExtractTool implements BuiltinTool<BrowserExtractInput> {
  readonly name = 'BrowserExtract' as const;
  readonly description =
    'Extract structured data from the current page or a specified URL using CSS selectors.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserExtractInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserExtractInput): ToolExecution {
    const host = args.url ? new URL(args.url).host : undefined;
    return {
      accesses: ToolAccesses.none(),
      description: host ? `Extract from ${args.url}` : 'Extract from current page',
      approvalRule: host ? browserHostApprovalRule(host) : this.name,
      matchesRule: host ? (ruleArgs) => matchesBrowserHostRule(ruleArgs, host) : undefined,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserExtractInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      if (args.url) {
        await page.goto(args.url, { waitUntil: 'networkidle2', timeout: 30000 });
      }

      const data = await page.evaluate((schema: Record<string, string>) => {
        const result: Record<string, string> = {};
        for (const [key, selector] of Object.entries(schema)) {
          const el = document.querySelector(selector);
          result[key] = el ? (el.textContent ?? '') : '';
        }
        return result;
      }, args.schema);

      const url = page.url();
      const title = await page.title();

      const builder = new ToolResultBuilder({ maxChars: 8000 });
      builder.write(JSON.stringify({ success: true, url, title, data }, null, 2));
      return builder.ok();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Extraction failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Create `packages/agent-core/src/tools/builtin/browser/act.ts`:

```typescript
import type { BuiltinTool } from '../../../../agent/tool';
import { ToolAccesses } from '../../../../loop/tool-access';
import type { ExecutableToolContext, ExecutableToolResult, ToolExecution } from '../../../../loop/types';
import { toInputJsonSchema } from '../../../support/input-schema';
import {
  BrowserActInputSchema,
  type BrowserActInput,
  type BrowserConnectionManager,
} from '../../../../browser';

export class BrowserActTool implements BuiltinTool<BrowserActInput> {
  readonly name = 'BrowserAct' as const;
  readonly description =
    'Perform an action on the current page: click, type, scroll, screenshot, or wait.';
  readonly parameters: Record<string, unknown> = toInputJsonSchema(BrowserActInputSchema);

  constructor(private readonly connection: BrowserConnectionManager) {}

  resolveExecution(args: BrowserActInput): ToolExecution {
    return {
      accesses: ToolAccesses.none(),
      description: `Browser act: ${args.action}`,
      approvalRule: this.name,
      execute: () => this.execution(args),
    };
  }

  private async execution(args: BrowserActInput): Promise<ExecutableToolResult> {
    const handle = await this.connection.resolveOrLaunchBrowser();
    const page = await handle.acquirePage();
    try {
      switch (args.action) {
        case 'click': {
          if (!args.selector) {
            return { output: 'Click action requires a selector', isError: true };
          }
          await page.click(args.selector);
          return { output: `Clicked ${args.selector}`, isError: false };
        }
        case 'type': {
          if (!args.selector || args.value === undefined) {
            return { output: 'Type action requires selector and value', isError: true };
          }
          await page.type(args.selector, args.value);
          return { output: `Typed into ${args.selector}`, isError: false };
        }
        case 'scroll_down': {
          await page.evaluate(() => window.scrollBy(0, window.innerHeight));
          return { output: 'Scrolled down', isError: false };
        }
        case 'scroll_up': {
          await page.evaluate(() => window.scrollBy(0, -window.innerHeight));
          return { output: 'Scrolled up', isError: false };
        }
        case 'screenshot': {
          const screenshot = await page.screenshot({ encoding: 'base64', fullPage: false });
          return {
            output: `Screenshot (base64):\n${screenshot}`,
            isError: false,
          };
        }
        case 'wait': {
          if (args.selector) {
            await page.waitForSelector(args.selector, { timeout: 10000 });
            return { output: `Waited for ${args.selector}`, isError: false };
          }
          return { output: 'Wait action requires a selector or timeout value', isError: true };
        }
        default:
          return { output: `Unknown action: ${args.action}`, isError: true };
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      return { output: `Action failed: ${msg}`, isError: true };
    } finally {
      handle.releasePage(page);
    }
  }
}
```

- [ ] Run typecheck:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes.

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/browser/extract.ts packages/agent-core/src/tools/builtin/browser/act.ts && git commit -m "feat(browser): add BrowserExtract and BrowserAct high-level tools"
```

---

### Task 9: Export and register browser tools

**Depends on:** Task 8

**Files:**
- Create: `packages/agent-core/src/tools/builtin/browser/index.ts`
- Modify: `packages/agent-core/src/tools/builtin/index.ts`
- Modify: `packages/agent-core/src/agent/index.ts` (add `browserConnection` field)
- Modify: `packages/agent-core/src/agent/tool/index.ts` (`initializeBuiltinTools`)

**Steps:**

- [ ] Create `packages/agent-core/src/tools/builtin/browser/index.ts`:

```typescript
export * from './browse';
export * from './extract';
export * from './act';
export * from './navigate';
export * from './snapshot';
export * from './click';
export * from './fill';
export * from './evaluate';
export * from './screenshot';
```

- [ ] Modify `packages/agent-core/src/tools/builtin/index.ts` — add at the end:

```typescript
export * from './browser';
```

- [ ] Modify `packages/agent-core/src/agent/index.ts`:
  1. Add import for `BrowserConnectionManager` near the top (after existing imports, around line 60):

```typescript
import { BrowserConnectionManager } from '../browser';
```

2. Add the field declaration after `readonly cron: CronManager | null;` (around line 130):

```typescript
readonly browserConnection?: BrowserConnectionManager;
```

3. Add initialization before `this.tools = new ToolManager(this);` (around line 178):

```typescript
if (this.config.hasProvider && this.kimiConfig?.browser?.enabled !== false) {
  this.browserConnection = new BrowserConnectionManager({
    chromePort: this.kimiConfig?.browser?.chromePort,
    autoLaunch: this.kimiConfig?.browser?.autoLaunch,
    headless: this.kimiConfig?.browser?.headless,
    executablePath: this.kimiConfig?.browser?.executablePath,
  });
}
```

- [ ] Modify `packages/agent-core/src/agent/tool/index.ts` — add browser tools in `initializeBuiltinTools()` after `toolServices?.urlFetcher && new b.FetchURLTool(toolServices.urlFetcher),` (around line 458):

```typescript
this.agent.browserConnection && new b.BrowserBrowseTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserExtractTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserActTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserNavigateTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserSnapshotTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserClickTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserFillTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserEvaluateTool(this.agent.browserConnection),
this.agent.browserConnection && new b.BrowserScreenshotTool(this.agent.browserConnection),
```

- [ ] Find all callers of `Agent` constructor to verify no breaks. Run:

```bash
cd /Users/ranwei/workspace/ody-code && grep -rn "new Agent(" packages/ --include='*.ts' | head -20
```

Expected: callers in `packages/agent-core/src/session/index.ts` and possibly tests. No caller should break because `browserConnection` is an optional field initialized internally.

- [ ] Run whole-tree typecheck:

```bash
cd /Users/ranwei/workspace/ody-code && pnpm -r typecheck
```

Expected: passes across all workspace packages.

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/builtin/index.ts packages/agent-core/src/tools/builtin/browser/index.ts packages/agent-core/src/agent/index.ts packages/agent-core/src/agent/tool/index.ts && git commit -m "feat(browser): register browser tools in ToolManager and Agent"
```

---

### Task 10: Tests for browser tools

**Depends on:** Task 9

**Files:**
- Create: `packages/agent-core/test/browser/tools.test.ts`

**Steps:**

- [ ] Write the test file:

```typescript
import { vi, describe, expect, it, beforeEach } from 'vitest';
import {
  BrowserBrowseTool,
  BrowserExtractTool,
  BrowserActTool,
  BrowserNavigateTool,
  BrowserSnapshotTool,
} from '../../src/tools/builtin/browser';
import { BrowserConnectionManager } from '../../src/browser/connection';
import type { BrowserHandle } from '../../src/browser/types';

vi.mock('../../src/browser/connection', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../src/browser/connection')>();
  return {
    ...actual,
    BrowserConnectionManager: vi.fn(),
  };
});

function createMockPage(overrides: Partial<{
  goto: typeof vi.fn;
  title: typeof vi.fn;
  url: typeof vi.fn;
  evaluate: typeof vi.fn;
  screenshot: typeof vi.fn;
  click: typeof vi.fn;
  type: typeof vi.fn;
  waitForSelector: typeof vi.fn;
  $: typeof vi.fn;
} > = {}) {
  return {
    goto: vi.fn().mockResolvedValue(undefined),
    title: vi.fn().mockResolvedValue('Test Page'),
    url: vi.fn().mockReturnValue('https://example.com/'),
    evaluate: vi.fn().mockResolvedValue({}),
    screenshot: vi.fn().mockResolvedValue('base64screenshot'),
    click: vi.fn().mockResolvedValue(undefined),
    type: vi.fn().mockResolvedValue(undefined),
    waitForSelector: vi.fn().mockResolvedValue(undefined),
    $: vi.fn().mockResolvedValue({
      evaluate: vi.fn().mockResolvedValue('element text'),
    }),
    ...overrides,
  };
}

function createMockHandle(page: ReturnType<typeof createMockPage>): BrowserHandle {
  return {
    id: 'test',
    kind: 'connected',
    browser: { connected: true, close: vi.fn() } as unknown as BrowserHandle['browser'],
    defaultPage: page as unknown as BrowserHandle['defaultPage'],
    acquirePage: vi.fn().mockResolvedValue(page),
    releasePage: vi.fn(),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

describe('Browser tools', () => {
  let mockConnection: BrowserConnectionManager;
  let mockPage: ReturnType<typeof createMockPage>;
  let mockHandle: BrowserHandle;

  beforeEach(() => {
    vi.resetAllMocks();
    mockPage = createMockPage();
    mockHandle = createMockHandle(mockPage);
    mockConnection = new BrowserConnectionManager();
    vi.mocked(mockConnection.resolveOrLaunchBrowser).mockResolvedValue(mockHandle);
  });

  describe('BrowserBrowseTool', () => {
    it('navigates and returns page info', async () => {
      mockPage.evaluate.mockResolvedValue('page content');
      const tool = new BrowserBrowseTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://example.com' });

      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(mockPage.goto).toHaveBeenCalledWith('https://example.com', expect.any(Object));
      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Test Page');
      expect(result.output).toContain('example.com');
    });

    it('sets approvalRule with host', () => {
      const tool = new BrowserBrowseTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://kimi.com/code' });
      expect(execution.approvalRule).toBe('Browser*(kimi.com)');
      expect(execution.matchesRule?.('kimi.com')).toBe(true);
      expect(execution.matchesRule?.('evil.kimi.com')).toBe(false);
    });

    it('returns error for invalid URL', async () => {
      const tool = new BrowserBrowseTool(mockConnection);
      // URL validation happens in schema parse, but resolveExecution would throw
      expect(() => tool.resolveExecution({ url: 'not-a-url' } as unknown as { url: string })).toThrow();
    });
  });

  describe('BrowserNavigateTool', () => {
    it('navigates to URL', async () => {
      const tool = new BrowserNavigateTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://example.com' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(mockPage.goto).toHaveBeenCalledWith('https://example.com', expect.any(Object));
      expect(result.isError).toBeFalsy();
    });

    it('sets Browser*(host) approvalRule', () => {
      const tool = new BrowserNavigateTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://kimi.com' });
      expect(execution.approvalRule).toBe('Browser*(kimi.com)');
    });
  });

  describe('BrowserSnapshotTool', () => {
    it('returns page text content', async () => {
      mockPage.evaluate.mockResolvedValue('Hello world');
      const tool = new BrowserSnapshotTool(mockConnection);
      const execution = tool.resolveExecution({});
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Hello world');
    });

    it('returns error for missing selector', async () => {
      mockPage.$.mockResolvedValue(null);
      const tool = new BrowserSnapshotTool(mockConnection);
      const execution = tool.resolveExecution({ selector: '#missing' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBe(true);
      expect(result.output).toContain('Element not found');
    });
  });

  describe('BrowserExtractTool', () => {
    it('extracts data using schema', async () => {
      mockPage.evaluate.mockResolvedValue({ title: 'Hello', body: 'World' });
      const tool = new BrowserExtractTool(mockConnection);
      const execution = tool.resolveExecution({
        schema: { title: 'h1', body: 'p' },
      });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Hello');
      expect(result.output).toContain('World');
    });

    it('uses tool name approvalRule when no URL', () => {
      const tool = new BrowserExtractTool(mockConnection);
      const execution = tool.resolveExecution({ schema: { title: 'h1' } });
      expect(execution.approvalRule).toBe('BrowserExtract');
      expect(execution.matchesRule).toBeUndefined();
    });

    it('uses Browser*(host) when URL provided', () => {
      const tool = new BrowserExtractTool(mockConnection);
      const execution = tool.resolveExecution({
        url: 'https://kimi.com',
        schema: { title: 'h1' },
      });
      expect(execution.approvalRule).toBe('Browser*(kimi.com)');
      expect(execution.matchesRule?.('kimi.com')).toBe(true);
    });
  });

  describe('BrowserActTool', () => {
    it('clicks element', async () => {
      const tool = new BrowserActTool(mockConnection);
      const execution = tool.resolveExecution({ action: 'click', selector: '#btn' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(mockPage.click).toHaveBeenCalledWith('#btn');
      expect(result.isError).toBeFalsy();
    });

    it('returns error for click without selector', async () => {
      const tool = new BrowserActTool(mockConnection);
      const execution = tool.resolveExecution({ action: 'click' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBe(true);
      expect(result.output).toContain('requires a selector');
    });

    it('uses tool name approvalRule', () => {
      const tool = new BrowserActTool(mockConnection);
      const execution = tool.resolveExecution({ action: 'scroll_down' });
      expect(execution.approvalRule).toBe('BrowserAct');
    });
  });
});
```

- [ ] Run tests:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm test -- test/browser/tools.test.ts
```

Expected: all tests pass.

- [ ] Commit:

```bash
git add packages/agent-core/test/browser/tools.test.ts && git commit -m "test(browser): add browser tool execution tests"
```

---

## Local Self-Review

- [ ] **1. Spec-coverage table:** Phase B covers all tool layer requirements — atomic tools (Task 6), high-level tools (Tasks 7-8), registration (Task 9), tool tests (Task 10).
- [ ] **2. Placeholder scan:** No TODO/TBD in any code block. All tool implementations are complete.
- [ ] **3. No phantom tasks:** Every task produces verifiable changes — 6 atomic tool files, browse tool, extract+act tools, export+registration modifications, test file.
- [ ] **4. Dependency soundness:** Task 6 (foundation.md: Task 5) → Task 7 (Task 6) → Task 8 (Task 7) → Task 9 (Task 8) → Task 10 (Task 9). Correct chain.
- [ ] **5. Caller & build soundness:** Task 9 adds `browserConnection?: BrowserConnectionManager` to `Agent` class (new optional field, no existing callers affected). It also modifies `ToolManager.initializeBuiltinTools()` to register browser tools (no signature change). Task 9 ends with whole-tree `pnpm -r typecheck`.
- [ ] **6. Test-the-risk:** 
  - `BrowserBrowseTool` approvalRule tested with `Browser*(kimi.com)` pattern and `matchesRule` behavior (Task 10).
  - `BrowserExtractTool` fallback to tool-name-only approvalRule tested (Task 10).
  - Page navigation and content extraction tested via mocked page (Task 10).
  - Error paths (missing selector, invalid URL) tested (Task 10).
- [ ] **7. Type consistency:** All browser tools use `BrowserConnectionManager` from `foundation.md` Task 4. Input schemas match those defined in `foundation.md` Task 3 (`types.ts`). `_utils.ts` helper functions are reused across all URL-bearing tools.
