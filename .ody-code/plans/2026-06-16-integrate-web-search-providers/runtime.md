# Part 3: Fallback Provider, Runtime Wiring & Verification

## Task 11: `FallbackWebSearchProvider` and retryable-error classifier

**Depends on:** Part 2 Task 10

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/fallback.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/fallback.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/fallback.test.ts
import { describe, expect, it, vi } from 'vitest';
import {
  FallbackWebSearchProvider,
  isRetryableError,
} from '../../../src/tools/providers/web-search/fallback';
import type { WebSearchProvider, WebSearchResult } from '../../../src/tools/providers/web-search/types';

function fakeProvider(
  name: string,
  behavior: (query: string) => Promise<WebSearchResult[]>,
): WebSearchProvider {
  return { name, search: vi.fn(behavior) };
}

describe('isRetryableError', () => {
  it('treats 401/403/unauthorized/auth as non-retryable', () => {
    expect(isRetryableError(new Error('HTTP 401'))).toBe(false);
    expect(isRetryableError(new Error('HTTP 403'))).toBe(false);
    expect(isRetryableError(new Error('unauthorized'))).toBe(false);
    expect(isRetryableError(new Error('authentication failed'))).toBe(false);
  });

  it('treats 429, 5xx, network, fetch and timeout as retryable', () => {
    expect(isRetryableError(new Error('HTTP 429'))).toBe(true);
    expect(isRetryableError(new Error('HTTP 503'))).toBe(true);
    expect(isRetryableError(new TypeError('fetch failed'))).toBe(true);
    expect(isRetryableError(new Error('network error'))).toBe(true);
    expect(isRetryableError(new Error('timed out'))).toBe(true);
  });

  it('treats AbortError as non-retryable', () => {
    const err = new Error('aborted');
    err.name = 'AbortError';
    expect(isRetryableError(err)).toBe(false);
  });
});

describe('FallbackWebSearchProvider', () => {
  it('returns primary results when primary succeeds', async () => {
    const primary = fakeProvider('primary', async () => [{ title: 'P', url: 'https://p.com', snippet: 'S' }]);
    const secondary = fakeProvider('secondary', async () => [{ title: 'S', url: 'https://s.com', snippet: 'S' }]);
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    const results = await fallback.search('hello');
    expect(results).toHaveLength(1);
    expect(results[0]?.title).toBe('P');
    expect(secondary.search).not.toHaveBeenCalled();
  });

  it('falls back to secondary on retryable primary failure', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('HTTP 503');
    });
    const secondary = fakeProvider('secondary', async () => [{ title: 'S', url: 'https://s.com', snippet: 'S' }]);
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    const results = await fallback.search('hello');
    expect(results[0]?.title).toBe('S');
  });

  it('throws primary error when secondary is undefined', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('primary failed');
    });
    const fallback = new FallbackWebSearchProvider(primary, undefined, { debug: vi.fn() } as never);
    await expect(fallback.search('hello')).rejects.toThrow('primary failed');
  });

  it('does not fallback on auth error', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('HTTP 401');
    });
    const secondary = fakeProvider('secondary', async () => [{ title: 'S', url: 'https://s.com', snippet: 'S' }]);
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    await expect(fallback.search('hello')).rejects.toThrow('HTTP 401');
    expect(secondary.search).not.toHaveBeenCalled();
  });

  it('throws combined secondary error when both fail', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('primary failed');
    });
    const secondary = fakeProvider('secondary', async () => {
      throw new Error('secondary failed');
    });
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    await expect(fallback.search('hello')).rejects.toThrow('secondary failed');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/fallback
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/fallback.ts`:

```ts
import type { Logger } from '../../../logging/types';
import type { WebSearchProvider, WebSearchResult } from './types';

export class FallbackWebSearchProvider implements WebSearchProvider {
  readonly name = 'fallback';

  constructor(
    private readonly primary: WebSearchProvider,
    private readonly secondary: WebSearchProvider | undefined,
    private readonly logger: Logger,
  ) {}

