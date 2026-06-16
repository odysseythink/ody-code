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
    if (data['knowledgeGraph']) rawResults.push(data['knowledgeGraph']);
    (data['organic'] as unknown[])?.forEach((r) => rawResults.push(r));
    return normalizeResults(rawResults, this.name);
  }
}
