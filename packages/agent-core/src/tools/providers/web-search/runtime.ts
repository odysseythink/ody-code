import { resolveWebSearchConfig } from '../../../config/web-search';
import type { KimiConfig } from '../../../config/schema';
import type { Logger } from '../../../logging/types';
import { FallbackWebSearchProvider } from './fallback';
import { createDefaultRegistry, type ProviderFactoryDeps } from './registry';
import type { WebSearchProvider } from './types';

export interface ResolveWebSearchRuntimeDeps extends ProviderFactoryDeps {
  logger?: Logger;
}

export function resolveWebSearchRuntime(
  config: KimiConfig,
  deps: ResolveWebSearchRuntimeDeps,
): WebSearchProvider | undefined {
  const webSearchConfig = resolveWebSearchConfig(config);
  if (webSearchConfig === undefined) return undefined;

  const registry = createDefaultRegistry();
  const resolvedDeps: ProviderFactoryDeps = {
    ...deps,
    moonshotServiceConfig: config.services?.moonshotSearch,
  };
  const primary = registry.create(webSearchConfig.primary, resolvedDeps);
  const secondary = webSearchConfig.secondary
    ? registry.create(webSearchConfig.secondary, resolvedDeps)
    : undefined;

  return new FallbackWebSearchProvider(primary, secondary, deps.logger ?? noopLogger);
}

const noopLogger: Logger = {
  debug: () => {},
  info: () => {},
  warn: () => {},
  error: () => {},
  createChild: () => noopLogger,
};
