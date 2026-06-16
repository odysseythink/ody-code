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
