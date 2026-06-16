import type { WebSearchProvider, WebSearchResult } from '../../builtin/web/web-search';

export type { WebSearchProvider, WebSearchResult };

export function normalizeResult(raw: unknown, _provider: string): WebSearchResult {
  const r = raw as Record<string, unknown>;
  const title = typeof r['title'] === 'string' ? r['title'].slice(0, 500) : typeof r['name'] === 'string' ? r['name'].slice(0, 500) : '';
  const url = typeof r['url'] === 'string' ? r['url'].slice(0, 2048) : typeof r['link'] === 'string' ? r['link'].slice(0, 2048) : typeof r['uri'] === 'string' ? r['uri'].slice(0, 2048) : '';
  const snippet = typeof r['snippet'] === 'string' ? r['snippet'].slice(0, 4000) : typeof r['description'] === 'string' ? r['description'].slice(0, 4000) : typeof r['content'] === 'string' ? r['content'].slice(0, 4000) : typeof r['text'] === 'string' ? r['text'].slice(0, 4000) : '';
  const result: WebSearchResult = { title, url, snippet, raw: r };
  if (typeof r['date'] === 'string' && (r['date'] as string).length > 0) result.date = r['date'] as string;
  if (typeof r['content'] === 'string' && (r['content'] as string).length > 0) result.content = r['content'] as string;
  return result;
}

export function normalizeResults(rawItems: unknown[], provider: string): WebSearchResult[] {
  if (!Array.isArray(rawItems)) return [];
  return rawItems
    .map((item) => normalizeResult(item, provider))
    .filter((r) => r.title.length > 0 && r.url.length > 0);
}
