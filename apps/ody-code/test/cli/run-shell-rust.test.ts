import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { buildRustHostLaunchOptions, resolveHostBinary, runSmokeTestBranch } from '#cli/run-shell-rust';
import { getHostBinaryPath } from '#native/native-assets';

vi.mock('#native/native-assets', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#native/native-assets')>()),
  getHostBinaryPath: vi.fn(),
}));

const mockedGetHostBinaryPath = vi.mocked(getHostBinaryPath);

describe('run-shell-rust helpers', () => {
  beforeEach(() => {
    mockedGetHostBinaryPath.mockReturnValue(null);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('defaults binary to ody-host when not provided and no embedded host exists', async () => {
    const binary = await resolveHostBinary({ hostBinary: undefined });
    expect(binary).toBe('ody-host');
    expect(mockedGetHostBinaryPath).toHaveBeenCalled();
  });

  it('uses the embedded native host binary when available', async () => {
    mockedGetHostBinaryPath.mockReturnValue('/cache/ody-host');
    const binary = await resolveHostBinary({ hostBinary: undefined });
    expect(binary).toBe('/cache/ody-host');
  });

  it('uses explicit hostBinary when provided', async () => {
    const binary = await resolveHostBinary({ hostBinary: '/my/custom/host' });
    expect(binary).toBe('/my/custom/host');
    expect(mockedGetHostBinaryPath).not.toHaveBeenCalled();
  });

  it('ignores empty hostBinary and falls back to default', async () => {
    const binary = await resolveHostBinary({ hostBinary: '' });
    expect(binary).toBe('ody-host');
    expect(mockedGetHostBinaryPath).toHaveBeenCalled();
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

  it('falls back to stdio when no socket or tcp is provided', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: false,
      hostSocket: undefined,
      hostTcp: undefined,
      hostBinary: '/tmp/ody-host',
    });
    expect(opts).toEqual({
      mode: 'stdio',
      binaryPath: '/tmp/ody-host',
    });
  });

  it('falls back to ody-host binary when hostBinary is omitted', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: true,
      hostSocket: undefined,
      hostTcp: undefined,
      hostBinary: undefined,
    });
    expect(opts).toEqual({
      mode: 'stdio',
      binaryPath: 'ody-host',
    });
  });

  it('builds socket options', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: false,
      hostSocket: '/tmp/test.sock',
      hostTcp: undefined,
      hostBinary: '/tmp/ody-host',
    });
    expect(opts).toEqual({
      mode: 'socket',
      binaryPath: '/tmp/ody-host',
      socketPath: '/tmp/test.sock',
    });
  });

  it('prefers socket over tcp when both are provided', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: false,
      hostSocket: '/tmp/test.sock',
      hostTcp: '127.0.0.1:9000',
      hostBinary: '/tmp/ody-host',
    });
    expect(opts).toEqual({
      mode: 'socket',
      binaryPath: '/tmp/ody-host',
      socketPath: '/tmp/test.sock',
    });
  });

  it('parses tcp host:port', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: false,
      hostSocket: undefined,
      hostTcp: '127.0.0.1:9000',
      hostBinary: '/tmp/ody-host',
      configPath: '/tmp/c.toml',
      homeDir: '/tmp/h',
    });
    expect(opts).toEqual({
      mode: 'tcp',
      binaryPath: '/tmp/ody-host',
      host: '127.0.0.1',
      port: 9000,
      configPath: '/tmp/c.toml',
      homeDir: '/tmp/h',
    });
  });
});

describe('runSmokeTestBranch', () => {
  it('prints SMOKE_OK and exits 0 on success', async () => {
    const harness = {
      ensureConfigFile: vi.fn().mockResolvedValue(undefined),
      getExperimentalFlags: vi.fn().mockResolvedValue({}),
      createSession: vi.fn().mockResolvedValue({ id: 'sess-ok' }),
      listSessions: vi.fn().mockResolvedValue([{ id: 'sess-ok' }]),
      close: vi.fn().mockResolvedValue(undefined),
    } as any;
    const stdout = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const exit = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);

    await runSmokeTestBranch(harness, { host: 'rust', hostStdio: true } as any);

    expect(stdout).toHaveBeenCalledWith('SMOKE_OK stdio sess-ok\n');
    expect(exit).toHaveBeenCalledWith(0);

    stdout.mockRestore();
    exit.mockRestore();
  });

  it('prints SMOKE_FAIL and exits 1 on failure', async () => {
    const harness = {
      ensureConfigFile: vi.fn().mockRejectedValue(new Error('boom')),
      close: vi.fn().mockResolvedValue(undefined),
    } as any;
    const stderr = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exit = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);

    await runSmokeTestBranch(harness, { host: 'rust', hostStdio: true } as any);

    expect(stderr).toHaveBeenCalledWith(expect.stringMatching(/^SMOKE_FAIL stdio: boom/));
    expect(exit).toHaveBeenCalledWith(1);

    stderr.mockRestore();
    exit.mockRestore();
  });
});
