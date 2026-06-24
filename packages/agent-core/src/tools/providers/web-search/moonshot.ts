import type { OAuthRef, WebSearchProviderConfig } from '@odysseythink/agent-core-shared';
import { MoonshotWebSearchProvider } from '../moonshot-web-search';
import type { WebSearchProvider } from './types';

export interface MoonshotProviderDeps {
  fetchImpl?: typeof fetch;
  kimiRequestHeaders?: Record<string, string>;
  resolveOAuthTokenProvider?: (provider: string, oauth?: OAuthRef) => { getAccessToken(): Promise<string> } | undefined;
  moonshotServiceConfig?: { baseUrl?: string; apiKey?: string; oauth?: OAuthRef; customHeaders?: Record<string, string> };
}

export function createMoonshotProvider(config: WebSearchProviderConfig, deps: MoonshotProviderDeps): WebSearchProvider {
  const options = (config.options ?? {}) as Record<string, unknown>;
  const baseUrl = options['baseUrl'] !== undefined ? String(options['baseUrl']) : deps.moonshotServiceConfig?.baseUrl;
  if (baseUrl === undefined) {
    throw new Error('Moonshot web search provider requires baseUrl (in services.web_search.primary.options or services.moonshotSearch.baseUrl)');
  }
  const apiKey = config.apiKey ?? (options['apiKey'] !== undefined ? String(options['apiKey']) : deps.moonshotServiceConfig?.apiKey);
  const oauth = (options['oauth'] as OAuthRef | undefined) ?? deps.moonshotServiceConfig?.oauth;
  const customHeadersFromOptions = options['customHeaders'] as Record<string, string> | undefined;
  const customHeaders = { ...deps.moonshotServiceConfig?.customHeaders, ...customHeadersFromOptions };
  const tokenProvider = oauth
    ? deps.resolveOAuthTokenProvider?.('managed:ody-code', oauth)
    : undefined;
  const inner = new MoonshotWebSearchProvider({
    baseUrl,
    apiKey,
    tokenProvider,
    defaultHeaders: deps.kimiRequestHeaders,
    customHeaders,
    fetchImpl: deps.fetchImpl,
  });
  return { name: 'moonshot', search: (q, opts) => inner.search(q, opts) };
}
