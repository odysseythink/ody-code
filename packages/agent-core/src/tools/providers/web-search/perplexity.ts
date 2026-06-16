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
    return normalizeResults((data['results'] ?? []) as unknown[], this.name);
  }
}
