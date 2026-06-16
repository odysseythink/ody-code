export interface HttpProviderContext {
  fetchImpl: typeof fetch;
  timeoutMs: number;
  apiKey?: string;
  toolCallId?: string;
  provider: string;
}

export function buildUrl(base: string, params: Record<string, string | number | undefined>): string {
  const url = new URL(base);
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) {
      url.searchParams.set(key, String(value));
    }
  }
  return url.toString();
}

export function authHeaderForProvider(provider: string, apiKey: string): Record<string, string> {
  switch (provider) {
    case 'searchapi':
    case 'perplexity':
      return { Authorization: `Bearer ${apiKey}` };
    case 'baidu':
      return {
        Authorization: `Bearer ${apiKey}`,
        'X-Appbuilder-Authorization': `Bearer ${apiKey}`,
      };
    case 'serper':
    case 'serply':
      return { 'X-API-KEY': apiKey };
    case 'bing':
      return { 'Ocp-Apim-Subscription-Key': apiKey };
    case 'exa':
      return { 'x-api-key': apiKey };
    case 'serpapi':
    case 'searxng':
    case 'tavily':
    case 'duckduckgo':
    case 'moonshot':
    default:
      return {};
  }
}

export async function postJson(url: string, body: unknown, ctx: HttpProviderContext): Promise<Response> {
  return fetchWithTimeout(
    url,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(ctx.apiKey ? authHeaderForProvider(ctx.provider, ctx.apiKey) : {}),
        ...(ctx.toolCallId ? { 'X-Msh-Tool-Call-Id': ctx.toolCallId } : {}),
      },
      body: JSON.stringify(body),
    },
    ctx,
  );
}

export async function getJson(url: string, ctx: HttpProviderContext): Promise<Response> {
  return fetchWithTimeout(
    url,
    {
      method: 'GET',
      headers: {
        ...(ctx.apiKey ? authHeaderForProvider(ctx.provider, ctx.apiKey) : {}),
        ...(ctx.toolCallId ? { 'X-Msh-Tool-Call-Id': ctx.toolCallId } : {}),
      },
    },
    ctx,
  );
}

async function fetchWithTimeout(url: string, init: RequestInit, ctx: HttpProviderContext): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), ctx.timeoutMs);
  try {
    return await ctx.fetchImpl(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}

export async function httpError(response: Response, provider: string): Promise<Error> {
  let detail = '';
  try {
    const text = await response.text();
    detail = text.slice(0, 500);
  } catch {
    /* ignore */
  }
  return new Error(
    `${provider} search failed: HTTP ${String(response.status)} ${response.statusText}${detail ? `. ${detail}` : ''}`.trim(),
  );
}
