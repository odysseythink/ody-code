# Registry, Fallback & Runtime Wiring

## 1. WebSearchProviderRegistry [C:INFERRED]

### 1.1 Interface

```ts
export interface WebSearchProviderFactory {
  create(config: WebSearchProviderConfig, deps: ProviderFactoryDeps): WebSearchProvider;
}

export interface ProviderFactoryDeps {
  fetchImpl?: typeof fetch;
  kimiRequestHeaders?: Record<string, string>;
  resolveOAuthTokenProvider?: OAuthTokenProviderResolver;
  moonshotServiceConfig?: MoonshotServiceConfig;
}

export class WebSearchProviderRegistry {
  private readonly factories = new Map<WebSearchProviderName, WebSearchProviderFactory>();

  register(name: WebSearchProviderName, factory: WebSearchProviderFactory): void;
  create(config: WebSearchProviderConfig, deps: ProviderFactoryDeps): WebSearchProvider;
  has(name: WebSearchProviderName): boolean;
}
```

### 1.2 Registration algorithm

```
function createDefaultRegistry(): WebSearchProviderRegistry
  registry := new WebSearchProviderRegistry()
  registry.register('duckduckgo', DuckDuckGoFactory)
  registry.register('serpapi', SerpApiFactory)
  registry.register('searchapi', SearchApiFactory)
  registry.register('serper', SerperFactory)
  registry.register('bing', BingFactory)
  registry.register('baidu', BaiduFactory)
  registry.register('serply', SerplyFactory)
  registry.register('searxng', SearXNGFactory)
  registry.register('tavily', TavilyFactory)
  registry.register('exa', ExaFactory)
  registry.register('perplexity', PerplexityFactory)
  registry.register('moonshot', MoonshotFactory)
  return registry
```

### 1.3 Factory dispatch

```
function create(config, deps)
  factory := this.factories.get(config.provider)
  if factory === undefined
    throw new Error(`Unknown web search provider: ${config.provider}`)
  return factory.create(config, deps)
```

Each provider factory validates its own `options` with its zod schema and constructs the provider with `config.apiKey`, merged options, `config.timeoutMs ?? 25000`, and `deps.fetchImpl ?? fetch`.

## 2. FallbackWebSearchProvider [C:USER]

### 2.1 Interface

```ts
export class FallbackWebSearchProvider implements WebSearchProvider {
  readonly name = 'fallback';
  constructor(
    private primary: WebSearchProvider,
    private secondary: WebSearchProvider | undefined,
    private logger: Logger,
  ) {}

  async search(
    query: string,
    options?: { limit?: number; includeContent?: boolean; toolCallId?: string },
  ): Promise<WebSearchResult[]>;
}
```

### 2.2 Fallback algorithm [C:USER]

```
async function search(query, options)
  logAttempt(this.primary.name)
  try
    results := await this.primary.search(query, options)
    logSuccess(this.primary.name, results.length)
    return results
  catch primaryError
    logFailure(this.primary.name, primaryError)

    if this.secondary === undefined
      throw primaryError

    if !isRetryableError(primaryError)
      throw primaryError

    logAttempt(this.secondary.name)
    try
      results := await this.secondary.search(query, options)
      logSuccess(this.secondary.name, results.length)
      return results
    catch secondaryError
      logFailure(this.secondary.name, secondaryError)
      throw combineErrors(primaryError, secondaryError)
```

### 2.3 Retryable error classifier [C:INFERRED]

```
function isRetryableError(error: unknown): boolean
  if error is AbortError return false
  if error is TimeoutError return true
  message := String(error instanceof Error ? error.message : error).toLowerCase()
  if message contains '401' or '403' or 'unauthorized' return false
  if message contains '429' return true
  if message contains '5' followed by two digits or 'http 5' return true
  if message contains 'network' or 'fetch' or 'timeout' or 'timed out' return true
  return false
```

Auth failures (401/403) do **not** trigger fallback because the secondary provider would likely fail with the same user-provided configuration assumption [C:INFERRED].

### 2.4 Logging format [C:USER]

```
logAttempt(provider):  logger.debug('web_search.attempt', { provider })
logSuccess(provider, count): logger.debug('web_search.success', { provider, resultCount: count })
logFailure(provider, error): logger.debug('web_search.failure', { provider, errorCategory: categorize(error) })
```

No query text or API keys are logged.

## 3. Runtime Wiring

### 3.1 New helper: `resolveWebSearchRuntime` [C:INFERRED]

```ts
export function resolveWebSearchRuntime(
  config: KimiConfig,
  deps: ProviderFactoryDeps,
): WebSearchProvider | undefined {
  const webSearchConfig = resolveWebSearchConfig(config);
  if (webSearchConfig === undefined) return undefined;

  const registry = createDefaultRegistry();
  const primary = registry.create(webSearchConfig.primary, deps);
  const secondary = webSearchConfig.secondary
    ? registry.create(webSearchConfig.secondary, deps)
    : undefined;

  return new FallbackWebSearchProvider(primary, secondary, deps.logger ?? log);
}
```

### 3.2 `resolveWebSearchConfig` [C:USER]

```
function resolveWebSearchConfig(config: KimiConfig): WebSearchConfig | undefined
  if config.services?.webSearch !== undefined
    return validate(config.services.webSearch)

  moonshot := config.services?.moonshotSearch
  if moonshot === undefined
    return undefined

  return {
    primary: {
      provider: 'moonshot',
      apiKey: moonshot.apiKey,
      timeoutMs: 25000,
      options: {},
    }
  }
```

