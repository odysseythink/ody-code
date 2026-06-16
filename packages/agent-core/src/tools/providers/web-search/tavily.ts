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
