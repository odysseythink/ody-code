# Provider Implementations

## 1. Shared Provider Interface [C:INFERRED]

```ts
export interface WebSearchProvider {
  readonly name: string;
  search(
    query: string,
    options?: { limit?: number; includeContent?: boolean; toolCallId?: string },
  ): Promise<WebSearchResult[]>;
}

export interface WebSearchResult {
  title: string;
  url: string;
  snippet: string;
  date?: string;
  content?: string;
  raw?: unknown;
}
```

`WebSearchResult` reuses the existing type in `packages/agent-core/src/tools/builtin/web/web-search.ts` with the addition of the optional `raw` field [C:USER].

## 2. Base HTTP Provider Helpers [C:INFERRED]

```ts
interface HttpProviderContext {
  fetchImpl: typeof fetch;
  timeoutMs: number;
  apiKey?: string;
  toolCallId?: string;
}

function buildUrl(base: string, params: Record<string, string | number | undefined>): string
  url := new URL(base)
  for each (key, value) in params
    if value !== undefined
      url.searchParams.set(key, String(value))
  return url.toString()

async function postJson(url: string, body: unknown, ctx: HttpProviderContext): Promise<Response>
  controller := new AbortController()
  timer := setTimeout(() => controller.abort(), ctx.timeoutMs)
  try
    return await ctx.fetchImpl(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(ctx.apiKey ? authHeaderForProvider(ctx.provider, ctx.apiKey) : {}),
        ...(ctx.toolCallId ? { 'X-Msh-Tool-Call-Id': ctx.toolCallId } : {}),
      },
      body: JSON.stringify(body),
      signal: controller.signal,
    })
  finally
    clearTimeout(timer)

async function getJson(url: string, ctx: HttpProviderContext): Promise<Response>
  // analogous to postJson with method GET and no body
```

`authHeaderForProvider(provider, apiKey)` returns the correct header object for each provider:

- `serpapi`: none (key is a query param)
- `searchapi`: `{ Authorization: 'Bearer ' + apiKey }`
- `serper`: `{ 'X-API-KEY': apiKey }`
- `bing`: `{ 'Ocp-Apim-Subscription-Key': apiKey }`
- `baidu`: `{ Authorization: 'Bearer ' + apiKey, 'X-Appbuilder-Authorization': 'Bearer ' + apiKey }`
- `serply`: `{ 'X-API-KEY': apiKey }`
- `searxng`: none
- `tavily`: none (key is in body)
- `exa`: `{ 'x-api-key': apiKey }`
- `perplexity`: `{ Authorization: 'Bearer ' + apiKey }`
- `moonshot`: handled by existing `MoonshotWebSearchProvider`

## 3. Result Normalization [C:USER]

```ts
function normalizeResult(raw: unknown, provider: string): WebSearchResult
  const r = raw as Record<string, unknown>
  return {
    title: String(r.title ?? r.name ?? '').slice(0, 500),
    url: String(r.url ?? r.link ?? r.uri ?? '').slice(0, 2048),
    snippet: String(r.snippet ?? r.description ?? r.content ?? r.text ?? '').slice(0, 4000),
    date: typeof r.date === 'string' && r.date.length > 0 ? r.date : undefined,
    content: typeof r.content === 'string' && r.content.length > 0 ? r.content : undefined,
    raw: r,
  }

function normalizeResults(rawItems: unknown[], provider: string): WebSearchResult[]
  if !Array.isArray(rawItems) return []
  return rawItems
    .map((item) => normalizeResult(item, provider))
    .filter((r) => r.title.length > 0 && r.url.length > 0)
```

## 4. Per-Provider Implementations

### 4.1 DuckDuckGoProvider [C:UPSTREAM]

Upstream reference: anything-llm `_duckDuckGoEngine`.

