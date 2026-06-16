import type { KimiConfig, WebSearchConfig } from './schema';

export function resolveWebSearchConfig(config: KimiConfig): WebSearchConfig | undefined {
  if (config.services?.webSearch !== undefined) {
    return config.services.webSearch;
  }

  const moonshot = config.services?.moonshotSearch;
  if (moonshot === undefined) {
    return undefined;
  }

  return {
    primary: {
      provider: 'moonshot',
      apiKey: moonshot.apiKey,
      timeoutMs: 25000,
      options: {},
    },
  };
}
