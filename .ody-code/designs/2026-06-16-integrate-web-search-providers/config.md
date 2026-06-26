# Config Schema & Data Models

## 1. New Config Schema

### 1.1 Provider name enum [C:USER]

```ts
export const WebSearchProviderNameSchema = z.enum([
  'duckduckgo',
  'serpapi',
  'searchapi',
  'serper',
  'bing',
  'baidu',
  'serply',
  'searxng',
  'tavily',
  'exa',
  'perplexity',
  'moonshot',
]);

export type WebSearchProviderName = z.infer<typeof WebSearchProviderNameSchema>;
```

### 1.2 Per-provider options schemas [C:USER]

```ts
export const DuckDuckGoOptionsSchema = z.object({
  proxyUrl: z.string().url().optional(),
});

export const SerpApiOptionsSchema = z.object({
  engine: z.string().optional(), // default: 'google'
});

export const SearchApiOptionsSchema = z.object({
  engine: z.string().optional(), // default: 'google'
});

export const SerperOptionsSchema = z.object({
  // no extra options beyond apiKey
});

export const BingOptionsSchema = z.object({
  market: z.string().optional(),
});

export const BaiduOptionsSchema = z.object({
  topK: z.number().int().min(1).max(50).optional(), // default: 10
});

export const SerplyOptionsSchema = z.object({
  language: z.string().optional(), // default: 'en'
  hl: z.string().optional(),       // default: 'us'
  gl: z.string().optional(),       // default: 'US'
  device: z.enum(['desktop', 'mobile']).optional(), // default: 'desktop'
});

export const SearXNGOptionsSchema = z.object({
  baseUrl: z.string().url(), // required
});

export const TavilyOptionsSchema = z.object({
  searchDepth: z.enum(['basic', 'advanced']).optional(), // default: 'basic'
});

export const ExaOptionsSchema = z.object({
  type: z.enum(['auto', 'fast', 'deep']).optional(), // default: 'auto'
  livecrawl: z.enum(['fallback', 'preferred']).optional(), // default: 'fallback'
});

export const PerplexityOptionsSchema = z.object({
  maxResults: z.number().int().min(1).max(20).optional(), // default: 5
  maxTokensPerPage: z.number().int().optional(), // default: 2048
});

export const MoonshotOptionsSchema = z.object({
  // uses baseUrl/apiKey at the service level; no extra options
});
```

### 1.3 Provider config schema [C:USER]

```ts
export const WebSearchProviderConfigSchema = z.object({
  provider: WebSearchProviderNameSchema,
  apiKey: z.string().optional(),
  timeoutMs: z.number().int().min(1000).max(120000).optional(), // default: 25000
  options: z.record(z.unknown()).optional(),
});

export type WebSearchProviderConfig = z.infer<typeof WebSearchProviderConfigSchema>;
```

### 1.4 Web search service config [C:USER]

```ts
export const WebSearchConfigSchema = z.object({
  primary: WebSearchProviderConfigSchema,
  secondary: WebSearchProviderConfigSchema.optional(),
});

export type WebSearchConfig = z.infer<typeof WebSearchConfigSchema>;
```

### 1.5 Updated `ServicesConfig` [C:INFERRED]

```ts
export const ServicesConfigSchema = z.object({
  moonshotSearch: MoonshotServiceConfigSchema.optional(),
  moonshotFetch: MoonshotServiceConfigSchema.optional(),
  webSearch: WebSearchConfigSchema.optional(),
});
```

### 1.6 Provider-specific options runtime union [C:INFERRED]

