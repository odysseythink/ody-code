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
    const webPages = data['webPages'] as Record<string, unknown> | undefined;
    const pages = (webPages?.['value'] ?? []) as Array<{
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
