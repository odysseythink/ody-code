# Part 1: Config Schema, TOML Round-Trip & Backward Compatibility

## Task 1: Add `services.webSearch` schemas

**Depends on:** none

**Files:**
- Create: `packages/agent-core/test/config/web-search.test.ts`
- Modify: `packages/agent-core/src/config/schema.ts:122-140`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/config/web-search.test.ts
import { describe, expect, it } from 'vitest';
import {
  KimiConfigSchema,
  ServicesConfigSchema,
  WebSearchConfigSchema,
  WebSearchProviderConfigSchema,
  WebSearchProviderNameSchema,
} from '../../src/config/schema';

describe('WebSearchConfigSchema', () => {
  it('accepts a minimal primary config', () => {
    const result = WebSearchConfigSchema.safeParse({ primary: { provider: 'duckduckgo' } });
    expect(result.success).toBe(true);
  });

  it('accepts primary and secondary slots', () => {
    const result = WebSearchConfigSchema.safeParse({
      primary: { provider: 'tavily', apiKey: 'sk-primary' },
      secondary: { provider: 'duckduckgo' },
    });
    expect(result.success).toBe(true);
  });

  it('rejects unknown provider names', () => {
    const result = WebSearchProviderConfigSchema.safeParse({ provider: 'unknown' });
    expect(result.success).toBe(false);
  });

  it('rejects timeoutMs below 1000', () => {
    const result = WebSearchProviderConfigSchema.safeParse({
      provider: 'duckduckgo',
      timeoutMs: 500,
    });
    expect(result.success).toBe(false);
  });

  it('rejects timeoutMs above 120000', () => {
    const result = WebSearchProviderConfigSchema.safeParse({
      provider: 'duckduckgo',
      timeoutMs: 200000,
    });
    expect(result.success).toBe(false);
  });

  it('includes webSearch in ServicesConfigSchema', () => {
    const result = ServicesConfigSchema.safeParse({
      moonshotSearch: { baseUrl: 'https://search.example/v1' },
      webSearch: { primary: { provider: 'exa' } },
    });
    expect(result.success).toBe(true);
  });

  it('includes webSearch in KimiConfigSchema through services', () => {
    const result = KimiConfigSchema.safeParse({
      services: { webSearch: { primary: { provider: 'perplexity' } } },
    });
    expect(result.success).toBe(true);
    if (!result.success) return;
    expect(result.data.services?.webSearch?.primary.provider).toBe('perplexity');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test config/web-search
```

Expected failure: `Cannot find module '../../src/config/schema' or ... 'WebSearchConfigSchema' does not exist in ...`.

- [ ] Write the minimal implementation.

```ts
// packages/agent-core/src/config/schema.ts
// Insert after MoonshotServiceConfigSchema (around line 129).

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

export const DuckDuckGoOptionsSchema = z.object({
  proxyUrl: z.string().url().optional(),
});

export const SerpApiOptionsSchema = z.object({
  engine: z.string().optional(),
});

export const SearchApiOptionsSchema = z.object({
  engine: z.string().optional(),
});

export const SerperOptionsSchema = z.object({});

export const BingOptionsSchema = z.object({
  market: z.string().optional(),
});

export const BaiduOptionsSchema = z.object({
  topK: z.number().int().min(1).max(50).optional(),
});

export const SerplyOptionsSchema = z.object({
  language: z.string().optional(),
  hl: z.string().optional(),
  gl: z.string().optional(),
  device: z.enum(['desktop', 'mobile']).optional(),
});

export const SearXNGOptionsSchema = z.object({
  baseUrl: z.string().url(),
});

export const TavilyOptionsSchema = z.object({
  searchDepth: z.enum(['basic', 'advanced']).optional(),
});

export const ExaOptionsSchema = z.object({
  type: z.enum(['auto', 'fast', 'deep']).optional(),
  livecrawl: z.enum(['fallback', 'preferred']).optional(),
});

export const PerplexityOptionsSchema = z.object({
  maxResults: z.number().int().min(1).max(20).optional(),
  maxTokensPerPage: z.number().int().optional(),
});

export const MoonshotOptionsSchema = z.object({});

export const WebSearchProviderConfigSchema = z.object({
  provider: WebSearchProviderNameSchema,
  apiKey: z.string().optional(),
  timeoutMs: z.number().int().min(1000).max(120000).optional(),
  options: z.record(z.unknown()).optional(),
});
export type WebSearchProviderConfig = z.infer<typeof WebSearchProviderConfigSchema>;

export const WebSearchConfigSchema = z.object({
  primary: WebSearchProviderConfigSchema,
  secondary: WebSearchProviderConfigSchema.optional(),
});
export type WebSearchConfig = z.infer<typeof WebSearchConfigSchema>;
```

Then extend `ServicesConfigSchema` (currently lines 131-134):

```ts
export const ServicesConfigSchema = z.object({
  moonshotSearch: MoonshotServiceConfigSchema.optional(),
  moonshotFetch: MoonshotServiceConfigSchema.optional(),
  webSearch: WebSearchConfigSchema.optional(),
});
```

And extend `ServicesConfigPatchSchema` (currently lines 248-252) by adding `webSearch`:

```ts
const MoonshotServiceConfigPatchSchema = MoonshotServiceConfigSchema.partial();
const WebSearchProviderConfigPatchSchema = WebSearchProviderConfigSchema.partial();
const WebSearchConfigPatchSchema = z.object({
  primary: WebSearchProviderConfigPatchSchema.optional(),
  secondary: WebSearchProviderConfigPatchSchema.optional(),
});
const ServicesConfigPatchSchema = z.object({
  moonshotSearch: MoonshotServiceConfigPatchSchema.optional(),
  moonshotFetch: MoonshotServiceConfigPatchSchema.optional(),
  webSearch: WebSearchConfigPatchSchema.optional(),
});
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test config/web-search
```

Expected: all 7 tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/config/schema.ts packages/agent-core/test/config/web-search.test.ts
git commit -m "feat(config): add services.webSearch schema with 12 provider names"
```

## Task 2: TOML read/write for `services.web_search`

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/config/toml.ts:130-147, 244-258, 425-452`
- Modify: `packages/agent-core/test/config/configs.test.ts:115-128, 193-200, 256-299`

- [ ] Write the failing test.

Add the following snippet to `COMPLETE_TOML` in `packages/agent-core/test/config/configs.test.ts` after the `[services.moonshot_fetch]` block (around line 123):

```toml
[services.web_search]
[services.web_search.primary]
provider = "tavily"
api_key = "sk-tavily"
timeout_ms = 15000
[services.web_search.primary.options]
search_depth = "advanced"
[services.web_search.secondary]
provider = "duckduckgo"
```

Add assertions inside `it('parses the current config.toml shape ...')` after the existing `services` assertions (around line 195):

```ts
expect(config.services?.webSearch?.primary.provider).toBe('tavily');
expect(config.services?.webSearch?.primary.apiKey).toBe('sk-tavily');
expect(config.services?.webSearch?.primary.timeoutMs).toBe(15000);
expect(config.services?.webSearch?.primary.options).toEqual({ searchDepth: 'advanced' });
expect(config.services?.webSearch?.secondary?.provider).toBe('duckduckgo');
```

Add a dedicated round-trip test at the end of the `describe('harness config TOML loader')` block:

```ts
it('round-trips services.web_search with provider-specific options', async () => {
  const dir = makeTempDir();
  const configPath = join(dir, 'web-search.toml');
  const toml = `
[services.web_search.primary]
provider = "tavily"
api_key = "sk-tavily"
timeout_ms = 15000
[services.web_search.primary.options]
search_depth = "advanced"
[services.web_search.secondary]
provider = "duckduckgo"
`;
  const config = parseConfigString(toml, configPath);
  expect(config.services?.webSearch?.primary.provider).toBe('tavily');
  expect(config.services?.webSearch?.primary.options).toEqual({ searchDepth: 'advanced' });

  await writeConfigFile(configPath, config);
  const text = await readFile(configPath, 'utf-8');
  expect(text).toContain('[services.web_search]');
  expect(text).toContain('provider = "tavily"');
  expect(text).toContain('search_depth = "advanced"');

  const roundTripped = parseConfigString(text, configPath);
  expect(roundTripped.services?.webSearch?.primary.provider).toBe('tavily');
  expect(roundTripped.services?.webSearch?.primary.options).toEqual({ searchDepth: 'advanced' });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test config/configs
```

Expected failure: `config.services?.webSearch` is `undefined` because the TOML transform does not yet recognize `web_search`.

- [ ] Write the minimal implementation.

In `transformTomlData` (around line 130), add a branch for `webSearch`:

```ts
} else if (targetKey === 'services' && isPlainObject(value)) {
  result[targetKey] = transformRecord(value, transformServiceData, snakeToCamel);
```

Change `transformServiceData` (lines 244-257) to dispatch nested `webSearch`:

```ts
function transformServiceData(data: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    const targetKey = snakeToCamel(key);
    if (targetKey === 'oauth') {
      out[targetKey] = isPlainObject(value) ? transformPlainObject(value) : value;
    } else if (targetKey === 'customHeaders') {
      out[targetKey] = cloneObjectValue(value);
    } else if (targetKey === 'webSearch' && isPlainObject(value)) {
      out[targetKey] = transformWebSearchData(value);
    } else {
      out[targetKey] = value;
    }
  }
  return out;
}

function transformWebSearchData(data: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    const targetKey = snakeToCamel(key);
    if ((targetKey === 'primary' || targetKey === 'secondary') && isPlainObject(value)) {
      out[targetKey] = transformWebSearchProviderData(value);
    } else {
      out[targetKey] = value;
    }
  }
  return out;
}

function transformWebSearchProviderData(data: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    const targetKey = snakeToCamel(key);
    if (targetKey === 'options' && isPlainObject(value)) {
      out[targetKey] = transformPlainObject(value);
    } else {
      out[targetKey] = value;
    }
  }
  return out;
}
```

In `servicesToToml` (lines 425-438), add a `web_search` branch:

```ts
function servicesToToml(services: ServicesConfig, rawServices: unknown): Record<string, unknown> {
  const out = cloneRecord(rawServices);
  if (services.moonshotSearch !== undefined) {
    out['moonshot_search'] = serviceToToml(services.moonshotSearch);
  } else {
    delete out['moonshot_search'];
  }
  if (services.moonshotFetch !== undefined) {
    out['moonshot_fetch'] = serviceToToml(services.moonshotFetch);
  } else {
    delete out['moonshot_fetch'];
  }
  if (services.webSearch !== undefined) {
    out['web_search'] = webSearchToToml(services.webSearch);
  } else {
    delete out['web_search'];
  }
  return out;
}

function webSearchToToml(cfg: WebSearchConfig): Record<string, unknown> {
  return {
    primary: webSearchProviderToToml(cfg.primary),
    secondary: cfg.secondary ? webSearchProviderToToml(cfg.secondary) : undefined,
  };
}

function webSearchProviderToToml(provider: WebSearchProviderConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(provider)) {
    if (key === 'options' && isPlainObject(value)) {
      out['options'] = transformRecord(value, (v) => v, camelToSnake);
    } else {
      setDefined(out, camelToSnake(key), value);
    }
  }
  return out;
}
```

Add `WebSearchConfig` and `WebSearchProviderConfig` to the `type` imports at the top of `toml.ts`:

```ts
import {
  KimiConfigSchema,
  formatConfigValidationError,
  getDefaultConfig,
  type BackgroundConfig,
  type BrowserConfig,
  type HookDefConfig,
  type KimiConfig,
  type LoopControl,
  type ModelAlias,
  type MoonshotServiceConfig,
  type OAuthRef,
  type PermissionConfig,
  type ProviderConfig,
  type ServicesConfig,
  type WebSearchConfig,
  type WebSearchProviderConfig,
  validateConfig,
} from '#/config/schema';
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test config/configs
```

Expected: existing tests and the new round-trip test pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/config/toml.ts packages/agent-core/test/config/configs.test.ts
git commit -m "feat(config): round-trip services.web_search in TOML read/write"
```

## Task 3: Backward-compatible `resolveWebSearchConfig`

**Depends on:** Task 2

**Files:**
- Create: `packages/agent-core/src/config/web-search.ts`
- Modify: `packages/agent-core/test/config/web-search.test.ts`
- Modify: `packages/agent-core/src/config/index.ts` (export the new helper)

- [ ] Write the failing test.

Append to `packages/agent-core/test/config/web-search.test.ts`:

```ts
import { resolveWebSearchConfig } from '../../src/config/web-search';
import type { KimiConfig } from '../../src/config/schema';

describe('resolveWebSearchConfig', () => {
  it('returns undefined when neither webSearch nor moonshotSearch is configured', () => {
    const config: KimiConfig = { providers: {} };
    expect(resolveWebSearchConfig(config)).toBeUndefined();
  });

  it('aliases moonshotSearch to a moonshot primary provider', () => {
    const config: KimiConfig = {
      providers: {},
      services: {
        moonshotSearch: { baseUrl: 'https://search.example/v1', apiKey: 'sk-moonshot' },
      },
    };
    const resolved = resolveWebSearchConfig(config);
    expect(resolved).toBeDefined();
    expect(resolved?.primary.provider).toBe('moonshot');
    expect(resolved?.primary.apiKey).toBe('sk-moonshot');
    expect(resolved?.primary.timeoutMs).toBe(25000);
  });

  it('gives webSearch precedence over moonshotSearch', () => {
    const config: KimiConfig = {
      providers: {},
      services: {
        moonshotSearch: { baseUrl: 'https://search.example/v1' },
        webSearch: { primary: { provider: 'exa' } },
      },
    };
    const resolved = resolveWebSearchConfig(config);
    expect(resolved?.primary.provider).toBe('exa');
  });

  it('preserves secondary provider from webSearch', () => {
    const config: KimiConfig = {
      providers: {},
      services: {
        webSearch: {
          primary: { provider: 'tavily' },
          secondary: { provider: 'duckduckgo' },
        },
      },
    };
    const resolved = resolveWebSearchConfig(config);
    expect(resolved?.secondary?.provider).toBe('duckduckgo');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test config/web-search
```

Expected failure: `Cannot find module '../../src/config/web-search'`.

- [ ] Write the minimal implementation.

```ts
// packages/agent-core/src/config/web-search.ts
import type { KimiConfig } from './schema';
import { type WebSearchConfig } from './schema';

export function resolveWebSearchConfig(config: KimiConfig): WebSearchConfig | undefined {
  if (config.services?.webSearch !== undefined) {
    return config.services.webSearch;
  }

  const moonshot = config.services?.moonshotSearch;
  if (moonshot === undefined) {
    return undefined;
  }

  return {
    primary: {
      provider: 'moonshot',
      apiKey: moonshot.apiKey,
      timeoutMs: 25000,
      options: {},
    },
  };
}
```

Export from `packages/agent-core/src/config/index.ts`. Open the file and add `resolveWebSearchConfig` to the exports (search existing exports and add alongside them):

```ts
export { resolveWebSearchConfig } from './web-search';
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test config/web-search
```

Expected: all 11 tests pass.

- [ ] Run a whole-tree typecheck to ensure the new schema types are consistent.

```bash
pnpm typecheck
```

Expected: no errors.

- [ ] Commit.

```bash
git add packages/agent-core/src/config/web-search.ts packages/agent-core/src/config/index.ts packages/agent-core/test/config/web-search.test.ts
git commit -m "feat(config): backward-compat resolver from moonshotSearch to webSearch"
```

## Local Self-Review (Part 1)

- [ ] No TODO/TBD placeholders in code or tests.
- [ ] Every task produced a verifiable change (schema, TOML transform, resolver) and a commit.
- [ ] Task dependencies are ordered: Task 1 → Task 2 → Task 3.
- [ ] Shared schema changes (`ServicesConfigSchema`, `ServicesConfigPatchSchema`) are confined to Task 1; no later task re-changes them.
- [ ] Every test asserts behavior (accept/reject/round-trip/alias precedence), not just compilation.
- [ ] Type names (`WebSearchConfig`, `WebSearchProviderConfig`, `WebSearchProviderName`) match across schema, TOML, resolver, and tests.