  async search(query: string, options?: { limit?: number; includeContent?: boolean; toolCallId?: string }): Promise<WebSearchResult[]> {
    this.logger.debug('web_search.attempt', { provider: this.primary.name });
    try {
      const results = await this.primary.search(query, options);
      this.logger.debug('web_search.success', { provider: this.primary.name, resultCount: results.length });
      return results;
    } catch (primaryError) {
      this.logger.debug('web_search.failure', {
        provider: this.primary.name,
        errorCategory: categorizeError(primaryError),
      });

      if (this.secondary === undefined) {
        throw primaryError;
      }
      if (!isRetryableError(primaryError)) {
        throw primaryError;
      }

      this.logger.debug('web_search.attempt', { provider: this.secondary.name });
      try {
        const results = await this.secondary.search(query, options);
        this.logger.debug('web_search.success', { provider: this.secondary.name, resultCount: results.length });
        return results;
      } catch (secondaryError) {
        this.logger.debug('web_search.failure', {
          provider: this.secondary.name,
          errorCategory: categorizeError(secondaryError),
        });
        throw secondaryError;
      }
    }
  }
}

export function isRetryableError(error: unknown): boolean {
  const name = error instanceof Error ? error.name : '';
  if (name === 'AbortError') return false;
  if (name === 'TimeoutError') return true;
  const message = String(error instanceof Error ? error.message : error).toLowerCase();
  if (message.includes('401') || message.includes('403') || message.includes('unauthorized') || message.includes('auth')) {
    return false;
  }
  if (message.includes('429')) return true;
  if (/\b5\d\d\b/.test(message) || message.includes('http 5')) return true;
  if (message.includes('network') || message.includes('fetch') || message.includes('timeout') || message.includes('timed out')) {
    return true;
  }
  return false;
}

