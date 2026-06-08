import { describe, expect, it } from 'vitest';
import { createChromeDevToolsServerDefinition } from '../../../src/mcp/built-in/chrome-devtools';

describe('createChromeDevToolsServerDefinition', () => {
  it('returns a stdio server definition with correct defaults', () => {
    const def = createChromeDevToolsServerDefinition('/mock/built-in/chrome-devtools');
    expect(def.name).toBe('chrome-devtools');
    expect(def.displayName).toBe('Chrome DevTools');
    expect(def.enabledByDefault).toBe(true);
    expect(def.config.transport).toBe('stdio');
    const cfg = def.config as Record<string, unknown>;
    expect(cfg['command']).toBe('node');
    expect(cfg['cwd']).toBe('/mock/built-in/chrome-devtools');
    expect(def.config.startupTimeoutMs).toBe(30_000);
    expect(def.config.toolTimeoutMs).toBe(60_000);
  });

  it('has no static args in config', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const cfg = def.config as Record<string, unknown>;
    expect(cfg['args']).toBeUndefined();
  });

  it('argsResolver disables telemetry and sets browserUrl with custom port', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const args = def.argsResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
      sessionId: 'session_abc123',
      chromePort: 9333,
    });
    expect(args).toEqual([
      './build/src/bin/chrome-devtools-mcp.js',
      '--no-usage-statistics',
      '--no-performance-crux',
      '--browserUrl=http://127.0.0.1:9333',
    ]);
  });

  it('argsResolver uses autoConnect when no chromePort is set', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const args = def.argsResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
      sessionId: 'session_abc123',
    });
    expect(args).toEqual([
      './build/src/bin/chrome-devtools-mcp.js',
      '--no-usage-statistics',
      '--no-performance-crux',
      '--autoConnect',
    ]);
  });

  it('has no envResolver', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    expect(def.envResolver).toBeUndefined();
  });
});