```ts
class DuckDuckGoProvider implements WebSearchProvider {
  readonly name = 'duckduckgo';
  constructor(private options: DuckDuckGoOptions, private fetchImpl = fetch) {}

  async search(query: string, opts): Promise<WebSearchResult[]> {
    const url = buildUrl('https://html.duckduckgo.com/html', { q: query });
    const response = await fetchThroughProxy(url, this.options.proxyUrl, this.fetchImpl);
    if (!response.ok) throw new Error(`DuckDuckGo search failed: HTTP ${response.status}`);
    const html = await response.text();
    const rawResults = parseDuckDuckGoHtml(html);
    return normalizeResults(rawResults, this.name);
  }
}
```

#### DuckDuckGo HTML parsing algorithm [C:UPSTREAM]

```
function parseDuckDuckGoHtml(html: string): Array<{ title, link, snippet }>
  results := []
  parts := html.split('<div class="result results_links')
  for i from 1 to parts.length - 1
    part := parts[i]

    titleMatch := part.match(/<a[^>]*class="result__a"[^>]*>(.*?)<\/a>/)
    title := stripHtml(titleMatch?.[1] ?? '').trim()

    hrefMatch := part.match(/<a[^>]*class="result__a"[^>]*href="([^"]*)"/)
    link := hrefMatch ? extractDuckDuckGoRedirectUrl(hrefMatch[1]) : ''

    snippetMatch := part.match(/<a[^>]*class="result__snippet"[^>]*>(.*?)<\/a>/)
    snippet := stripHtml(snippetMatch?.[1]?.replace(/<\/?b>/g, '') ?? '').trim()

    if title && link && snippet
      results.push({ title, link, snippet })
  return results

function extractDuckDuckGoRedirectUrl(href: string): string
  if href starts with '//'
    href := 'https:' + href
  try
    url := new URL(href)
    actual := url.searchParams.get('uddg')
    return actual ? decodeURIComponent(actual) : href
  catch
    return href
```

### 4.2 SerpApiProvider [C:UPSTREAM]

```ts
class SerpApiProvider implements WebSearchProvider {
  readonly name = 'serpapi';
  constructor(private apiKey: string, private options: SerpApiOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const url = buildUrl('https://serpapi.com/search.json', {
      engine: this.options.engine ?? 'google',
      q: query,
      api_key: this.apiKey,
    });
    const response = await this.fetchImpl(url);
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    const rawResults = selectSerpApiResults(data, this.options.engine ?? 'google');
    return normalizeResults(rawResults, this.name);
  }
}

function selectSerpApiResults(data: unknown, engine: string): unknown[] {
  const d = data as Record<string, unknown>;
  const out: unknown[] = [];
  if (engine === 'google') {
    if (d.knowledge_graph) out.push(d.knowledge_graph);
    if (d.answer_box) out.push(d.answer_box);
    (d.organic_results as unknown[])?.forEach((r) => out.push(r));
  } else if (engine === 'baidu') {
    if (d.answer_box) out.push(d.answer_box);
    (d.organic_results as unknown[])?.forEach((r) => out.push(r));
  } else {
    (d.organic_results as unknown[])?.forEach((r) => out.push(r));
  }
  return out;
}
```

### 4.3 SearchApiProvider [C:UPSTREAM]

