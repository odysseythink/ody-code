import { buildUrl, getJson, httpError } from './http';
import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface SerplyOptions {
  language?: string;
  hl?: string;
  gl?: string;
  device?: 'desktop' | 'mobile';
  timeoutMs: number;
}

export class SerplyProvider implements WebSearchProvider {
  readonly name = 'serply';

  constructor(
    private readonly apiKey: string,
    private readonly options: SerplyOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const gl = (this.options.gl ?? 'US').toUpperCase();
    const url = buildUrl('https://api.serply.io/v1/search/', {
      q: query,
      language: this.options.language ?? 'en',
      hl: this.options.hl ?? 'en',
      gl,
    });
    const response = await getJson(url, {
      fetchImpl: this.fetchImpl,
      timeoutMs: this.options.timeoutMs,
      apiKey: this.apiKey,
      provider: this.name,
      extraHeaders: {
        'X-User-Agent': this.options.device ?? 'desktop',
      },
    });
    if (!response.ok) throw await httpError(response, this.name);
    const data = (await response.json()) as Record<string, unknown>;
    if (data['message'] === 'Unauthorized') throw new Error('Serply authentication failed');
    return normalizeResults((data['results'] ?? []) as unknown[], this.name);
  }
}
