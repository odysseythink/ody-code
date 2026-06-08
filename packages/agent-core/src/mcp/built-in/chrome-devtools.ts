import { join } from 'pathe';
import type { BuiltInContext, BuiltInMcpServerDefinition } from './registry';
import { resolveBuiltInRoot } from './resolve-root';

export function createChromeDevToolsServerDefinition(
  rootPath?: string,
): BuiltInMcpServerDefinition {
  return {
    name: 'chrome-devtools',
    displayName: 'Chrome DevTools',
    enabledByDefault: true,
    config: {
      transport: 'stdio',
      command: 'node',
      args: ['./build/src/index.js'],
      cwd: rootPath ?? resolveBuiltInRoot('chrome-devtools'),
      startupTimeoutMs: 30_000,
      toolTimeoutMs: 60_000,
    },
    envResolver: (ctx: BuiltInContext) => ({
      CHROME_REMOTE_DEBUGGING_PORT: String(ctx.chromePort ?? 9222),
      ODY_CODE_HOME: ctx.kimiHomeDir,
      CDP_TRACE_DIR: join(
        ctx.kimiHomeDir,
        'sessions',
        ctx.sessionId ?? 'unknown',
        'chrome-traces',
      ),
    }),
  };
}