```ts
class SearchApiProvider implements WebSearchProvider {
  readonly name = 'searchapi';
  constructor(private apiKey: string, private options: SearchApiOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const url = buildUrl('https://www.searchapi.io/api/v1/search', {
      engine: this.options.engine ?? 'google',
      q: query,
    });
    const response = await this.fetchImpl(url, {
      headers: {
        Authorization: `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
        'X-SearchApi-Source': 'ody-code',
      },
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    const rawResults: unknown[] = [];
    if (data.knowledge_graph?.description) rawResults.push(data.knowledge_graph.description);
    if (data.answer_box?.answer) rawResults.push(data.answer_box.answer);
    (data.organic_results as unknown[])?.forEach((r) => rawResults.push(r));
    return normalizeResults(rawResults, this.name);
  }
}
```

### 4.4 SerperProvider [C:UPSTREAM]

```ts
class SerperProvider implements WebSearchProvider {
  readonly name = 'serper';
  constructor(private apiKey: string, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const response = await this.fetchImpl('https://google.serper.dev/search', {
      method: 'POST',
      headers: {
        'X-API-KEY': this.apiKey,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ q: query }),
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    const rawResults: unknown[] = [];
    if (data.knowledgeGraph) rawResults.push(data.knowledgeGraph);
    (data.organic as unknown[])?.forEach((r) => rawResults.push(r));
    return normalizeResults(rawResults, this.name);
  }
}
```

### 4.5 BingProvider [C:UPSTREAM]

```ts
class BingProvider implements WebSearchProvider {
  readonly name = 'bing';
  constructor(private apiKey: string, private options: BingOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const url = buildUrl('https://api.bing.microsoft.com/v7.0/search', { q: query });
    const response = await this.fetchImpl(url, {
      headers: { 'Ocp-Apim-Subscription-Key': this.apiKey },
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    const pages = (data.webPages?.value ?? []) as Array<{ name: string; url: string; snippet: string }>;
    return normalizeResults(pages.map((p) => ({ title: p.name, url: p.url, snippet: p.snippet })), this.name);
  }
}
```

### 4.6 BaiduProvider [C:UPSTREAM]

```ts
class BaiduProvider implements WebSearchProvider {
  readonly name = 'baidu';
  constructor(private apiKey: string, private options: BaiduOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const response = await this.fetchImpl('https://qianfan.baidubce.com/v2/ai_search/web_search', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${this.apiKey}`,
        'X-Appbuilder-Authorization': `Bearer ${this.apiKey}`,
      },
      body: JSON.stringify({
        messages: [{ role: 'user', content: query }],
        resource_type_filter: [{ type: 'web', top_k: this.options.topK ?? 10 }],
      }),
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    if (data.code || (data.message && !data.references)) {
      throw new Error(`Baidu search error: ${data.message ?? data.code}`);
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

### 4.7 SerplyProvider [C:UPSTREAM]

```ts
class SerplyProvider implements WebSearchProvider {
  readonly name = 'serply';
  constructor(private apiKey: string, private options: SerplyOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const url = buildUrl('https://api.serply.io/v1/search/', {
      q: query,
      language: this.options.language ?? 'en',
      hl: this.options.hl ?? 'us',
      gl: (this.options.gl ?? 'US').toUpperCase(),
    });
    const response = await this.fetchImpl(url, {
      headers: {
        'X-API-KEY': this.apiKey,
        'Content-Type': 'application/json',
        'User-Agent': 'ody-code',
        'X-Proxy-Location': (this.options.gl ?? 'US').toUpperCase(),
        'X-User-Agent': this.options.device ?? 'desktop',
      },
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    if (data.message === 'Unauthorized') throw new Error('Serply authentication failed');
    return normalizeResults((data.results ?? []) as unknown[], this.name);
  }
}
```

### 4.8 SearXNGProvider [C:UPSTREAM]

```ts
class SearXNGProvider implements WebSearchProvider {
  readonly name = 'searxng';
  constructor(private options: SearXNGOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const url = buildUrl(this.options.baseUrl, { q: query, format: 'json' });
    const response = await this.fetchImpl(url, {
      headers: { 'Content-Type': 'application/json', 'User-Agent': 'ody-code' },
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    const rawResults = ((data.results ?? []) as Array<{ title: string; url: string; content: string; publishedDate?: string }>)
      .map((r) => ({ title: r.title, link: r.url, snippet: r.content, date: r.publishedDate }));
    return normalizeResults(rawResults, this.name);
  }
}
```

### 4.9 TavilyProvider [C:UPSTREAM]

```ts
class TavilyProvider implements WebSearchProvider {
  readonly name = 'tavily';
  constructor(private apiKey: string, private options: TavilyOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const response = await this.fetchImpl('https://api.tavily.com/search', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        api_key: this.apiKey,
        query,
        search_depth: this.options.searchDepth ?? 'basic',
      }),
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    return normalizeResults((data.results ?? []) as unknown[], this.name);
  }
}
```

### 4.10 ExaProvider [C:UPSTREAM]

```ts
class ExaProvider implements WebSearchProvider {
  readonly name = 'exa';
  constructor(private apiKey: string, private options: ExaOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const response = await this.fetchImpl('https://api.exa.ai/search', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'x-api-key': this.apiKey },
      body: JSON.stringify({
        query,
        type: this.options.type ?? 'auto',
        numResults: opts?.limit ?? 10,
        contents: { text: opts?.includeContent ?? false },
        livecrawl: this.options.livecrawl ?? 'fallback',
      }),
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    const rawResults = ((data.results ?? []) as Array<{ title?: string; url: string; text?: string; publishedDate?: string }>)
      .map((r) => ({ title: r.title ?? '', link: r.url, snippet: r.text ?? '', date: r.publishedDate }));
    return normalizeResults(rawResults, this.name);
  }
}
```

### 4.11 PerplexityProvider [C:UPSTREAM]

```ts
class PerplexityProvider implements WebSearchProvider {
  readonly name = 'perplexity';
  constructor(private apiKey: string, private options: PerplexityOptions, private fetchImpl = fetch) {}

  async search(query, opts): Promise<WebSearchResult[]> {
    const response = await this.fetchImpl('https://api.perplexity.ai/search', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${this.apiKey}` },
      body: JSON.stringify({
        query,
        max_results: this.options.maxResults ?? 5,
        max_tokens_per_page: this.options.maxTokensPerPage ?? 2048,
      }),
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = await response.json();
    return normalizeResults((data.results ?? []) as unknown[], this.name);
  }
}
```

### 4.12 MoonshotProvider (existing adapter) [C:INFERRED]

The existing `MoonshotWebSearchProvider` is reused. The registry wraps it so it conforms to the new `WebSearchProvider` interface.

```ts
function createMoonshotProvider(
  config: MoonshotServiceConfig,
  deps: { kimiRequestHeaders?: Record<string, string>; resolveOAuthTokenProvider? },
): WebSearchProvider {
  return new MoonshotWebSearchProvider({
    baseUrl: config.baseUrl!,
    apiKey: config.apiKey,
    tokenProvider: config.oauth ? deps.resolveOAuthTokenProvider?.('managed:ody-code', config.oauth) : undefined,
    defaultHeaders: deps.kimiRequestHeaders,
    customHeaders: config.customHeaders,
  });
}
```

## 5. Call-Site Integration

### 5.1 New source directory

Create `packages/agent-core/src/tools/providers/web-search/` [C:INFERRED]:

- `index.ts` — exports registry and provider classes.
- `types.ts` — shared interfaces and normalization helpers.
- `duckduckgo.ts`, `serpapi.ts`, `searchapi.ts`, `serper.ts`, `bing.ts`, `baidu.ts`, `serply.ts`, `searxng.ts`, `tavily.ts`, `exa.ts`, `perplexity.ts` — one file per provider.

### 5.2 Existing file changes

- `packages/agent-core/src/tools/providers/moonshot-web-search.ts` — no change; reused as-is.
- `packages/agent-core/src/tools/builtin/index.ts` — no change; `WebSearchTool` already exported.

## 6. Test Plan

| # | Test | Assertion |
|---|---|---|
| 1 | `normalizeResult` extracts title/url/snippet | `normalizeResult({ title: 'T', link: 'U', snippet: 'S' }).url === 'U'` |
| 2 | `normalizeResult` filters out empty title/url | `normalizeResults([{ title: '', url: '', snippet: '' }]).length === 0` |
| 3 | `DuckDuckGoProvider` parses captured HTML | `provider.search('test')` returns 3 results matching snapshot |
| 4 | `SerpApiProvider` maps Google organic results | mocked 200 JSON returns 2 normalized results with correct urls |
| 5 | `BaiduProvider` deduplicates by URL | mocked response with duplicate references returns 1 result |
| 6 | Provider without `apiKey` throws auth error on 401 | `rejects.toThrow(/authentication/i)` |
