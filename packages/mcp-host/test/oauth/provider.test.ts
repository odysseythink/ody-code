import { describe, expect, it } from 'vitest';
import { JsonFileStore } from '../../src/oauth/store';
import { McpOAuthClientProvider } from '../../src/oauth/provider';

function makeProvider(): McpOAuthClientProvider {
  const store = new JsonFileStore('/tmp/ody-mcp-provider-test-' + Math.random().toString(36).slice(2));
  return new McpOAuthClientProvider({ serverName: 'srv', serverUrl: 'https://mcp.example/', store });
}

describe('McpOAuthClientProvider.state', () => {
  it('returns a 32-character hex string (16 bytes)', () => {
    const provider = makeProvider();
    const state = provider.state();
    expect(state).toMatch(/^[0-9a-f]{32}$/);
  });

  it('returns the same state on repeated calls', () => {
    const provider = makeProvider();
    expect(provider.state()).toBe(provider.state());
  });

  it('produces different states across providers', () => {
    const a = makeProvider().state();
    const b = makeProvider().state();
    expect(a).not.toBe(b);
  });
});