```ts
export type WebSearchProviderOptions =
  | { provider: 'duckduckgo'; } & z.infer<typeof DuckDuckGoOptionsSchema>
  | { provider: 'serpapi'; } & z.infer<typeof SerpApiOptionsSchema>
  | { provider: 'searchapi'; } & z.infer<typeof SearchApiOptionsSchema>
  | { provider: 'serper'; } & z.infer<typeof SerperOptionsSchema>
  | { provider: 'bing'; } & z.infer<typeof BingOptionsSchema>
  | { provider: 'baidu'; } & z.infer<typeof BaiduOptionsSchema>
  | { provider: 'serply'; } & z.infer<typeof SerplyOptionsSchema>
  | { provider: 'searxng'; } & z.infer<typeof SearXNGOptionsSchema>
  | { provider: 'tavily'; } & z.infer<typeof TavilyOptionsSchema>
  | { provider: 'exa'; } & z.infer<typeof ExaOptionsSchema>
  | { provider: 'perplexity'; } & z.infer<typeof PerplexityOptionsSchema>
  | { provider: 'moonshot'; } & z.infer<typeof MoonshotOptionsSchema>;
```

## 2. Backward Compatibility [C:USER]

### 2.1 Alias mapping algorithm

```
function resolveWebSearchConfig(config: KimiConfig): WebSearchConfig | undefined
  if config.services.webSearch is defined
    return validate(config.services.webSearch)

  moonshot := config.services.moonshotSearch
  if moonshot is undefined
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

### 2.2 Moonshot runtime resolution

When `provider === 'moonshot'`:

1. Read `config.services.moonshotSearch`.
2. Use its `baseUrl`, `apiKey`, `oauth`, and `customHeaders` to construct the existing `MoonshotWebSearchProvider`.
3. The `apiKey` in `webSearch.primary.apiKey` acts as an optional override; if empty, fall back to `moonshotSearch.apiKey`.

## 3. TOML Round-Trip [C:INFERRED]

`packages/agent-core/src/config/toml.ts` already has a `servicesToToml` helper. Add a `webSearchToToml` branch so the new section survives write/load cycles:

```
function webSearchToToml(cfg: WebSearchConfig): Record<string, unknown>
  return {
    primary: providerConfigToToml(cfg.primary),
    secondary: cfg.secondary ? providerConfigToToml(cfg.secondary) : undefined,
  }
```

`providerConfigToToml` preserves `provider`, `api_key`, `timeout_ms`, and the provider-specific `options` sub-table.

## 4. Call-Site Integration

### 4.1 Schema addition

File: `packages/agent-core/src/config/schema.ts`
- Add `WebSearchConfigSchema`, `WebSearchProviderConfigSchema`, and provider option schemas after `MoonshotServiceConfigSchema` (around line 129).
- Extend `ServicesConfigSchema` with `webSearch: WebSearchConfigSchema.optional()` (around line 131).

### 4.2 TOML transform

File: `packages/agent-core/src/config/toml.ts`
- Extend `transformServiceData` to recognize `web_search` and call `transformWebSearchData`.
- Add `webSearchToToml` and call it from `servicesToToml` (around line 425).

### 4.3 Runtime config resolution

File: `packages/agent-core/src/rpc/core-impl.ts`
- In `createRuntimeConfig`, replace the current `searchService` branch with a call to `resolveWebSearchRuntime(config, ...)`.
- The helper lives in the new provider runtime module (see `runtime.md`).

## 5. Test Plan

| # | Test | Assertion |
|---|---|---|
| 1 | `WebSearchConfigSchema` accepts minimal primary config | `parse({ primary: { provider: 'duckduckgo' } }).success === true` |
| 2 | `WebSearchConfigSchema` rejects unknown provider | `parse({ primary: { provider: 'unknown' } }).success === false` |
| 3 | Backward alias with only `moonshotSearch` | `resolveWebSearchConfig({ services: { moonshotSearch: { baseUrl: '...' } } }).primary.provider === 'moonshot'` |
| 4 | Override: both `moonshotSearch` and `webSearch` present | `resolveWebSearchConfig(...).primary.provider === 'exa'` |
| 5 | TOML round-trip preserves `webSearch.primary.provider` and nested options | `parseConfigString(stringifyToml(cfg)).services.webSearch.primary.provider === cfg.primary.provider` |
