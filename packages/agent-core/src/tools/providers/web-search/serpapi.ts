import { buildUrl, getJson, httpError } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface SerpApiOptions {
  engine?: string;
  timeoutMs: number;
}

export class SerpApiProvider implements WebSearchProvider {
  readonly name = 'serpapi';

  constructor(
    private readonly apiKey: string,
    private readonly options: SerpApiOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const url = buildUrl('https://serpapi.com/search.json', {
      engine: this.options.engine ?? 'google',
      q: query,
      api_key: this.apiKey,
    });
    const response = await getJson(url, {
      fetchImpl: this.fetchImpl,
      timeoutMs: this.options.timeoutMs,
      provider: this.name,
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    return normalizeResults(selectSerpApiResults(data, this.options.engine ?? 'google'), this.name);
  }
}

function selectSerpApiResults(data: Record<string, unknown>, engine: string): unknown[] {
  const out: unknown[] = [];
  if (engine === 'google') {
    if (data.knowledge_graph) out.push(data.knowledge_graph);
    if (data.answer_box) out.push(data.answer_box);
    (data.organic_results as unknown[])?.forEach((r) => out.push(r));
  } else if (engine === 'baidu') {
    if (data.answer_box) out.push(data.answer_box);
    (data.organic_results as unknown[])?.forEach((r) => out.push(r));
  } else {
    (data.organic_results as unknown[])?.forEach((r) => out.push(r));
  }
  return out;
}
