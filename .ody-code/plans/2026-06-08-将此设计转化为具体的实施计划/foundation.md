# Phase A: Foundation — 依赖、配置、类型、连接管理器

---

### Task 1: Add `puppeteer-core` dependency

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/package.json:56-77`
- Test: `pnpm install` lockfile update

**Steps:**

- [ ] Add `puppeteer-core` to `packages/agent-core/package.json` dependencies array:

```json
"puppeteer-core": "^25.1.0",
```

Insert it alphabetically between `"pathe"` and `"proper-lockfile"`.

Full dependencies block after change (lines 56-77):

```json
  "dependencies": {
    "@antfu/utils": "^9.3.0",
    "@modelcontextprotocol/sdk": "^1.29.0",
    "@odysseythink/kaos": "workspace:^",
    "@odysseythink/kosong": "workspace:^",
    "@mozilla/readability": "^0.6.0",
    "ajv": "^8.18.0",
    "ajv-formats": "^3.0.1",
    "js-yaml": "^4.1.1",
    "linkedom": "^0.18.12",
    "nunjucks": "^3.2.4",
    "open": "^10.2.0",
    "pathe": "^2.0.3",
    "picomatch": "^4.0.4",
    "proper-lockfile": "^4.1.2",
    "puppeteer-core": "^25.1.0",
    "regexp.escape": "^2.0.0",
    "retry": "0.13.1",
    "smol-toml": "^1.6.1",
    "tar": "^7.5.13",
    "yauzl": "^3.3.0",
    "zod": "catalog:"
  },
```

- [ ] Run `pnpm install` from repo root:

```bash
cd /Users/ranwei/workspace/ody-code && pnpm install
```

Expected output: lockfile updated, no errors, `puppeteer-core` downloaded.

- [ ] Commit:

```bash
git add packages/agent-core/package.json pnpm-lock.yaml && git commit -m "deps(agent-core): add puppeteer-core for native browser tools"
```

---

### Task 2: Extend `BrowserConfigSchema`

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/config/schema.ts:186-191`
- Modify: `packages/agent-core/test/config/browser-config.test.ts`

**Steps:**

- [ ] Write the failing test first. Append to `packages/agent-core/test/config/browser-config.test.ts` after the existing `it('is accepted by KimiConfigPatchSchema', ...)` block:

```typescript
  it('parses new fields: autoLaunch, headless, executablePath, legacyMcpEnabled', () => {
    const parsed = BrowserConfigSchema.parse({
      enabled: true,
      chromePort: 9222,
      autoLaunch: true,
      headless: false,
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      legacyMcpEnabled: false,
    });
    expect(parsed).toEqual({
      enabled: true,
      chromePort: 9222,
      autoLaunch: true,
      headless: false,
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      legacyMcpEnabled: false,
    });
  });

  it('parses with only legacyMcpEnabled', () => {
    expect(BrowserConfigSchema.parse({ legacyMcpEnabled: true })).toEqual({
      legacyMcpEnabled: true,
    });
  });
```

- [ ] Run test and verify it FAILS:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm test -- test/config/browser-config.test.ts
```

Expected failure: `ZodError` — `autoLaunch`, `headless`, `executablePath`, `legacyMcpEnabled` are unrecognized fields.

- [ ] Update `BrowserConfigSchema` in `packages/agent-core/src/config/schema.ts:186-191`:

```typescript
export const BrowserConfigSchema = z.object({
  enabled: z.boolean().optional(),
  chromePort: z.number().int().min(1).max(65535).optional(),
  traceEnabled: z.boolean().optional(),
  traceRetentionDays: z.number().int().min(1).optional(),
  autoLaunch: z.boolean().optional(),
  headless: z.boolean().optional(),
  executablePath: z.string().optional(),
  legacyMcpEnabled: z.boolean().optional(),
});
```

- [ ] Run test and verify it PASSES:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm test -- test/config/browser-config.test.ts
```

Expected: all tests pass.

- [ ] Commit:

```bash
git add packages/agent-core/src/config/schema.ts packages/agent-core/test/config/browser-config.test.ts && git commit -m "feat(config): extend BrowserConfigSchema with autoLaunch, headless, executablePath, legacyMcpEnabled"
```

---

### Task 3: Define browser types and interfaces

**Depends on:** Task 2

**Files:**
- Create: `packages/agent-core/src/browser/types.ts`
- Create: `packages/agent-core/src/browser/connection.ts` (stub)
- Create: `packages/agent-core/src/browser/index.ts`

**Steps:**

- [ ] Create `packages/agent-core/src/browser/types.ts`:

