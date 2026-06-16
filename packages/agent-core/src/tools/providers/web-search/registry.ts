import type {
  WebSearchProviderConfig,
  WebSearchProviderName,
} from '../../../config/schema';
import type { OAuthRef } from '../../../config/schema';
import {
  BaiduOptionsSchema,
  BingOptionsSchema,
  DuckDuckGoOptionsSchema,
  ExaOptionsSchema,
  PerplexityOptionsSchema,
  SearchApiOptionsSchema,
  SearXNGOptionsSchema,
  SerpApiOptionsSchema,
  SerplyOptionsSchema,
  TavilyOptionsSchema,
} from '../../../config/schema';
import { BaiduProvider } from './baidu';
import { BingProvider } from './bing';
import { DuckDuckGoProvider } from './duckduckgo';
import { ExaProvider } from './exa';
import { createMoonshotProvider } from './moonshot';
import { PerplexityProvider } from './perplexity';
import { SearchApiProvider } from './searchapi';
import { SearXNGProvider } from './searxng';
import { SerpApiProvider } from './serpapi';
import { SerperProvider } from './serper';
import { SerplyProvider } from './serply';
import { TavilyProvider } from './tavily';
import type { WebSearchProvider } from './types';

export interface ProviderFactoryDeps {
  fetchImpl?: typeof fetch;
  kimiRequestHeaders?: Record<string, string>;
  resolveOAuthTokenProvider?: (provider: string, oauth?: OAuthRef) => { getAccessToken(): Promise<string> } | undefined;
  moonshotServiceConfig?: { baseUrl?: string; apiKey?: string; oauth?: OAuthRef; customHeaders?: Record<string, string> };
}

export interface WebSearchProviderFactory {
  create(config: WebSearchProviderConfig, deps: ProviderFactoryDeps): WebSearchProvider;
}

export class WebSearchProviderRegistry {
  private readonly factories = new Map<WebSearchProviderName, WebSearchProviderFactory>();

  register(name: WebSearchProviderName, factory: WebSearchProviderFactory): void {
    this.factories.set(name, factory);
  }

  create(config: WebSearchProviderConfig, deps: ProviderFactoryDeps): WebSearchProvider {
    const factory = this.factories.get(config.provider);
    if (factory === undefined) {
      throw new Error(`Unknown web search provider: ${config.provider}`);
    }
    return factory.create(config, deps);
  }

  has(name: WebSearchProviderName): boolean {
    return this.factories.has(name);
  }
}

export function createDefaultRegistry(): WebSearchProviderRegistry {
  const registry = new WebSearchProviderRegistry();
  registry.register('duckduckgo', {
    create(config, deps) {
      const options = DuckDuckGoOptionsSchema.parse(config.options ?? {});
      return new DuckDuckGoProvider(
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('serpapi', {
    create(config, deps) {
      const options = SerpApiOptionsSchema.parse(config.options ?? {});
      return new SerpApiProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('searchapi', {
    create(config, deps) {
      const options = SearchApiOptionsSchema.parse(config.options ?? {});
      return new SearchApiProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('serper', {
    create(config, deps) {
      return new SerperProvider(
        config.apiKey ?? '',
        { timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('bing', {
    create(config, deps) {
      const options = BingOptionsSchema.parse(config.options ?? {});
      return new BingProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('baidu', {
    create(config, deps) {
      const options = BaiduOptionsSchema.parse(config.options ?? {});
      return new BaiduProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('serply', {
    create(config, deps) {
      const options = SerplyOptionsSchema.parse(config.options ?? {});
      return new SerplyProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('searxng', {
    create(config, deps) {
      const options = SearXNGOptionsSchema.parse(config.options ?? {});
      return new SearXNGProvider(
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('tavily', {
    create(config, deps) {
      const options = TavilyOptionsSchema.parse(config.options ?? {});
      return new TavilyProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('exa', {
    create(config, deps) {
      const options = ExaOptionsSchema.parse(config.options ?? {});
      return new ExaProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('perplexity', {
    create(config, deps) {
      const options = PerplexityOptionsSchema.parse(config.options ?? {});
      return new PerplexityProvider(
        config.apiKey ?? '',
        { ...options, timeoutMs: config.timeoutMs ?? 25000 },
        deps.fetchImpl,
      );
    },
  });
  registry.register('moonshot', {
    create(_config, deps) {
      return createMoonshotProvider(deps);
    },
  });
  return registry;
}
