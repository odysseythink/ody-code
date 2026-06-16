export type { WebSearchProvider, WebSearchResult } from '../../builtin/web/web-search';

export function normalizeResult(raw: unknown, _provider: string): WebSearchResult {
  const r = raw as Record<string, unknown>;
  const title = String(r.title ?? r.name ?? '').slice(0, 500);
  const url = String(r.url ?? r.link ?? r.uri ?? '').slice(0, 2048);
  const snippet = String(r.snippet ?? r.description ?? r.content ?? r.text ?? '').slice(0, 4000);
  const result: WebSearchResult = { title, url, snippet, raw: r };
  if (typeof r.date === 'string' && r.date.length > 0) result.date = r.date;
  if (typeof r.content === 'string' && r.content.length > 0) result.content = r.content;
  return result;
}

export function normalizeResults(rawItems: unknown[], provider: string): WebSearchResult[] {
  if (!Array.isArray(rawItems)) return [];
  return rawItems
    .map((item) => normalizeResult(item, provider))
    .filter((r) => r.title.length > 0 && r.url.length > 0);
}
