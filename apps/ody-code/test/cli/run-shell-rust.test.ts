import { describe, expect, it } from 'vitest';

import { buildRustHostLaunchOptions, resolveHostBinary } from '#/cli/run-shell-rust';

describe('run-shell-rust helpers', () => {
  it('defaults binary to ody-host when not provided', async () => {
    const binary = await resolveHostBinary({ hostBinary: undefined });
    expect(binary).toBe('ody-host');
  });

  it('builds stdio options', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: true,
      hostSocket: undefined,
      hostTcp: undefined,
      hostBinary: '/tmp/ody-host',
      configPath: '/tmp/c.toml',
      homeDir: '/tmp/h',
    });
    expect(opts).toEqual({
      mode: 'stdio',
      binaryPath: '/tmp/ody-host',
      configPath: '/tmp/c.toml',
      homeDir: '/tmp/h',
    });
  });

  it('parses tcp host:port', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: false,
      hostSocket: undefined,
      hostTcp: '127.0.0.1:9000',
      hostBinary: '/tmp/ody-host',
    });
    expect(opts).toEqual({
      mode: 'tcp',
      binaryPath: '/tmp/ody-host',
      host: '127.0.0.1',
      port: 9000,
    });
  });
});
