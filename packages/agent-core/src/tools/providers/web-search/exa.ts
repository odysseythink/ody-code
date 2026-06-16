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
