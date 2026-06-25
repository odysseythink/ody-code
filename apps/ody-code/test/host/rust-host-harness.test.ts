import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

import { RustHostHarness } from '../../src/host/rust-host-harness';

function createMockClient() {
  return {
    homeDir: '/tmp/ody',
    configPath: '/tmp/ody/config.toml',
    interactiveAgentId: 'main',
    createSession: vi.fn().mockResolvedValue({ id: 's1', workDir: '/tmp', title: null }),
    resumeSession: vi.fn().mockResolvedValue({ id: 's1', workDir: '/tmp', title: null }),
    listSessions: vi.fn().mockResolvedValue([]),
    getExperimentalFlags: vi.fn().mockResolvedValue({}),
    getConfig: vi.fn().mockResolvedValue({ providers: [] }),
    setConfig: vi.fn().mockResolvedValue({ providers: [] }),
    removeProvider: vi.fn().mockResolvedValue({ providers: [] }),
    close: vi.fn().mockResolvedValue(undefined),
    onEvent: vi.fn().mockReturnValue(() => {}),
  } as unknown as import('@odysseythink/ody-code-sdk').SDKRpcClient;
}

describe('RustHostHarness', () => {
  it('delegates createSession and keeps a Session', async () => {
    const client = createMockClient();
    const harness = new RustHostHarness({ client, telemetry: { track: vi.fn(), withContext: vi.fn(), setContext: vi.fn() } });
    const session = await harness.createSession({ workDir: '/tmp' });
    expect(session.id).toBe('s1');
    expect(client.createSession).toHaveBeenCalledWith({ workDir: '/tmp' });
  });

  it('returns active session from resumeSession without re-creating', async () => {
    const client = createMockClient();
    const harness = new RustHostHarness({ client, telemetry: { track: vi.fn(), withContext: vi.fn(), setContext: vi.fn() } });
    const first = await harness.resumeSession({ id: 's1' });
    const second = await harness.resumeSession({ id: 's1' });
    expect(first.id).toBe('s1');
    expect(second.id).toBe('s1');
    expect(client.resumeSession).toHaveBeenCalledTimes(1);
  });

  it('proxies interactiveAgentId to the client', () => {
    const client = createMockClient();
    const harness = new RustHostHarness({ client, telemetry: { track: vi.fn(), withContext: vi.fn(), setContext: vi.fn() } });
    expect(harness.interactiveAgentId).toBe('main');
    harness.interactiveAgentId = 'worker-1';
    expect(client.interactiveAgentId).toBe('worker-1');
  });
});
