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
    expect(cfg['args']).toEqual(['./build/src/index.js']);
    expect(cfg['cwd']).toBe('/mock/built-in/chrome-devtools');
    expect(def.config.startupTimeoutMs).toBe(30_000);
    expect(def.config.toolTimeoutMs).toBe(60_000);
  });

  it('envResolver produces correct environment variables with custom port', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const env = def.envResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
      sessionId: 'session_abc123',
      chromePort: 9333,
    });
    expect(env).toEqual({
      CHROME_REMOTE_DEBUGGING_PORT: '9333',
      ODY_CODE_HOME: '/home/user/.ody-code',
      CDP_TRACE_DIR: '/home/user/.ody-code/sessions/session_abc123/chrome-traces',
    });
  });

  it('envResolver falls back to default port 9222', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const env = def.envResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
      sessionId: 'session_abc123',
    });
    expect(env?.['CHROME_REMOTE_DEBUGGING_PORT']).toBe('9222');
  });

  it('envResolver handles missing sessionId', () => {
    const def = createChromeDevToolsServerDefinition('/mock');
    const env = def.envResolver?.({
      kimiHomeDir: '/home/user/.ody-code',
    });
    expect(env?.['CDP_TRACE_DIR']).toBe('/home/user/.ody-code/sessions/unknown/chrome-traces');
  });
});
