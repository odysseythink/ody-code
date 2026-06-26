# Part 2: Provider Types, HTTP Helpers & Provider Implementations

## Task 4: Add `raw` to `WebSearchResult` and create shared normalization types

**Depends on:** Part 1 Task 3

**Files:**
- Modify: `packages/agent-core/src/tools/builtin/web/web-search.ts:21-27`
- Create: `packages/agent-core/src/tools/providers/web-search/types.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/types.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/types.test.ts
import { describe, expect, it } from 'vitest';
import {
  normalizeResult,
  normalizeResults,
} from '../../../src/tools/providers/web-search/types';

describe('normalizeResult', () => {
  it('extracts title, url and snippet from common upstream shapes', () => {
    const r = normalizeResult({ title: 'T', link: 'https://example.com', snippet: 'S' }, 'test');
    expect(r.title).toBe('T');
    expect(r.url).toBe('https://example.com');
    expect(r.snippet).toBe('S');
    expect(r.raw).toEqual({ title: 'T', link: 'https://example.com', snippet: 'S' });
  });

  it('falls back through url/link/uri and snippet/description/content/text', () => {
    const r = normalizeResult({ name: 'N', uri: 'https://x.com', content: 'C' }, 'test');
    expect(r.title).toBe('N');
    expect(r.url).toBe('https://x.com');
    expect(r.snippet).toBe('C');
  });

  it('truncates oversized fields defensively', () => {
    const r = normalizeResult(
      { title: 'x'.repeat(600), url: 'https://x.com/' + 'y'.repeat(3000), snippet: 'z'.repeat(5000) },
      'test',
    );
    expect(r.title.length).toBe(500);
    expect(r.url.length).toBe(2048);
    expect(r.snippet.length).toBe(4000);
  });
});

describe('normalizeResults', () => {
  it('drops results with empty title or url', () => {
    const out = normalizeResults(
      [
        { title: 'T', url: 'https://x.com', snippet: 'S' },
        { title: '', url: 'https://x.com', snippet: 'S' },
        { title: 'T', url: '', snippet: 'S' },
      ],
      'test',
    );
    expect(out).toHaveLength(1);
  });

  it('returns an empty array for non-array input', () => {
    expect(normalizeResults(null as unknown as unknown[], 'test')).toEqual([]);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/types
```

Expected failure: module not found for `types.ts`.

- [ ] Write the minimal implementation.

Add `raw?: unknown` to `WebSearchResult` in `packages/agent-core/src/tools/builtin/web/web-search.ts`:

```ts
export interface WebSearchResult {
  title: string;
  url: string;
  snippet: string;
  date?: string | undefined;
  content?: string | undefined;
  raw?: unknown;
}
```

Create `packages/agent-core/src/tools/providers/web-search/types.ts`:

```ts
export type { WebSearchProvider, WebSearchResult } from '../../builtin/web/web-search';

export function normalizeResult(raw: unknown, _provider: string): WebSearchResult {
  const r = raw as Record<string, unknown>;
  const title = String(r.title ?? r.name ?? '').slice(0, 500);
  const url = String(r.url ?? r.link ?? r.uri ?? '').slice(0, 2048);
  const snippet = String(r.snippet ?? r.description ?? r.content ?? r.text ?? '').slice(0, 4000);
  const result: WebSearchResult = { title, url, snippet, raw: r };
  if (typeof r.date === 'string' && r.date.length > 0) result.date = r.date;
  if (typeof r.content === 'string' && r.content.length > 0) result.content = r.content;
  return result;
}

export function normalizeResults(rawItems: unknown[], provider: string): WebSearchResult[] {
  if (!Array.isArray(rawItems)) return [];
  return rawItems
    .map((item) => normalizeResult(item, provider))
    .filter((r) => r.title.length > 0 && r.url.length > 0);
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/types
```

