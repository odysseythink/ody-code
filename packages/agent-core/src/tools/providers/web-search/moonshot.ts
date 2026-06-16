import { MoonshotWebSearchProvider } from '../moonshot-web-search';
import type { WebSearchProvider } from './types';
import type { OAuthRef } from '../../../config/schema';

export interface MoonshotProviderDeps {
  fetchImpl?: typeof fetch;
  kimiRequestHeaders?: Record<string, string>;
  resolveOAuthTokenProvider?: (provider: string, oauth?: OAuthRef) => { getAccessToken(): Promise<string> } | undefined;
  moonshotServiceConfig?: { baseUrl?: string; apiKey?: string; oauth?: OAuthRef; customHeaders?: Record<string, string> };
}

export function createMoonshotProvider(deps: MoonshotProviderDeps): WebSearchProvider {
  const config = deps.moonshotServiceConfig;
  if (config?.baseUrl === undefined) {
    throw new Error('Moonshot web search provider requires services.moonshotSearch.baseUrl');
  }
  const tokenProvider = config.oauth
    ? deps.resolveOAuthTokenProvider?.('managed:ody-code', config.oauth)
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