```typescript
import { z } from 'zod';
import type { Browser, Page } from 'puppeteer-core';

// ─── Connection Options ───

export interface BrowserConnectionOptions {
  /** Explicit remote debugging port for existing Chrome. */
  chromePort?: number;
  /** Whether to auto-launch a new browser if connection fails. Default: true. */
  autoLaunch?: boolean;
  /** Headless mode for launched instances. Default: true. */
  headless?: boolean;
  /** Explicit Chrome/Chromium executable path for launch. */
  executablePath?: string;
  /** User data dir for launched instances. */
  userDataDir?: string;
}

// ─── Browser Handle ───

export interface BrowserHandle {
  readonly id: string;
  readonly kind: 'connected' | 'launched';
  readonly browser: Browser;
  readonly defaultPage: Page;
  acquirePage(): Promise<Page>;
  releasePage(page: Page): void;
  close(): Promise<void>;
}

// ─── Tool Input Schemas ───

export const BrowserBrowseInputSchema = z.object({
  url: z.string().url(),
  goal: z.string().optional(),
  waitFor: z.union([z.string(), z.number().int().min(0)]).optional(),
  extract: z.record(z.string(), z.string()).optional(),
});

export type BrowserBrowseInput = z.infer<typeof BrowserBrowseInputSchema>;

export const BrowserExtractInputSchema = z.object({
  url: z.string().url().optional(),
  schema: z.record(z.string(), z.string()),
});

export type BrowserExtractInput = z.infer<typeof BrowserExtractInputSchema>;

export const BrowserActInputSchema = z.object({
  action: z.enum(['click', 'type', 'scroll_down', 'scroll_up', 'screenshot', 'wait']),
  selector: z.string().optional(),
  value: z.string().optional(),
});

export type BrowserActInput = z.infer<typeof BrowserActInputSchema>;

export const BrowserNavigateInputSchema = z.object({
  url: z.string().url(),
});

export type BrowserNavigateInput = z.infer<typeof BrowserNavigateInputSchema>;

export const BrowserSnapshotInputSchema = z.object({
  selector: z.string().optional(),
});

export type BrowserSnapshotInput = z.infer<typeof BrowserSnapshotInputSchema>;

export const BrowserClickInputSchema = z.object({
  selector: z.string(),
});

export type BrowserClickInput = z.infer<typeof BrowserClickInputSchema>;

export const BrowserFillInputSchema = z.object({
  selector: z.string(),
  value: z.string(),
});

export type BrowserFillInput = z.infer<typeof BrowserFillInputSchema>;

export const BrowserEvaluateInputSchema = z.object({
  script: z.string(),
});

export type BrowserEvaluateInput = z.infer<typeof BrowserEvaluateInputSchema>;

export const BrowserScreenshotInputSchema = z.object({
  fullPage: z.boolean().optional(),
});

export type BrowserScreenshotInput = z.infer<typeof BrowserScreenshotInputSchema>;

// ─── Tool Output ───

export interface BrowserToolOutput {
  readonly success: boolean;
  readonly url: string;
  readonly title: string;
  readonly content?: string;
  readonly data?: unknown;
  readonly screenshot?: string;
  readonly error?: string;
}

// ─── Error ───

export class BrowserConnectionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'BrowserConnectionError';
  }
}
```

- [ ] Create stub `packages/agent-core/src/browser/connection.ts`:

```typescript
// Stub — will be implemented in Task 4
export class BrowserConnectionManager {}
```

- [ ] Create `packages/agent-core/src/browser/index.ts`:

```typescript
export * from './types';
export * from './connection';
```

- [ ] Run compile check:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes (stubs only, no real puppeteer calls yet).

- [ ] Commit:

```bash
git add packages/agent-core/src/browser/ && git commit -m "feat(browser): define browser tool types and connection interfaces"
```

---

### Task 4: Implement `BrowserConnectionManager`

**Depends on:** Task 3

**Files:**
- Create: `packages/agent-core/src/browser/connection.ts` (overwrite stub)

**Steps:**

- [ ] Write the implementation to `packages/agent-core/src/browser/connection.ts`:

```typescript
import puppeteer, { type Browser, type Page } from 'puppeteer-core';
import {
  BrowserConnectionError,
  type BrowserConnectionOptions,
  type BrowserHandle,
} from './types';

export class BrowserConnectionManager {
  private activeHandle?: BrowserHandle;
  private readonly options: Required<Pick<BrowserConnectionOptions, 'autoLaunch' | 'headless'>> &
    BrowserConnectionOptions;

  constructor(options: BrowserConnectionOptions = {}) {
    this.options = {
      autoLaunch: true,
      headless: true,
      ...options,
    };
  }

  async resolveOrLaunchBrowser(): Promise<BrowserHandle> {
    if (this.activeHandle) {
      try {
        if (this.activeHandle.browser.connected) {
          return this.activeHandle;
        }
      } catch {
        // Browser disconnected, continue to reconnect
      }
    }

    this.activeHandle = undefined;

    // PRIMARY: Try to connect to existing Chrome
    const connectedHandle = await this.tryConnect();
    if (connectedHandle) {
      this.activeHandle = connectedHandle;
      return connectedHandle;
    }

    // FALLBACK: Launch new browser
    if (this.options.autoLaunch) {
      const launchedHandle = await this.tryLaunch();
      if (launchedHandle) {
        this.activeHandle = launchedHandle;
        return launchedHandle;
      }
    }

    throw new BrowserConnectionError(
      'No browser available. Please start Chrome with --remote-debugging-port, ' +
        'or set browser.autoLaunch=true in config.',
    );
  }

  getActiveHandle(): BrowserHandle | undefined {
    return this.activeHandle;
  }

  async closeAll(): Promise<void> {
    if (this.activeHandle) {
      await this.activeHandle.close();
      this.activeHandle = undefined;
    }
  }

  private async tryConnect(): Promise<BrowserHandle | undefined> {
    const ports = this.options.chromePort
      ? [this.options.chromePort]
      : [9222, 9223, 9224];

    for (const port of ports) {
      try {
        const browser = await puppeteer.connect({
          browserURL: `http://127.0.0.1:${port}`,
          defaultViewport: null,
        });
        return await this.createHandle(browser, 'connected');
      } catch {
        // Try next port
      }
    }
    return undefined;
  }

  private async tryLaunch(): Promise<BrowserHandle | undefined> {
    try {
      const launchOptions: puppeteer.LaunchOptions = {
        headless: this.options.headless,
        defaultViewport: null,
      };
      if (this.options.executablePath) {
        launchOptions.executablePath = this.options.executablePath;
      }
      if (this.options.userDataDir) {
        launchOptions.userDataDir = this.options.userDataDir;
      }
      const browser = await puppeteer.launch(launchOptions);
      return await this.createHandle(browser, 'launched');
    } catch {
      return undefined;
    }
  }

  private async createHandle(
    browser: Browser,
    kind: 'connected' | 'launched',
  ): Promise<BrowserHandle> {
    const id = `${kind}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const defaultPage = await browser.newPage();
    const pagePool: Page[] = [];
    const acquiredPages = new Set<Page>([defaultPage]);

    return {
      id,
      kind,
      browser,
      defaultPage,
      acquirePage: async () => {
        const released = pagePool.pop();
        if (released && !released.isClosed()) {
          acquiredPages.add(released);
          return released;
        }
        const page = await browser.newPage();
        acquiredPages.add(page);
        return page;
      },
      releasePage: (page: Page) => {
        acquiredPages.delete(page);
        if (page !== defaultPage && !page.isClosed()) {
          pagePool.push(page);
        }
      },
      close: async () => {
        for (const page of [...pagePool, ...acquiredPages]) {
          if (!page.isClosed()) {
            await page.close().catch(() => {});
          }
        }
        pagePool.length = 0;
        acquiredPages.clear();
        if (browser.connected) {
          await browser.close().catch(() => {});
        }
      },
    };
  }
}
```

- [ ] Run typecheck:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes (puppeteer-core types available).

- [ ] Commit:

```bash
git add packages/agent-core/src/browser/connection.ts && git commit -m "feat(browser): implement BrowserConnectionManager with CDP connect and launch fallback"
```

---

### Task 5: Tests for `BrowserConnectionManager`

**Depends on:** Task 4

**Files:**
- Create: `packages/agent-core/test/browser/connection.test.ts`

**Steps:**

- [ ] Write the test file:

```typescript
import { vi, describe, expect, it, beforeEach } from 'vitest';
import puppeteer from 'puppeteer-core';
import {
  BrowserConnectionManager,
  BrowserConnectionError,
} from '../../src/browser/connection';

vi.mock('puppeteer-core', () => ({
  default: {
    connect: vi.fn(),
    launch: vi.fn(),
  },
}));

function createMockBrowser() {
  return {
    connected: true,
    newPage: vi.fn().mockResolvedValue({
      isClosed: () => false,
      close: vi.fn().mockResolvedValue(undefined),
    }),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

describe('BrowserConnectionManager', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('connects to existing Chrome on port 9222', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    const handle = await manager.resolveOrLaunchBrowser();

    expect(handle.kind).toBe('connected');
    expect(puppeteer.connect).toHaveBeenCalledWith(
      expect.objectContaining({ browserURL: 'http://127.0.0.1:9222' }),
    );
    expect(puppeteer.launch).not.toHaveBeenCalled();
  });

  it('tries custom chromePort when provided', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager({ chromePort: 9333 });
    await manager.resolveOrLaunchBrowser();

    expect(puppeteer.connect).toHaveBeenCalledWith(
      expect.objectContaining({ browserURL: 'http://127.0.0.1:9333' }),
    );
    expect(puppeteer.connect).toHaveBeenCalledTimes(1);
  });

  it('falls back to launch when connect fails and autoLaunch=true', async () => {
    vi.mocked(puppeteer.connect).mockRejectedValue(new Error('Connection failed'));
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.launch).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.launch>);

    const manager = new BrowserConnectionManager();
    const handle = await manager.resolveOrLaunchBrowser();

    expect(handle.kind).toBe('launched');
    expect(puppeteer.launch).toHaveBeenCalledWith(
      expect.objectContaining({ headless: true, defaultViewport: null }),
    );
  });

  it('passes executablePath to launch when configured', async () => {
    vi.mocked(puppeteer.connect).mockRejectedValue(new Error('Connection failed'));
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.launch).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.launch>);

    const manager = new BrowserConnectionManager({
      executablePath: '/usr/bin/google-chrome',
    });
    await manager.resolveOrLaunchBrowser();

    expect(puppeteer.launch).toHaveBeenCalledWith(
      expect.objectContaining({ executablePath: '/usr/bin/google-chrome' }),
    );
  });

  it('throws BrowserConnectionError when connect fails and autoLaunch=false', async () => {
    vi.mocked(puppeteer.connect).mockRejectedValue(new Error('Connection failed'));

    const manager = new BrowserConnectionManager({ autoLaunch: false });
    await expect(manager.resolveOrLaunchBrowser()).rejects.toBeInstanceOf(BrowserConnectionError);
    await expect(manager.resolveOrLaunchBrowser()).rejects.toThrow('No browser available');
  });

  it('returns same handle on subsequent calls (singleton)', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    const handle1 = await manager.resolveOrLaunchBrowser();
    const handle2 = await manager.resolveOrLaunchBrowser();

    expect(handle1).toBe(handle2);
    expect(puppeteer.connect).toHaveBeenCalledTimes(1);
  });

  it('reconnects when active browser disconnects', async () => {
    const mockBrowser1 = { ...createMockBrowser(), connected: false };
    const mockBrowser2 = createMockBrowser();

    vi.mocked(puppeteer.connect)
      .mockResolvedValueOnce(mockBrowser1 as unknown as ReturnType<typeof puppeteer.connect>)
      .mockResolvedValueOnce(mockBrowser2 as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    const handle1 = await manager.resolveOrLaunchBrowser();
    expect(handle1.browser).toBe(mockBrowser1);

    const handle2 = await manager.resolveOrLaunchBrowser();
    expect(handle2.browser).toBe(mockBrowser2);
    expect(puppeteer.connect).toHaveBeenCalledTimes(2);
  });

  it('closeAll closes browser and clears active handle', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    await manager.resolveOrLaunchBrowser();
    await manager.closeAll();

    expect(mockBrowser.close).toHaveBeenCalled();
    expect(manager.getActiveHandle()).toBeUndefined();
  });
});
```

- [ ] Run the tests:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm test -- test/browser/connection.test.ts
```

Expected: all 8 tests pass.

- [ ] Commit:

```bash
git add packages/agent-core/test/browser/connection.test.ts && git commit -m "test(browser): add BrowserConnectionManager unit tests"
```

---

## Local Self-Review

- [ ] **1. Spec-coverage table:** All Phase A requirements (dependency, config schema, types, connection manager, singleton, connect/launch fallback) are covered by Tasks 1-5.
- [ ] **2. Placeholder scan:** No TODO/TBD in any code block. Every file contains complete implementation.
- [ ] **3. No phantom tasks:** Every task produces verifiable changes — package.json edit + lockfile, schema edit + test, type files, connection manager implementation, test file.
- [ ] **4. Dependency soundness:** Task 1 (none) → Task 2 (Task 1) → Task 3 (Task 2) → Task 4 (Task 3) → Task 5 (Task 4). Correct chain.
- [ ] **5. Caller & build soundness:** Task 2 changes `BrowserConfigSchema` which is used by `KimiConfigSchema` and `KimiConfigPatchSchema` in the same file — both automatically pick up the new fields because they reference `BrowserConfigSchema` directly. No external callers need updating. Task 4 introduces new files only. No shared signature changes across multiple tasks.
- [ ] **6. Test-the-risk:** 
  - Connection singleton tested (Task 5, test "returns same handle on subsequent calls").
  - Launch fallback tested (Task 5, test "falls back to launch when connect fails").
  - Auto-launch disable tested (Task 5, test "throws when connect fails and autoLaunch=false").
  - Config schema new fields tested (Task 2, test "parses new fields").
- [ ] **7. Type consistency:** `BrowserConnectionOptions` in `types.ts` matches the fields added to `BrowserConfigSchema` (`chromePort`, `autoLaunch`, `headless`, `executablePath`, `userDataDir`). The `BrowserConnectionManager` constructor accepts `BrowserConnectionOptions` and applies defaults correctly.