Expected: all 5 tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/builtin/web/web-search.ts packages/agent-core/src/tools/providers/web-search/types.ts packages/agent-core/test/tools/providers/web-search/types.test.ts
git commit -m "feat(tools): add raw field to WebSearchResult and normalize helpers"
```

## Task 5: Shared HTTP helpers

**Depends on:** Task 4

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/http.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/http.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/http.test.ts
import { describe, expect, it, vi } from 'vitest';
import {
  authHeaderForProvider,
  buildUrl,
  getJson,
  httpError,
  postJson,
} from '../../../src/tools/providers/web-search/http';

describe('buildUrl', () => {
  it('omits undefined params and stringifies values', () => {
    expect(buildUrl('https://api.example.com/search', { q: 'hello', limit: 10, skip: undefined })).toBe(
      'https://api.example.com/search?q=hello&limit=10',
    );
  });
});

describe('authHeaderForProvider', () => {
  it('returns Bearer for searchapi, baidu, perplexity', () => {
    expect(authHeaderForProvider('searchapi', 'k')).toEqual({ Authorization: 'Bearer k' });
    expect(authHeaderForProvider('baidu', 'k')).toEqual({
      Authorization: 'Bearer k',
      'X-Appbuilder-Authorization': 'Bearer k',
    });
    expect(authHeaderForProvider('perplexity', 'k')).toEqual({ Authorization: 'Bearer k' });
  });

  it('returns X-API-KEY for serper and serply', () => {
    expect(authHeaderForProvider('serper', 'k')).toEqual({ 'X-API-KEY': 'k' });
    expect(authHeaderForProvider('serply', 'k')).toEqual({ 'X-API-KEY': 'k' });
  });

  it('returns Ocp-Apim-Subscription-Key for bing', () => {
    expect(authHeaderForProvider('bing', 'k')).toEqual({ 'Ocp-Apim-Subscription-Key': 'k' });
  });

  it('returns lowercase x-api-key for exa', () => {
    expect(authHeaderForProvider('exa', 'k')).toEqual({ 'x-api-key': 'k' });
  });

  it('returns an empty object for providers that pass keys in query/body', () => {
    expect(authHeaderForProvider('serpapi', 'k')).toEqual({});
    expect(authHeaderForProvider('searxng', 'k')).toEqual({});
    expect(authHeaderForProvider('tavily', 'k')).toEqual({});
  });
});

describe('postJson', () => {
  it('times out after timeoutMs', async () => {
    const fetchImpl = vi.fn<typeof fetch>(() => new Promise(() => {}));
    const promise = postJson('https://api.example.com', { q: 'x' }, {
      fetchImpl,
      timeoutMs: 5,
      provider: 'tavily',
    });
    await expect(promise).rejects.toThrow(/timed out/i);
  });

  it('sends JSON body and default headers', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response('{}'));
    await postJson('https://api.example.com', { q: 'x' }, {
      fetchImpl,
      timeoutMs: 1000,
      apiKey: 'ak',
      provider: 'perplexity',
      toolCallId: 'tc1',
    });
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.method).toBe('POST');
    expect(init.body).toBe('{"q":"x"}');
    expect(init.headers).toMatchObject({
      'Content-Type': 'application/json',
      Authorization: 'Bearer ak',
      'X-Msh-Tool-Call-Id': 'tc1',
    });
  });
});

describe('httpError', () => {
  it('includes status and a JSON body detail', async () => {
    const response = new Response(JSON.stringify({ error: 'bad' }), { status: 500, statusText: 'Oops' });
    const err = await httpError(response, 'test');
    expect(err.message).toContain('HTTP 500');
    expect(err.message).toContain('bad');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/http
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/http.ts`:

```ts
export interface HttpProviderContext {
  fetchImpl: typeof fetch;
  timeoutMs: number;
  apiKey?: string;
  toolCallId?: string;
  provider: string;
}

export function buildUrl(base: string, params: Record<string, string | number | undefined>): string {
  const url = new URL(base);
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) {
      url.searchParams.set(key, String(value));
    }
  }
  return url.toString();
}

export function authHeaderForProvider(provider: string, apiKey: string): Record<string, string> {
  switch (provider) {
    case 'searchapi':
    case 'perplexity':
      return { Authorization: `Bearer ${apiKey}` };
    case 'baidu':
      return {
        Authorization: `Bearer ${apiKey}`,
        'X-Appbuilder-Authorization': `Bearer ${apiKey}`,
      };
    case 'serper':
    case 'serply':
      return { 'X-API-KEY': apiKey };
    case 'bing':
      return { 'Ocp-Apim-Subscription-Key': apiKey };
    case 'exa':
      return { 'x-api-key': apiKey };
    case 'serpapi':
    case 'searxng':
    case 'tavily':
    case 'duckduckgo':
    case 'moonshot':
    default:
      return {};
  }
}

export async function postJson(url: string, body: unknown, ctx: HttpProviderContext): Promise<Response> {
  return fetchWithTimeout(
    url,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(ctx.apiKey ? authHeaderForProvider(ctx.provider, ctx.apiKey) : {}),
        ...(ctx.toolCallId ? { 'X-Msh-Tool-Call-Id': ctx.toolCallId } : {}),
      },
      body: JSON.stringify(body),
    },
    ctx,
  );
}

export async function getJson(url: string, ctx: HttpProviderContext): Promise<Response> {
  return fetchWithTimeout(
    url,
    {
      method: 'GET',
      headers: {
        ...(ctx.apiKey ? authHeaderForProvider(ctx.provider, ctx.apiKey) : {}),
        ...(ctx.toolCallId ? { 'X-Msh-Tool-Call-Id': ctx.toolCallId } : {}),
      },
    },
    ctx,
  );
}

async function fetchWithTimeout(url: string, init: RequestInit, ctx: HttpProviderContext): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), ctx.timeoutMs);
  try {
    return await ctx.fetchImpl(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}

export async function httpError(response: Response, provider: string): Promise<Error> {
  let detail = '';
  try {
    const text = await response.text();
    detail = text.slice(0, 500);
  } catch {
    /* ignore */
  }
  return new Error(
    `${provider} search failed: HTTP ${String(response.status)} ${response.statusText}${detail ? `. ${detail}` : ''}`.trim(),
  );
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/http
```

Expected: all tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/http.ts packages/agent-core/test/tools/providers/web-search/http.test.ts
git commit -m "feat(tools): shared HTTP helpers for web search providers"
```

## Task 6: DuckDuckGo provider

**Depends on:** Task 5

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/duckduckgo.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/duckduckgo.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/duckduckgo.test.ts
import { describe, expect, it, vi } from 'vitest';
import { DuckDuckGoProvider } from '../../../src/tools/providers/web-search/duckduckgo';

const SAMPLE_HTML = `
<div class="result results_links">
  <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fone">Title One</a>
  <a class="result__snippet"><b>Snippet</b> one</a>
</div>
<div class="result results_links">
  <a class="result__a" href="https://example.com/two">Title Two</a>
  <a class="result__snippet">Snippet two</a>
