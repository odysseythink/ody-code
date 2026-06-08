import type { BuiltInContext, BuiltInMcpServerDefinition } from './registry';
import { resolveBuiltInRoot } from './resolve-root';

export function createChromeDevToolsServerDefinition(
  rootPath?: string | null,
): BuiltInMcpServerDefinition {
  return {
    name: 'chrome-devtools',
    displayName: 'Chrome DevTools',
    enabledByDefault: true,
    config: {
      transport: 'stdio',
      command: 'node',
      cwd: rootPath === null ? undefined : (rootPath ?? resolveBuiltInRoot('chrome-devtools')),
      startupTimeoutMs: 30_000,
      toolTimeoutMs: 60_000,
    },
    argsResolver: (ctx: BuiltInContext) => {
      const port = ctx.chromePort ?? 9222;
      return [
        './build/src/index.js',
        '--no-usage-statistics',
        '--no-performance-crux',
        `--browserUrl=http://127.0.0.1:${port}`,
      ];
    },
  };
}
