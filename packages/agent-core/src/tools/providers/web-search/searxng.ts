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
      (data['results'] ?? []) as Array<{ title: string; url: string; content: string; publishedDate?: string }>
    ).map((r) => ({ title: r.title, link: r.url, snippet: r.content, date: r.publishedDate }));
    return normalizeResults(rawResults, this.name);
  }
}