</div>
`;

describe('DuckDuckGoProvider', () => {
  it('parses captured HTML into normalized results', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(SAMPLE_HTML, { status: 200 }));
    const provider = new DuckDuckGoProvider({}, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(2);
    expect(results[0]).toMatchObject({
      title: 'Title One',
      url: 'https://example.com/one',
      snippet: 'Snippet one',
    });
    expect(results[1]).toMatchObject({
      title: 'Title Two',
      url: 'https://example.com/two',
      snippet: 'Snippet two',
    });
  });

  it('throws on non-ok HTTP status', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response('err', { status: 503 }));
    const provider = new DuckDuckGoProvider({}, fetchImpl);
    await expect(provider.search('hello')).rejects.toThrow(/HTTP 503/);
  });

  it('respects proxyUrl when configured', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(SAMPLE_HTML, { status: 200 }));
    const provider = new DuckDuckGoProvider({ proxyUrl: 'http://proxy.example.com' }, fetchImpl);
    await provider.search('hello');
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://proxy.example.com',
      expect.objectContaining({
        method: 'GET',
        headers: expect.objectContaining({ 'X-Proxy-Url': 'https://html.duckduckgo.com/html?q=hello' }),
      }),
    );
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/duckduckgo
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/duckduckgo.ts`:

```ts
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface DuckDuckGoOptions {
  proxyUrl?: string;
}

export class DuckDuckGoProvider implements WebSearchProvider {
  readonly name = 'duckduckgo';

  constructor(
    private readonly options: DuckDuckGoOptions & { timeoutMs: number },
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const targetUrl = `https://html.duckduckgo.com/html?q=${encodeURIComponent(query)}`;
    const response = await this.fetchThroughProxy(targetUrl);
    if (!response.ok) {
      throw new Error(`DuckDuckGo search failed: HTTP ${String(response.status)}`);
    }
    const html = await response.text();
    const rawResults = parseDuckDuckGoHtml(html);
    return normalizeResults(rawResults, this.name);
  }

  private fetchThroughProxy(targetUrl: string): Promise<Response> {
    if (this.options.proxyUrl !== undefined) {
      return this.fetchImpl(this.options.proxyUrl, {
        method: 'GET',
        headers: {
          'X-Proxy-Url': targetUrl,
          'User-Agent': 'ody-code',
        },
      });
    }
    return this.fetchImpl(targetUrl, {
      method: 'GET',
      headers: { 'User-Agent': 'ody-code' },
    });
  }
}

function parseDuckDuckGoHtml(html: string): Array<{ title: string; link: string; snippet: string }> {
  const results: Array<{ title: string; link: string; snippet: string }> = [];
  const parts = html.split('<div class="result results_links');
  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];
    const titleMatch = part.match(/<a[^>]*class="result__a"[^>]*>(.*?)<\/a>/);
    const title = stripHtml(titleMatch?.[1] ?? '').trim();
    const hrefMatch = part.match(/<a[^>]*class="result__a"[^>]*href="([^"]*)"/);
    const link = hrefMatch ? extractDuckDuckGoRedirectUrl(hrefMatch[1]) : '';
    const snippetMatch = part.match(/<a[^>]*class="result__snippet"[^>]*>(.*?)<\/a>/);
    const snippet = stripHtml((snippetMatch?.[1] ?? '').replace(/<\/?b>/g, '')).trim();
    if (title && link && snippet) {
      results.push({ title, link, snippet });
    }
  }
  return results;
}

function extractDuckDuckGoRedirectUrl(href: string): string {
  let normalized = href;
  if (normalized.startsWith('//')) {
    normalized = `https:${normalized}`;
  }
  try {
    const url = new URL(normalized);
    const actual = url.searchParams.get('uddg');
    return actual ? decodeURIComponent(actual) : normalized;
  } catch {
    return normalized;
  }
}

