import { MoonshotWebSearchProvider } from '../moonshot-web-search';
import type { WebSearchProvider } from './types';

export interface MoonshotProviderDeps {
  fetchImpl?: typeof fetch;
  kimiRequestHeaders?: Record<string, string>;
  resolveOAuthTokenProvider?: (provider: string, oauth: { storage: string; key: string }) => { getAccessToken(): Promise<string> };
  moonshotServiceConfig?: { baseUrl?: string; apiKey?: string; oauth?: { storage: string; key: string }; customHeaders?: Record<string, string> };
}

export function createMoonshotProvider(deps: MoonshotProviderDeps): WebSearchProvider {
  const config = deps.moonshotServiceConfig;
  if (config?.baseUrl === undefined) {
    throw new Error('Moonshot web search provider requires services.moonshotSearch.baseUrl');
  }
  const tokenProvider = config.oauth
    ? deps.resolveOAuthTokenProvider?.('managed:ody-code', config.oauth as { storage: string; key: string })
    : undefined;
  const inner = new MoonshotWebSearchProvider({
    baseUrl: config.baseUrl,
    apiKey: config.apiKey,
    tokenProvider,
    defaultHeaders: deps.kimiRequestHeaders,
    customHeaders: config.customHeaders,
    fetchImpl: deps.fetchImpl,
  });
  return { name: 'moonshot', search: (q, opts) => inner.search(q, opts) };
}
