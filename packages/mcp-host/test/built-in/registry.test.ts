import { describe, expect, it } from 'vitest';
import { BuiltInMcpRegistry } from '#/built-in/registry';
import type { OdyConfig } from '@odysseythink/agent-core-shared';

describe('BuiltInMcpRegistry', () => {
  it('register then getEnabledConfigs returns the server', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'test-server',
      displayName: 'Test Server',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'echo' },
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home' },
      { providers: {} },
    );
    expect(configs).toHaveProperty('test-server');
    expect(configs['test-server']).toMatchObject({ transport: 'stdio', command: 'echo' });
  });

  it('isDisabled returns true when chrome-devtools is explicitly disabled', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'chrome-devtools',
      displayName: 'Chrome DevTools',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    const config: OdyConfig = { providers: {}, browser: { enabled: false } };
    expect(registry.isDisabled('chrome-devtools', config)).toBe(true);
  });

  it('isDisabled returns false for chrome-devtools by default', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'chrome-devtools',
      displayName: 'Chrome DevTools',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    const config: OdyConfig = { providers: {} };
    expect(registry.isDisabled('chrome-devtools', config)).toBe(false);
  });

  it('isDisabled returns true for unknown server names', () => {
    const registry = new BuiltInMcpRegistry();
    expect(registry.isDisabled('unknown', { providers: {} })).toBe(true);
  });

  it('envResolver merges env into base config env', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'test-server',
      displayName: 'Test',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node', env: { BASE: '1' } },
      envResolver: () => ({ EXTRA: '2' }),
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home' },
      { providers: {} },
    );
    expect((configs['test-server'] as Record<string, unknown>)['env']).toEqual({ BASE: '1', EXTRA: '2' });
  });

  it('getEnabledConfigs skips disabled servers', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'enabled-server',
      displayName: 'Enabled',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    registry.register({
      name: 'disabled-server',
      displayName: 'Disabled',
      enabledByDefault: false,
      config: { transport: 'stdio' as const, command: 'node' },
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home' },
      { providers: {} },
    );
    expect(Object.keys(configs)).toEqual(['enabled-server']);
  });

  it('chromePort is forwarded to envResolver via context', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'test-server',
      displayName: 'Test',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node' },
      envResolver: (ctx) => ({ PORT: String(ctx.chromePort ?? 9222) }),
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home', chromePort: 9333 },
      { providers: {} },
    );
    expect((configs['test-server'] as Record<string, unknown>)['env']).toEqual({ PORT: '9333' });
  });

  it('argsResolver overrides static config args', () => {
    const registry = new BuiltInMcpRegistry();
    registry.register({
      name: 'test-server',
      displayName: 'Test',
      enabledByDefault: true,
      config: { transport: 'stdio' as const, command: 'node', args: ['--default'] },
      argsResolver: (ctx) => ['--port', String(ctx.chromePort ?? 9222)],
    });
    const configs = registry.getEnabledConfigs(
      { kimiHomeDir: '/tmp/home', chromePort: 9333 },
      { providers: {} },
    );
    expect((configs['test-server'] as Record<string, unknown>)['args']).toEqual(['--port', '9333']);
  });
});
