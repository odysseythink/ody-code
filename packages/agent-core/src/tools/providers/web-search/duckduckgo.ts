import type { WebSearchProvider, WebSearchResult } from './types';
import { normalizeResults } from './types';

export interface DuckDuckGoOptions {
  proxyUrl?: string;
  timeoutMs: number;
}

export class DuckDuckGoProvider implements WebSearchProvider {
  readonly name = 'duckduckgo';

  constructor(
    private readonly options: DuckDuckGoOptions,
    private readonly fetchImpl: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}

  async search(query: string): Promise<WebSearchResult[]> {
    const targetUrl = `https://html.duckduckgo.com/html?q=${encodeURIComponent(query)}`;
    const response = await this.fetchThroughProxy(targetUrl);
    if (!response.ok) {
      throw new Error(`DuckDuckGo search failed: HTTP ${String(response.status)}`);
    }
    const html = await response.text();
    const rawResults = parseDuckDuckGoHtml(html);
    return normalizeResults(rawResults, this.name);
  }

  private fetchThroughProxy(targetUrl: string): Promise<Response> {
    if (this.options.proxyUrl !== undefined) {
      return this.fetchImpl(this.options.proxyUrl, {
        method: 'GET',
        headers: {
          'X-Proxy-Url': targetUrl,
          'User-Agent': 'ody-code',
        },
      });
    }
    return this.fetchImpl(targetUrl, {
      method: 'GET',
      headers: { 'User-Agent': 'ody-code' },
    });
  }
}

function parseDuckDuckGoHtml(html: string): Array<{ title: string; link: string; snippet: string }> {
  const results: Array<{ title: string; link: string; snippet: string }> = [];
  const parts = html.split('<div class="result results_links');
  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];
    const titleMatch = part.match(/<a[^>]*class="result__a"[^>]*>(.*?)<\/a>/);
    const title = stripHtml(titleMatch?.[1] ?? '').trim();
    const hrefMatch = part.match(/<a[^>]*class="result__a"[^>]*href="([^"]*)"/);
    const link = hrefMatch ? extractDuckDuckGoRedirectUrl(hrefMatch[1]) : '';
    const snippetMatch = part.match(/<a[^>]*class="result__snippet"[^>]*>(.*?)<\/a>/);
    const snippet = stripHtml((snippetMatch?.[1] ?? '').replace(/<\/?b>/g, '')).trim();
    if (title && link && snippet) {
      results.push({ title, link, snippet });
    }
  }
  return results;
}

function extractDuckDuckGoRedirectUrl(href: string): string {
  let normalized = href;
  if (normalized.startsWith('//')) {
    normalized = `https:${normalized}`;
  }
  try {
    const url = new URL(normalized);
    const actual = url.searchParams.get('uddg');
    return actual ? decodeURIComponent(actual) : normalized;
  } catch {
    return normalized;
  }
}

function stripHtml(html: string): string {
  return html.replace(/<[^>]+>/g, '');
}
