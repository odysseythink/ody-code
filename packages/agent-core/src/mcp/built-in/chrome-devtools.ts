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
      const args = [
        './build/src/bin/chrome-devtools-mcp.js',
        '--no-usage-statistics',
        '--no-performance-crux',
      ];
      const port = ctx.chromePort;
      if (port !== undefined) {
        args.push(`--browserUrl=http://127.0.0.1:${port}`);
      } else {
        args.push('--autoConnect');
      }
      return args;
    },
  };
}