### 3.3 `createRuntimeConfig` change [C:INFERRED]

File: `packages/agent-core/src/rpc/core-impl.ts` (lines 821-849)

Replace the current `webSearcher` branch:

```ts
const localFetcher = new LocalFetchURLProvider();
const fetchService = input.config.services?.moonshotFetch;

return {
  urlFetcher:
    fetchService?.baseUrl === undefined
      ? localFetcher
      : new MoonshotFetchURLProvider({ ... }),
  webSearcher: resolveWebSearchRuntime(input.config, {
    fetchImpl: globalThis.fetch.bind(globalThis),
    kimiRequestHeaders: input.kimiRequestHeaders,
    resolveOAuthTokenProvider: input.resolveOAuthTokenProvider,
    moonshotServiceConfig: input.config.services?.moonshotSearch,
    logger: log,
  }),
};
```

If `resolveWebSearchRuntime` returns `undefined`, `ToolServices.webSearcher` is `undefined` and `WebSearchTool` is not registered (existing behavior) [C:INFERRED].

## 4. Error Handling

### 4.1 Error categories and handling [C:INFERRED]

| Error class | Detection | Immediate handling | Degradation path | Recovery |
|---|---|---|---|---|
| Missing API key | Provider throws before fetch or 401/403 response | Throw with provider-specific message | Fallback only if secondary configured and error is retryable | User adds apiKey |
| Timeout / network | `TimeoutError`, `TypeError`, `fetch` failure, 5xx | Log, mark retryable, trigger fallback if secondary exists | Use secondary provider | Transient; retry next turn |
| Rate limit (429) | HTTP 429 or message contains "rate limit" | Log, mark retryable, trigger fallback | Use secondary provider | Cooldown / quota refresh |
| Auth (401/403) | HTTP status or message | Throw immediately; do **not** fallback | None (assumption: secondary likely same user config) | User fixes key/permissions |
| Empty results | Provider returns `[]` | Return "No search results found" (existing tool behavior) | None needed | User refines query |
| DuckDuckGo parse failure | HTML parser returns `[]` or throws | Throw retryable error; fallback if configured | Use secondary provider | DDG layout change requires code fix |

### 4.2 Error propagation to the model

`FallbackWebSearchProvider` preserves the original error. `WebSearchTool.classifySearchError` already adds a category prefix:

- `Search timed out:`
- `Search cancelled:`
- `Search failed (authentication):`
- `Search failed (network):`
- `Search failed:`

When fallback also fails, the tool surfaces the secondary error; debug logs contain both.

## 5. Call-Site Integration

### 5.1 `packages/agent-core/src/rpc/core-impl.ts`

- Import `resolveWebSearchRuntime` at the top (near existing provider imports) [C:INFERRED].
- In `createRuntimeConfig` (line ~821): replace `webSearcher` construction with `resolveWebSearchRuntime(...)`.

### 5.2 `packages/agent-core/src/tools/builtin/web/web-search.ts`

- No behavioral change. Add `raw?: unknown` to `WebSearchResult` interface [C:USER].
- The tool already catches errors and calls `classifySearchError`; composite provider errors flow through unchanged.

### 5.3 `packages/agent-core/src/tools/builtin/index.ts`

- No change; `WebSearchTool` is already registered conditionally when `toolServices.webSearcher` is present.

## 6. Test Plan

### 6.1 Unit tests

| # | Test | Assertion |
|---|---|---|
| 1 | Registry creates DuckDuckGo provider | `registry.create({ provider: 'duckduckgo' }, deps).name === 'duckduckgo'` |
| 2 | Registry rejects unknown provider | `throws(/Unknown web search provider: unknown/)` |
| 3 | Fallback returns primary results when primary succeeds | `fallback.search('q')` resolves to primary results |
| 4 | Fallback returns secondary results when primary throws retryable error | primary rejects with `TypeError`, secondary resolves to `[r]` |
| 5 | Fallback throws primary error when secondary undefined and primary fails | `rejects.toThrow('primary failed')` |
| 6 | Fallback does **not** fallback on auth error | primary rejects with `Error('HTTP 401')`, `rejects.toThrow('HTTP 401')` |
| 7 | Config alias: only `moonshotSearch` produces `moonshot` primary | `resolveWebSearchConfig(cfg).primary.provider === 'moonshot'` |
| 8 | Config override: `webSearch` takes precedence | both present → `primary.provider === cfg.services.webSearch.primary.provider` |

### 6.2 Integration tests

| # | Test | Assertion |
|---|---|---|
| 9 | `createRuntimeConfig` with `webSearch.primary.provider = 'duckduckgo'` returns a `ToolServices.webSearcher` | `runtime.webSearcher instanceof FallbackWebSearchProvider` (or duck-type check) |
| 10 | `createRuntimeConfig` with no search config returns `webSearcher: undefined` | `runtime.webSearcher === undefined` |

### 6.3 Done criteria

```bash
pnpm --filter @odysseythink/agent-core test tools/web-search
pnpm --filter @odysseythink/agent-core test config
pnpm --filter @odysseythink/agent-core test rpc
```

All three commands must pass. Lint and type-check must also pass:

```bash
pnpm lint
pnpm typecheck
```
