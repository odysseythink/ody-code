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
      extraHeaders: { 'X-SearchApi-Source': 'ody-code' },
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
