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
    const rawResults: Array<Record<string, unknown>> = [];
    const kg = data['knowledge_graph'] as Record<string, unknown> | undefined;
    if (kg !== undefined) {
      const description = kg['description'];
      if (description !== null && typeof description === 'object') {
        rawResults.push(description as Record<string, unknown>);
      } else if (typeof description === 'string' && description.length > 0) {
        rawResults.push({
          title: typeof kg['title'] === 'string' ? kg['title'] : typeof kg['name'] === 'string' ? kg['name'] : 'Knowledge Graph',
          link: typeof kg['website'] === 'string' ? kg['website'] : '',
          snippet: description,
        });
      }
    }
    const ab = data['answer_box'] as Record<string, unknown> | undefined;
    if (ab !== undefined) {
      const answer = ab['answer'];
      if (answer !== null && typeof answer === 'object') {
        rawResults.push(answer as Record<string, unknown>);
      } else if (typeof answer === 'string' && answer.length > 0) {
        rawResults.push({
          title: typeof ab['title'] === 'string' ? ab['title'] : 'Answer Box',
          link: typeof ab['link'] === 'string' ? ab['link'] : '',
          snippet: answer,
        });
      }
    }
    (data['organic_results'] as unknown[])?.forEach((r) => {
      if (r !== null && typeof r === 'object') {
        rawResults.push(r as Record<string, unknown>);
      }
    });
    return normalizeResults(rawResults, this.name);
  }
}
