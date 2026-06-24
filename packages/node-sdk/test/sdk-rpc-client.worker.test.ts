import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { SDKRpcClient } from '../src/rpc';

describe('SDKRpcClient worker mode', () => {
  it('spawns a worker thread and serves CoreAPI calls', async () => {
    const tmpDir = mkdtempSync(join(tmpdir(), 'ody-sdk-worker-'));
    const client = new SDKRpcClient({ worker: true, homeDir: tmpDir });
    const config = await client.getConfig();
    expect(config).toBeDefined();
    expect(client.homeDir).toBe(tmpDir);
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('falls back to in-process when worker is false', async () => {
    const client = new SDKRpcClient({ worker: false });
    expect(client.core).toBeDefined();
  });
});