function stripHtml(html: string): string {
  return html.replace(/<[^>]+>/g, '');
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/duckduckgo
```

Expected: all 3 tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/duckduckgo.ts packages/agent-core/test/tools/providers/web-search/duckduckgo.test.ts
git commit -m "feat(tools): DuckDuckGo web search provider"
```

## Task 7: SerpApi, SearchApi & Serper providers

**Depends on:** Task 5

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/serpapi.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/searchapi.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/serper.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/serp-search-serper.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/serp-search-serper.test.ts
import { describe, expect, it, vi } from 'vitest';
import { SerpApiProvider } from '../../../src/tools/providers/web-search/serpapi';
import { SearchApiProvider } from '../../../src/tools/providers/web-search/searchapi';
import { SerperProvider } from '../../../src/tools/providers/web-search/serper';

describe('SerpApiProvider', () => {
  it('maps Google organic results plus knowledge_graph and answer_box', async () => {
    const data = {
      knowledge_graph: { title: 'KG', link: 'https://kg.com', snippet: 'kg snippet' },
      answer_box: { title: 'AB', link: 'https://ab.com', snippet: 'ab snippet' },
      organic_results: [{ title: 'Organic', link: 'https://organic.com', snippet: 'organic snippet' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SerpApiProvider('key', { engine: 'google', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(3);
    expect(results.map((r) => r.title)).toEqual(['KG', 'AB', 'Organic']);
  });
});

describe('SearchApiProvider', () => {
  it('maps results with Authorization header and source tag', async () => {
    const data = {
      knowledge_graph: { description: { title: 'KG', link: 'https://kg.com', snippet: 'kg' } },
      answer_box: { answer: { title: 'AB', link: 'https://ab.com', snippet: 'ab' } },
      organic_results: [{ title: 'O', link: 'https://o.com', snippet: 'o' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SearchApiProvider('key', { engine: 'google', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(3);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ Authorization: 'Bearer key', 'X-SearchApi-Source': 'ody-code' });
  });
});

describe('SerperProvider', () => {
  it('maps knowledgeGraph and organic arrays', async () => {
    const data = {
      knowledgeGraph: { title: 'KG', link: 'https://kg.com', snippet: 'kg' },
      organic: [{ title: 'O', link: 'https://o.com', snippet: 'o' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SerperProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(2);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ 'X-API-KEY': 'key' });
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/serp-search-serper
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/serpapi.ts`:

```ts
import { buildUrl, getJson, httpError } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface SerpApiOptions {
  engine?: string;
  timeoutMs: number;
}

export class SerpApiProvider implements WebSearchProvider {
  readonly name = 'serpapi';

  constructor(
    private readonly apiKey: string,
    private readonly options: SerpApiOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const url = buildUrl('https://serpapi.com/search.json', {
      engine: this.options.engine ?? 'google',
      q: query,
      api_key: this.apiKey,
    });
    const response = await getJson(url, {
      fetchImpl: this.fetchImpl,
      timeoutMs: this.options.timeoutMs,
      provider: this.name,
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    return normalizeResults(selectSerpApiResults(data, this.options.engine ?? 'google'), this.name);
  }
}

function selectSerpApiResults(data: Record<string, unknown>, engine: string): unknown[] {
  const out: unknown[] = [];
  if (engine === 'google') {
    if (data.knowledge_graph) out.push(data.knowledge_graph);
    if (data.answer_box) out.push(data.answer_box);
    (data.organic_results as unknown[])?.forEach((r) => out.push(r));
  } else if (engine === 'baidu') {
    if (data.answer_box) out.push(data.answer_box);
    (data.organic_results as unknown[])?.forEach((r) => out.push(r));
  } else {
    (data.organic_results as unknown[])?.forEach((r) => out.push(r));
  }
  return out;
}
```

Create `packages/agent-core/src/tools/providers/web-search/searchapi.ts`:

```ts
import { buildUrl, getJson, httpError } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface SearchApiOptions {
  engine?: string;
  timeoutMs: number;
}

export class SearchApiProvider implements WebSearchProvider {
  readonly name = 'searchapi';

  constructor(
    private readonly apiKey: string,
    private readonly options: SearchApiOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const url = buildUrl('https://www.searchapi.io/api/v1/search', {
      engine: this.options.engine ?? 'google',
      q: query,
    });
    const response = await getJson(url, {
      fetchImpl: this.fetchImpl,
      timeoutMs: this.options.timeoutMs,
      apiKey: this.apiKey,
      provider: this.name,
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    const rawResults: unknown[] = [];
    if ((data.knowledge_graph as Record<string, unknown> | undefined)?.description) {
      rawResults.push((data.knowledge_graph as Record<string, unknown>).description);
    }
    if ((data.answer_box as Record<string, unknown> | undefined)?.answer) {
      rawResults.push((data.answer_box as Record<string, unknown>).answer);
    }
    (data.organic_results as unknown[])?.forEach((r) => rawResults.push(r));
    return normalizeResults(rawResults, this.name);
  }
}
```

Create `packages/agent-core/src/tools/providers/web-search/serper.ts`:

```ts
import { httpError, postJson } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface SerperOptions {
  timeoutMs: number;
}

export class SerperProvider implements WebSearchProvider {
  readonly name = 'serper';

  constructor(
    private readonly apiKey: string,
    private readonly options: SerperOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const response = await postJson(
      'https://google.serper.dev/search',
      { q: query },
      {
        fetchImpl: this.fetchImpl,
        timeoutMs: this.options.timeoutMs,
        apiKey: this.apiKey,
        provider: this.name,
      },
    );
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    const rawResults: unknown[] = [];
    if (data.knowledgeGraph) rawResults.push(data.knowledgeGraph);
    (data.organic as unknown[])?.forEach((r) => rawResults.push(r));
    return normalizeResults(rawResults, this.name);
  }
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/serp-search-serper
```

Expected: all 3 provider tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/serpapi.ts packages/agent-core/src/tools/providers/web-search/searchapi.ts packages/agent-core/src/tools/providers/web-search/serper.ts packages/agent-core/test/tools/providers/web-search/serp-search-serper.test.ts
git commit -m "feat(tools): SerpApi, SearchApi and Serper web search providers"
```

## Task 8: Bing, Baidu & Serply providers

**Depends on:** Task 5

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/bing.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/baidu.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/serply.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/bing-baidu-serply.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/bing-baidu-serply.test.ts
import { describe, expect, it, vi } from 'vitest';
import { BingProvider } from '../../../src/tools/providers/web-search/bing';
import { BaiduProvider } from '../../../src/tools/providers/web-search/baidu';
import { SerplyProvider } from '../../../src/tools/providers/web-search/serply';

describe('BingProvider', () => {
  it('maps webPages.value to normalized results', async () => {
    const data = {
      webPages: {
        value: [
          { name: 'N1', url: 'https://one.com', snippet: 'S1' },
          { name: 'N2', url: 'https://two.com', snippet: 'S2' },
        ],
      },
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new BingProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(2);
    expect(results[0]?.url).toBe('https://one.com');
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ 'Ocp-Apim-Subscription-Key': 'key' });
  });
});

describe('BaiduProvider', () => {
  it('maps references and deduplicates by URL', async () => {
    const data = {
      references: [
        { type: 'web', title: 'T', url: 'https://x.com', snippet: 'S' },
        { type: 'web', title: 'T2', url: 'https://x.com', snippet: 'S2' },
        { type: 'image', title: 'I', url: 'https://img.com', snippet: 'S' },
      ],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new BaiduProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    expect(results[0]?.url).toBe('https://x.com');
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.body).toContain('top_k');
  });

  it('throws on upstream error payload', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(JSON.stringify({ code: 'E1', message: 'bad' }), { status: 200 }),
    );
    const provider = new BaiduProvider('key', { timeoutMs: 1000 }, fetchImpl);
    await expect(provider.search('hello')).rejects.toThrow(/bad/);
  });
});

describe('SerplyProvider', () => {
  it('maps results and sets location/device headers', async () => {
    const data = { results: [{ title: 'T', link: 'https://x.com', snippet: 'S' }] };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SerplyProvider('key', { timeoutMs: 1000, gl: 'us', device: 'desktop' }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    const url = fetchImpl.mock.calls[0]?.[0] as string;
    expect(url).toContain('hl=us');
    expect(url).toContain('gl=US');
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ 'X-API-KEY': 'key', 'X-User-Agent': 'desktop' });
  });

  it('throws on Unauthorized payload', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(JSON.stringify({ message: 'Unauthorized' }), { status: 200 }),
    );
    const provider = new SerplyProvider('key', { timeoutMs: 1000 }, fetchImpl);
    await expect(provider.search('hello')).rejects.toThrow(/authentication/i);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/bing-baidu-serply
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/bing.ts`:

```ts
import { buildUrl, getJson, httpError } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface BingOptions {
  market?: string;
  timeoutMs: number;
}

export class BingProvider implements WebSearchProvider {
  readonly name = 'bing';

  constructor(
    private readonly apiKey: string,
    private readonly options: BingOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const url = buildUrl('https://api.bing.microsoft.com/v7.0/search', { q: query });
    const response = await getJson(url, {
      fetchImpl: this.fetchImpl,
      timeoutMs: this.options.timeoutMs,
      apiKey: this.apiKey,
      provider: this.name,
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    const pages = ((data.webPages as Record<string, unknown> | undefined)?.value ?? []) as Array<{
      name: string;
      url: string;
      snippet: string;
    }>;
    return normalizeResults(
      pages.map((p) => ({ title: p.name, url: p.url, snippet: p.snippet })),
      this.name,
    );
  }
}
```

Create `packages/agent-core/src/tools/providers/web-search/baidu.ts`:

```ts
import { httpError, postJson } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';

export interface BaiduOptions {
  topK?: number;
  timeoutMs: number;
}

export class BaiduProvider implements WebSearchProvider {
  readonly name = 'baidu';

  constructor(
    private readonly apiKey: string,
    private readonly options: BaiduOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const response = await postJson(
      'https://qianfan.baidubce.com/v2/ai_search/web_search',
      {
        messages: [{ role: 'user', content: query }],
        resource_type_filter: [{ type: 'web', top_k: this.options.topK ?? 10 }],
      },
      {
        fetchImpl: this.fetchImpl,
        timeoutMs: this.options.timeoutMs,
        apiKey: this.apiKey,
        provider: this.name,
      },
    );
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    if (data.code || (data.message && !data.references)) {
      throw new Error(`Baidu search error: ${String(data.message ?? data.code)}`);
    }
    const refs = (data.references ?? []) as unknown[];
    return normalizeBaiduReferences(refs);
  }
}

function normalizeBaiduReferences(refs: unknown[]): WebSearchResult[] {
  const seen = new Set<string>();
  const out: WebSearchResult[] = [];
  for (const ref of refs) {
    const r = ref as Record<string, unknown>;
    const type = String(r.type ?? r.resource_type ?? 'web').toLowerCase();
    if (type !== 'web') continue;
    const title = String(r.title ?? r.web_anchor ?? '').trim();
    const url = String(r.url ?? '').trim();
    const snippet = String(r.snippet ?? r.content ?? '').trim();
    if (!title || !url || seen.has(url)) continue;
    seen.add(url);
    out.push({ title, url, snippet, raw: r });
  }
  return out;
}
```

Create `packages/agent-core/src/tools/providers/web-search/serply.ts`:

```ts
import { buildUrl, getJson, httpError } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface SerplyOptions {
  language?: string;
  hl?: string;
  gl?: string;
  device?: 'desktop' | 'mobile';
  timeoutMs: number;
}

export class SerplyProvider implements WebSearchProvider {
  readonly name = 'serply';

  constructor(
    private readonly apiKey: string,
    private readonly options: SerplyOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const gl = (this.options.gl ?? 'US').toUpperCase();
    const url = buildUrl('https://api.serply.io/v1/search/', {
      q: query,
      language: this.options.language ?? 'en',
      hl: this.options.hl ?? 'us',
      gl,
    });
    const response = await getJson(url, {
      fetchImpl: this.fetchImpl,
      timeoutMs: this.options.timeoutMs,
      apiKey: this.apiKey,
      provider: this.name,
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    if (data.message === 'Unauthorized') throw new Error('Serply authentication failed');
    return normalizeResults((data.results ?? []) as unknown[], this.name);
  }
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/bing-baidu-serply
```

Expected: all tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/bing.ts packages/agent-core/src/tools/providers/web-search/baidu.ts packages/agent-core/src/tools/providers/web-search/serply.ts packages/agent-core/test/tools/providers/web-search/bing-baidu-serply.test.ts
git commit -m "feat(tools): Bing, Baidu and Serply web search providers"
```

## Task 9: SearXNG, Tavily, Exa & Perplexity providers

**Depends on:** Task 5

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/searxng.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/tavily.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/exa.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/perplexity.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/searxng-tavily-exa-perplexity.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/searxng-tavily-exa-perplexity.test.ts
import { describe, expect, it, vi } from 'vitest';
import { SearXNGProvider } from '../../../src/tools/providers/web-search/searxng';
import { TavilyProvider } from '../../../src/tools/providers/web-search/tavily';
import { ExaProvider } from '../../../src/tools/providers/web-search/exa';
import { PerplexityProvider } from '../../../src/tools/providers/web-search/perplexity';

describe('SearXNGProvider', () => {
  it('maps results from a configured baseUrl', async () => {
    const data = {
      results: [
        { title: 'T', url: 'https://x.com', content: 'C', publishedDate: '2024-01-01' },
      ],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SearXNGProvider({ baseUrl: 'https://searx.example.com', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    expect(results[0]?.date).toBe('2024-01-01');
    const url = fetchImpl.mock.calls[0]?.[0] as string;
    expect(url).toContain('https://searx.example.com');
    expect(url).toContain('format=json');
  });
});

describe('TavilyProvider', () => {
  it('posts api_key and search_depth', async () => {
    const data = { results: [{ title: 'T', url: 'https://x.com', snippet: 'S' }] };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new TavilyProvider('key', { searchDepth: 'advanced', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    const body = JSON.parse(init.body as string);
    expect(body).toMatchObject({ api_key: 'key', query: 'hello', search_depth: 'advanced' });
  });
});

describe('ExaProvider', () => {
  it('posts query, type, numResults and contents', async () => {
    const data = {
      results: [{ title: 'T', url: 'https://x.com', text: 'X', publishedDate: '2024-01-01' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new ExaProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello', { limit: 3, includeContent: true });
    expect(results).toHaveLength(1);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    const body = JSON.parse(init.body as string);
    expect(body).toMatchObject({ query: 'hello', numResults: 3, contents: { text: true } });
  });
});

describe('PerplexityProvider', () => {
  it('posts query with max_results and max_tokens_per_page', async () => {
    const data = { results: [{ title: 'T', url: 'https://x.com', snippet: 'S' }] };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new PerplexityProvider('key', { timeoutMs: 1000, maxResults: 7 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    const body = JSON.parse(init.body as string);
    expect(body).toMatchObject({ query: 'hello', max_results: 7, max_tokens_per_page: 2048 });
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/searxng-tavily-exa-perplexity
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/searxng.ts`:

```ts
import { buildUrl, getJson, httpError } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface SearXNGOptions {
  baseUrl: string;
  timeoutMs: number;
}

export class SearXNGProvider implements WebSearchProvider {
  readonly name = 'searxng';

  constructor(
    private readonly options: SearXNGOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const url = buildUrl(this.options.baseUrl, { q: query, format: 'json' });
    const response = await getJson(url, {
      fetchImpl: this.fetchImpl,
      timeoutMs: this.options.timeoutMs,
      provider: this.name,
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    const rawResults = (
      (data.results ?? []) as Array<{ title: string; url: string; content: string; publishedDate?: string }>
    ).map((r) => ({ title: r.title, link: r.url, snippet: r.content, date: r.publishedDate }));
    return normalizeResults(rawResults, this.name);
  }
}
```

Create `packages/agent-core/src/tools/providers/web-search/tavily.ts`:

```ts
import { httpError, postJson } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface TavilyOptions {
  searchDepth?: 'basic' | 'advanced';
  timeoutMs: number;
}

export class TavilyProvider implements WebSearchProvider {
  readonly name = 'tavily';

  constructor(
    private readonly apiKey: string,
    private readonly options: TavilyOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const response = await postJson(
      'https://api.tavily.com/search',
      {
        api_key: this.apiKey,
        query,
        search_depth: this.options.searchDepth ?? 'basic',
      },
      {
        fetchImpl: this.fetchImpl,
        timeoutMs: this.options.timeoutMs,
        provider: this.name,
      },
    );
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    return normalizeResults((data.results ?? []) as unknown[], this.name);
  }
}
```

Create `packages/agent-core/src/tools/providers/web-search/exa.ts`:

```ts
import { httpError, postJson } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface ExaOptions {
  type?: 'auto' | 'fast' | 'deep';
  livecrawl?: 'fallback' | 'preferred';
  timeoutMs: number;
}

export class ExaProvider implements WebSearchProvider {
  readonly name = 'exa';

  constructor(
    private readonly apiKey: string,
    private readonly options: ExaOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string, opts?: { limit?: number; includeContent?: boolean }): Promise<WebSearchResult[]> {
    const response = await postJson(
      'https://api.exa.ai/search',
      {
        query,
        type: this.options.type ?? 'auto',
        numResults: opts?.limit ?? 10,
        contents: { text: opts?.includeContent ?? false },
        livecrawl: this.options.livecrawl ?? 'fallback',
      },
      {
        fetchImpl: this.fetchImpl,
        timeoutMs: this.options.timeoutMs,
        apiKey: this.apiKey,
        provider: this.name,
      },
    );
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    const rawResults = (
      (data.results ?? []) as Array<{ title?: string; url: string; text?: string; publishedDate?: string }>
    ).map((r) => ({ title: r.title ?? '', link: r.url, snippet: r.text ?? '', date: r.publishedDate }));
    return normalizeResults(rawResults, this.name);
  }
}
```

Create `packages/agent-core/src/tools/providers/web-search/perplexity.ts`:

```ts
import { httpError, postJson } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface PerplexityOptions {
  maxResults?: number;
  maxTokensPerPage?: number;
  timeoutMs: number;
}

export class PerplexityProvider implements WebSearchProvider {
  readonly name = 'perplexity';

  constructor(
    private readonly apiKey: string,
    private readonly options: PerplexityOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const response = await postJson(
      'https://api.perplexity.ai/search',
      {
        query,
        max_results: this.options.maxResults ?? 5,
        max_tokens_per_page: this.options.maxTokensPerPage ?? 2048,
      },
      {
        fetchImpl: this.fetchImpl,
        timeoutMs: this.options.timeoutMs,
        apiKey: this.apiKey,
        provider: this.name,
      },
    );
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    return normalizeResults((data.results ?? []) as unknown[], this.name);
  }
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/searxng-tavily-exa-perplexity
```

Expected: all 4 provider tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/searxng.ts packages/agent-core/src/tools/providers/web-search/tavily.ts packages/agent-core/src/tools/providers/web-search/exa.ts packages/agent-core/src/tools/providers/web-search/perplexity.ts packages/agent-core/test/tools/providers/web-search/searxng-tavily-exa-perplexity.test.ts
git commit -m "feat(tools): SearXNG, Tavily, Exa and Perplexity web search providers"
```

## Task 10: `WebSearchProviderRegistry` and Moonshot adapter

**Depends on:** Task 6-9

**Files:**
- Create: `packages/agent-core/src/tools/providers/web-search/moonshot.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/registry.ts`
- Create: `packages/agent-core/src/tools/providers/web-search/index.ts`
- Create: `packages/agent-core/test/tools/providers/web-search/registry.test.ts`

- [ ] Write the failing test.

```ts
// packages/agent-core/test/tools/providers/web-search/registry.test.ts
import { describe, expect, it, vi } from 'vitest';
import { createDefaultRegistry } from '../../../src/tools/providers/web-search/registry';

describe('createDefaultRegistry', () => {
  it('creates a DuckDuckGo provider', () => {
    const registry = createDefaultRegistry();
    const provider = registry.create({ provider: 'duckduckgo', timeoutMs: 1000 }, { fetchImpl: vi.fn() });
    expect(provider.name).toBe('duckduckgo');
  });

  it('creates a Tavily provider with an apiKey', () => {
    const registry = createDefaultRegistry();
    const provider = registry.create(
      { provider: 'tavily', apiKey: 'sk-tavily', timeoutMs: 1000 },
      { fetchImpl: vi.fn() },
    );
    expect(provider.name).toBe('tavily');
  });

  it('rejects unknown provider names', () => {
    const registry = createDefaultRegistry();
    expect(() =>
      registry.create({ provider: 'unknown' as never, timeoutMs: 1000 }, { fetchImpl: vi.fn() }),
    ).toThrow(/Unknown web search provider: unknown/);
  });

  it('creates a Moonshot provider from the existing adapter', () => {
    const registry = createDefaultRegistry();
    const provider = registry.create(
      { provider: 'moonshot', timeoutMs: 1000 },
      {
        fetchImpl: vi.fn(),
        moonshotServiceConfig: { baseUrl: 'https://search.example/v1', apiKey: 'sk-moon' },
      },
    );
    expect(provider.name).toBe('moonshot');
  });

  it('throws when moonshot is requested without moonshotServiceConfig', () => {
    const registry = createDefaultRegistry();
    expect(() =>
      registry.create({ provider: 'moonshot', timeoutMs: 1000 }, { fetchImpl: vi.fn() }),
    ).toThrow(/moonshotSearch/);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/registry
```

Expected failure: module not found.

- [ ] Write the minimal implementation.

Create `packages/agent-core/src/tools/providers/web-search/moonshot.ts`:

```ts
import { MoonshotWebSearchProvider } from '../moonshot-web-search';
import type { WebSearchProvider } from './types';

export interface MoonshotProviderDeps {
  fetchImpl?: typeof fetch;
  kimiRequestHeaders?: Record<string, string>;
  resolveOAuthTokenProvider?: (provider: string, oauth: { storage: string; key: string }) => { getAccessToken(): Promise<string> };
  moonshotServiceConfig?: { baseUrl?: string; apiKey?: string; oauth?: { storage: string; key: string }; customHeaders?: Record<string, string> };
}

export function createMoonshotProvider(deps: MoonshotProviderDeps): WebSearchProvider {
  const config = deps.moonshotServiceConfig;
  if (config?.baseUrl === undefined) {
    throw new Error('Moonshot web search provider requires services.moonshotSearch.baseUrl');
  }
  const tokenProvider = config.oauth
    ? deps.resolveOAuthTokenProvider?.('managed:ody-code', config.oauth as { storage: string; key: string })
    : undefined;
  return new MoonshotWebSearchProvider({
    baseUrl: config.baseUrl,
    apiKey: config.apiKey,
    tokenProvider,
    defaultHeaders: deps.kimiRequestHeaders,
    customHeaders: config.customHeaders,
    fetchImpl: deps.fetchImpl,
  });
}
```

Create `packages/agent-core/src/tools/providers/web-search/registry.ts`:

```ts
import { z } from 'zod';

import type {
  WebSearchProviderConfig,
  WebSearchProviderName,
} from '../../../config/schema';
import { BaiduOptionsSchema, BingOptionsSchema, DuckDuckGoOptionsSchema, ExaOptionsSchema, PerplexityOptionsSchema, SearchApiOptionsSchema, SearXNGOptionsSchema, SerpApiOptionsSchema, SerperOptionsSchema, SerplyOptionsSchema, TavilyOptionsSchema } from '../../../config/schema';
import { BaiduProvider } from './baidu';
import { BingProvider } from './bing';
import { DuckDuckGoProvider } from './duckduckgo';
import { ExaProvider } from './exa';
import { createMoonshotProvider, type MoonshotProviderDeps } from './moonshot';
import { PerplexityProvider } from './perplexity';
import { SearchApiProvider } from './searchapi';
import { SearXNGProvider } from './searxng';
import { SerpApiProvider } from './serpapi';
import { SerperProvider } from './serper';
import { SerplyProvider } from './serply';
import { TavilyProvider } from './tavily';
import type { WebSearchProvider } from './types';

export interface ProviderFactoryDeps {
  fetchImpl?: typeof fetch;
  kimiRequestHeaders?: Record<string, string>;
  resolveOAuthTokenProvider?: (provider: string, oauth: { storage: string; key: string }) => { getAccessToken(): Promise<string> };
  moonshotServiceConfig?: { baseUrl?: string; apiKey?: string; oauth?: { storage: string; key: string }; customHeaders?: Record<string, string> };
}

export interface WebSearchProviderFactory {
  create(config: WebSearchProviderConfig, deps: ProviderFactoryDeps): WebSearchProvider;
}

export class WebSearchProviderRegistry {
  private readonly factories = new Map<WebSearchProviderName, WebSearchProviderFactory>();

  register(name: WebSearchProviderName, factory: WebSearchProviderFactory): void {
    this.factories.set(name, factory);
  }

  create(config: WebSearchProviderConfig, deps: ProviderFactoryDeps): WebSearchProvider {
    const factory = this.factories.get(config.provider);
    if (factory === undefined) {
      throw new Error(`Unknown web search provider: ${config.provider}`);
    }
    return factory.create(config, deps);
  }

  has(name: WebSearchProviderName): boolean {
    return this.factories.has(name);
  }
}

export function createDefaultRegistry(): WebSearchProviderRegistry {
  const registry = new WebSearchProviderRegistry();
  registry.register('duckduckgo', {
    create(config, deps) {
      const options = DuckDuckGoOptionsSchema.parse(config.options ?? {});
      return new DuckDuckGoProvider(
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('serpapi', {
    create(config, deps) {
      const options = SerpApiOptionsSchema.parse(config.options ?? {});
      return new SerpApiProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('searchapi', {
    create(config, deps) {
      const options = SearchApiOptionsSchema.parse(config.options ?? {});
      return new SearchApiProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('serper', {
    create(config, deps) {
      return new SerperProvider(
        config.apiKey ?? '',
        { timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('bing', {
    create(config, deps) {
      const options = BingOptionsSchema.parse(config.options ?? {});
      return new BingProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('baidu', {
    create(config, deps) {
      const options = BaiduOptionsSchema.parse(config.options ?? {});
      return new BaiduProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('serply', {
    create(config, deps) {
      const options = SerplyOptionsSchema.parse(config.options ?? {});
      return new SerplyProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('searxng', {
    create(config, deps) {
      const options = SearXNGOptionsSchema.parse(config.options ?? {});
      return new SearXNGProvider(
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('tavily', {
    create(config, deps) {
      const options = TavilyOptionsSchema.parse(config.options ?? {});
      return new TavilyProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('exa', {
    create(config, deps) {
      const options = ExaOptionsSchema.parse(config.options ?? {});
      return new ExaProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('perplexity', {
    create(config, deps) {
      const options = PerplexityOptionsSchema.parse(config.options ?? {});
      return new PerplexityProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('moonshot', {
    create(_config, deps) {
      return createMoonshotProvider(deps);
    },
  });
  return registry;
}
```

Create `packages/agent-core/src/tools/providers/web-search/index.ts`:

```ts
export { createMoonshotProvider } from './moonshot';
export {
  ProviderFactoryDeps,
  WebSearchProviderFactory,
  WebSearchProviderRegistry,
  createDefaultRegistry,
} from './registry';
export type { WebSearchProvider, WebSearchResult } from './types';
export { normalizeResult, normalizeResults } from './types';
```

- [ ] Run it and verify it PASSES.

```bash
pnpm --filter @odysseythink/agent-core test tools/providers/web-search/registry
```

Expected: all 5 tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/tools/providers/web-search/moonshot.ts packages/agent-core/src/tools/providers/web-search/registry.ts packages/agent-core/src/tools/providers/web-search/index.ts packages/agent-core/test/tools/providers/web-search/registry.test.ts
git commit -m "feat(tools): WebSearchProviderRegistry with 12 providers and Moonshot adapter"
```

## Local Self-Review (Part 2)

- [ ] No TODO/TBD placeholders in provider code or tests.
- [ ] Every task produced a verifiable change (types, HTTP helpers, 11 providers, registry) and a commit.
- [ ] Task dependencies are ordered: Task 4 → Task 5 → Tasks 6-9 → Task 10.
- [ ] Shared interface change (`WebSearchResult.raw`) is confined to Task 4; `WebSearchTool` is the only consumer and it only reads optional fields.
- [ ] Every provider has a behavioral test asserting request shape and result normalization.
- [ ] Provider names used in `registry.ts` match the `WebSearchProviderNameSchema` enum from Task 1.
- [ ] Timeout handling is consistent: each provider reads `options.timeoutMs` populated by its factory with `config.timeoutMs ?? 25000`.
