import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  createRPC,
  KimiCore,
  type CoreAPI,
  type SDKAPI,
  type ApprovalResponse,
} from '../../src';

describe('Built-in chrome-devtools MCP integration', () => {
  let tmp: string;

  afterEach(async () => {
    if (tmp !== undefined) {
      await rm(tmp, { recursive: true, force: true });
    }
  });

  it('injects chrome-devtools server config into new sessions', async () => {
    tmp = await mkdtemp(join(tmpdir(), 'kimi-core-built-in-'));
    const homeDir = join(tmp, 'home');
    const workDir = join(tmp, 'work');
    await mkdir(homeDir, { recursive: true });
    await mkdir(workDir, { recursive: true });

    const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
    const core = new KimiCore(coreRpc, { homeDir });
    const rpc = await sdkRpc({
      emitEvent: vi.fn(),
      requestApproval: vi.fn(
        async (): Promise<ApprovalResponse> => ({ decision: 'rejected' }),
      ),
      requestQuestion: vi.fn(async () => null),
      openExternal: vi.fn(async () => ({ opened: false })),
      toolCall: vi.fn(async () => ({ output: '' })),
    });

    const created = await rpc.createSession({
      id: 'ses_builtin_test',
      workDir,
    });
    const session = core.sessions.get(created.id);
    expect(session).toBeDefined();

    // Session ctor fire-and-forget's loadMcpServers; wait for it to finish.
    await session!.mcp.waitForInitialLoad();

    const entries = session!.mcp.list();
    const chromeDevTools = entries.find((e) => e.name === 'chrome-devtools');
    expect(chromeDevTools).toBeDefined();
    // In the test environment the vendored built-in directory usually does not
    // exist (tests run from packages/agent-core/test), so the server typically
    // lands in `failed`.  What matters is that the server was registered and
    // attempted to connect rather than being silently omitted.
    expect(['pending', 'connected', 'failed']).toContain(
      chromeDevTools!.status,
    );
  }, 30000);

  it('omits chrome-devtools when browser.enabled is false in config.toml', async () => {
    tmp = await mkdtemp(join(tmpdir(), 'kimi-core-built-in-'));
    const homeDir = join(tmp, 'home');
    const workDir = join(tmp, 'work');
    await mkdir(homeDir, { recursive: true });
    await mkdir(workDir, { recursive: true });
    await writeFile(
      join(homeDir, 'config.toml'),
      '[browser]\nenabled = false\n',
      'utf-8',
    );

    const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
    const core = new KimiCore(coreRpc, { homeDir });
    const rpc = await sdkRpc({
      emitEvent: vi.fn(),
      requestApproval: vi.fn(
        async (): Promise<ApprovalResponse> => ({ decision: 'rejected' }),
      ),
      requestQuestion: vi.fn(async () => null),
      openExternal: vi.fn(async () => ({ opened: false })),
      toolCall: vi.fn(async () => ({ output: '' })),
    });

    const created = await rpc.createSession({
      id: 'ses_builtin_disabled',
      workDir,
    });
    const session = core.sessions.get(created.id);
    expect(session).toBeDefined();

    await session!.mcp.waitForInitialLoad();

    const entries = session!.mcp.list();
    const chromeDevTools = entries.find((e) => e.name === 'chrome-devtools');
    expect(chromeDevTools).toBeUndefined();
  }, 30000);
});