function categorizeError(error: unknown): string {
  const message = String(error instanceof Error ? error.message : error).toLowerCase();
  if (message.includes('401') || message.includes('403') || message.includes('unauthorized') || message.includes('auth')) {
    return 'auth';
  }
  if (message.includes('429')) return 'rate-limit';
  if (/\b5\d\d\b/.test(message) || message.includes('http 5')) return 'server';
  if (message.includes('timeout') || message.includes('timed out')) return 'timeout';
  if (message.includes('network') || message.includes('fetch') || error instanceof TypeError) return 'network';
  return 'unknown';
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/fallback
```

Expected: all 9 tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/fallback.ts packages/agent-core/test/tools/providers/web-search/fallback.test.ts
git commit -m "feat(tools): FallbackWebSearchProvider with retryable-error classifier"
```

## Task 12: `resolveWebSearchRuntime` helper

**Depends on:** Part 1 Task 3, Part 2 Task 10, Part 3 Task 11

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/runtime.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/runtime.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/runtime.test.ts
import { describe, expect, it, vi } from 'vitest';
import { resolveWebSearchRuntime } from '../../../src/tools/providers/web-search/runtime';
import type { KimiConfig } from '../../../src/config/schema';

describe('resolveWebSearchRuntime', () => {
  it('returns undefined when no search config exists', () => {
    const config: KimiConfig = { providers: {} };
    expect(resolveWebSearchRuntime(config, { fetchImpl: vi.fn() })).toBeUndefined();
  });

  it('returns a fallback provider for webSearch.primary', () => {
    const config: KimiConfig = {
      providers: {},
      services: {
        webSearch: { primary: { provider: 'duckduckgo' } },
      },
    };
    const runtime = resolveWebSearchRuntime(config, { fetchImpl: vi.fn() });
    expect(runtime).toBeDefined();
    expect(runtime?.name).toBe('fallback');
  });

  it('composes primary and secondary providers', () => {
    const config: KimiConfig = {
      providers: {},
      services: {
        webSearch: {
          primary: { provider: 'tavily', apiKey: 'sk-tavily' },
          secondary: { provider: 'duckduckgo' },
        },
      },
    };
    const runtime = resolveWebSearchRuntime(config, { fetchImpl: vi.fn() });
    expect(runtime).toBeDefined();
  });

  it('aliases moonshotSearch to a moonshot provider', () => {
    const config: KimiConfig = {
      providers: {},
      services: {
        moonshotSearch: { baseUrl: 'https://search.example/v1' },
      },
    };
    const runtime = resolveWebSearchRuntime(config, { fetchImpl: vi.fn() });
    expect(runtime).toBeDefined();
    expect(runtime?.name).toBe('fallback');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/runtime
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/runtime.ts`:

```ts
import { resolveWebSearchConfig } from '../../../config/web-search';
import type { KimiConfig } from '../../../config/schema';
import type { Logger } from '../../../logging/types';
import { FallbackWebSearchProvider } from './fallback';
import { createDefaultRegistry, type ProviderFactoryDeps } from './registry';
import type { WebSearchProvider } from './types';

export interface ResolveWebSearchRuntimeDeps extends ProviderFactoryDeps {
  logger?: Logger;
}

export function resolveWebSearchRuntime(
  config: KimiConfig,
  deps: ResolveWebSearchRuntimeDeps,
): WebSearchProvider | undefined {
  const webSearchConfig = resolveWebSearchConfig(config);
  if (webSearchConfig === undefined) return undefined;

  const registry = createDefaultRegistry();
  const primary = registry.create(webSearchConfig.primary, deps);
  const secondary = webSearchConfig.secondary
    ? registry.create(webSearchConfig.secondary, deps)
    : undefined;

  return new FallbackWebSearchProvider(primary, secondary, deps.logger ?? noopLogger);
}

const noopLogger: Logger = {
  debug: () => {},
  info: () => {},
  warn: () => {},
  error: () => {},
  createChild: () => noopLogger,
};
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/runtime
```

Expected: all 4 tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/runtime.ts packages/agent-core/test/tools/providers/web-search/runtime.test.ts
git commit -m "feat(tools): resolveWebSearchRuntime glue helper"
```

## Task 13: Wire `KimiCore.createRuntimeConfig`

**Depends on:** Part 3 Task 12

**Files:**
- Modify: `packages/agent-core/src/rpc/core-impl.ts:821-849`
- Modify: `packages/agent-core/test/harness/runtime.test.ts:91-180`

- [ ] Write the failing test.

Add a new test in `packages/agent-core/test/harness/runtime.test.ts` inside `describe('KimiCore runtime config')` after the existing OAuth test (around line 90):

```ts
it('builds a FallbackWebSearchProvider from services.webSearch', async () => {
  tmp = await mkdtemp(join(tmpdir(), 'kimi-core-runtime-'));
  const homeDir = join(tmp, 'home');
  const workDir = join(tmp, 'work');
  await mkdir(homeDir, { recursive: true });
  await mkdir(workDir, { recursive: true });
  await writeFile(
    join(homeDir, 'config.toml'),
    `
[services.web_search.primary]
provider = "duckduckgo"
`,
  );

  const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
  const core = new KimiCore(coreRpc, { homeDir });
  const rpc = await sdkRpc({
    emitEvent: vi.fn(),
    requestApproval: vi.fn(async (): Promise<ApprovalResponse> => ({ decision: 'rejected' })),
    requestQuestion: vi.fn(async () => null),
    openExternal: vi.fn(async () => ({ opened: false })),
    toolCall: vi.fn(async () => ({ output: '' })),
  });

  const created = await rpc.createSession({ id: 'ses_runtime_web_search', workDir });
  const session = core.sessions.get(created.id);

  expect(session?.options.toolServices?.webSearcher).toBeDefined();
  expect(session?.options.toolServices?.webSearcher?.name).toBe('fallback');
});

it('still builds a Moonshot provider from legacy services.moonshot_search', async () => {
  tmp = await mkdtemp(join(tmpdir(), 'kimi-core-runtime-'));
  const homeDir = join(tmp, 'home');
  const workDir = join(tmp, 'work');
  await mkdir(homeDir, { recursive: true });
  await mkdir(workDir, { recursive: true });
  await writeFile(
    join(homeDir, 'config.toml'),
    `
[services.moonshot_search]
base_url = "https://search.example/v1"
api_key = "sk-legacy"
`,
  );

  const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(
    new Response(JSON.stringify({ search_results: [] }), { status: 200 }),
  );
  vi.stubGlobal('fetch', fetchImpl);

  const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
  const core = new KimiCore(coreRpc, { homeDir });
  const rpc = await sdkRpc({
    emitEvent: vi.fn(),
    requestApproval: vi.fn(async (): Promise<ApprovalResponse> => ({ decision: 'rejected' })),
    requestQuestion: vi.fn(async () => null),
    openExternal: vi.fn(async () => ({ opened: false })),
    toolCall: vi.fn(async () => ({ output: '' })),
  });

  const created = await rpc.createSession({ id: 'ses_runtime_legacy_search', workDir });
  const session = core.sessions.get(created.id);

  expect(session?.options.toolServices?.webSearcher).toBeDefined();
  await session!.options.toolServices?.webSearcher!.search('kimi');
  expect(fetchImpl).toHaveBeenCalled();
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test harness/runtime
```

Expected failure: the new tests fail because `createRuntimeConfig` still builds `MoonshotWebSearchProvider` directly and ignores `services.webSearch`.

- [ ] Write the minimal implementation.

In `packages/agent-core/src/rpc/core-impl.ts`, add the import near the top (around line 9):

```ts
import { resolveWebSearchRuntime } from '#/tools/providers/web-search/runtime';
```

Replace the `createRuntimeConfig` function (currently lines 821-849) with:

```ts
async function createRuntimeConfig(input: {
  readonly config: KimiConfig;
  readonly kimiRequestHeaders?: Record<string, string> | undefined;
  readonly resolveOAuthTokenProvider?: OAuthTokenProviderResolver | undefined;
}): Promise<ToolServices> {
  const localFetcher = new LocalFetchURLProvider();
  const fetchService = input.config.services?.moonshotFetch;

  return {
    urlFetcher:
      fetchService?.baseUrl === undefined
        ? localFetcher
        : new MoonshotFetchURLProvider({
            baseUrl: fetchService.baseUrl,
            localFallback: localFetcher,
            defaultHeaders: input.kimiRequestHeaders,
            ...serviceCredentials(fetchService, input.resolveOAuthTokenProvider),
          }),
    webSearcher: resolveWebSearchRuntime(input.config, {
      fetchImpl: globalThis.fetch.bind(globalThis),
      kimiRequestHeaders: input.kimiRequestHeaders,
      resolveOAuthTokenProvider: input.resolveOAuthTokenProvider,
      moonshotServiceConfig: input.config.services?.moonshotSearch,
      logger: log,
    }),
  };
}
```

The `MoonshotWebSearchProvider` import is no longer directly needed by `createRuntimeConfig`, but keep it if other code in the file uses it; otherwise remove it. Verify with:

```bash
grep -n "MoonshotWebSearchProvider" packages/agent-core/src/rpc/core-impl.ts
```

If only the old `createRuntimeConfig` branch used it, remove the import.

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test harness/runtime
```

Expected: all runtime tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/core-impl.ts packages/agent-core/test/harness/runtime.test.ts
git commit -m "feat(rpc): wire resolveWebSearchRuntime into createRuntimeConfig"
```

## Task 14: Whole-tree typecheck and lint

**Depends on:** Part 3 Task 13

**Files:**
- Modify: none (verification only)

- [ ] Run the focused test suites.

```bash
pnpm --filter @odysseythink/agent-core test config/web-search
pnpm --filter @odysseythink/agent-core test config/configs
pnpm --filter @odysseythink/agent-core test tools/web-search
pnpm --filter @odysseythink/agent-core test tools/providers/web-search
pnpm --filter @odysseythink/agent-core test harness/runtime
```

Expected: all five commands exit 0.

- [ ] Run the package-level typecheck.

```bash
pnpm --filter @odysseythink/agent-core typecheck
```

Expected: no TypeScript errors.

- [ ] Run the workspace-wide typecheck (includes test files in all packages).

```bash
pnpm typecheck
```

Expected: no TypeScript errors across the monorepo.

- [ ] Run lint.

```bash
pnpm lint
```

Expected: no lint errors. If lint auto-fixes formatting, review the diff before committing.

- [ ] Commit any lint/format fixes.

```bash
git add -A
git commit -m "style: lint fixes for web search providers" || echo "no lint changes"
```

## Local Self-Review (Part 3)

- [ ] No TODO/TBD placeholders in fallback, runtime, or wiring code.
- [ ] Every task produced a verifiable change and a commit.
- [ ] Task dependencies are ordered: Task 11 → Task 12 → Task 13 → Task 14.
- [ ] Shared wiring change (`createRuntimeConfig`) is confined to Task 13; `MoonshotWebSearchProvider` import is cleaned up in the same task.
- [ ] Fallback tests assert the risk: auth errors do not trigger fallback, retryable errors do, and combined errors surface the secondary error.
- [ ] Runtime test traces a concrete config value (`services.web_search.primary.provider = "duckduckgo"`) through `createRuntimeConfig` to `session.options.toolServices.webSearcher.name === 'fallback'`.
- [ ] Type names (`FallbackWebSearchProvider`, `resolveWebSearchRuntime`, `ProviderFactoryDeps`) match across implementation and tests.
